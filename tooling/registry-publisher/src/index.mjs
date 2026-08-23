#!/usr/bin/env node

import {
  createHash,
  createPrivateKey,
  createPublicKey,
  sign,
} from "node:crypto"
import {
  lstat,
  mkdir,
  readFile,
  readdir,
  realpath,
  writeFile,
} from "node:fs/promises"
import { basename, join, relative, resolve, sep } from "node:path"
import { pathToFileURL } from "node:url"

import Ajv2020 from "ajv/dist/2020.js"
import addFormats from "ajv-formats"
import * as tar from "tar"
import { parse as parseYaml } from "yaml"

const pluginSchema =
  "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json"
const maxFiles = 10_000
const maxExpandedBytes = 256 * 1024 * 1024
const semver =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/
const skillName = /^(?!.*--)[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/

async function schemaValidator(path) {
  const schema = JSON.parse(await readFile(path, "utf8"))
  const ajv = new Ajv2020({ allErrors: true, strict: true })
  addFormats(ajv)
  return ajv.compile(schema)
}

function validationError(context, validator) {
  const details = (validator.errors ?? [])
    .map((error) => `${error.instancePath || "/"} ${error.message}`)
    .join("; ")
  return new Error(`${context} does not conform to its schema: ${details}`)
}

function parseArguments(argv) {
  const values = new Map()

  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index]
    const value = argv[index + 1]
    if (!key?.startsWith("--") || value === undefined) {
      throw new Error("arguments must be --name value pairs")
    }
    values.set(key.slice(2), value)
  }

  for (const required of ["plugins-dir", "output", "private-key", "base-url"]) {
    if (!values.has(required)) {
      throw new Error(`missing --${required}`)
    }
  }

  return Object.fromEntries(values)
}

function inside(root, candidate) {
  const path = relative(root, candidate)
  return path === "" || (!path.startsWith(`..${sep}`) && path !== "..")
}

async function inspectTree(root) {
  const canonicalRoot = await realpath(root)
  const entries = []
  let expandedBytes = 0

  async function visit(directory) {
    const children = await readdir(directory, { withFileTypes: true })
    children.sort((left, right) => left.name.localeCompare(right.name))

    for (const child of children) {
      const path = join(directory, child.name)
      const metadata = await lstat(path)
      if (metadata.isSymbolicLink()) {
        throw new Error(`plugin contains a symbolic link: ${relative(root, path)}`)
      }
      if (!metadata.isDirectory() && !metadata.isFile()) {
        throw new Error(`plugin contains a special file: ${relative(root, path)}`)
      }

      const canonical = await realpath(path)
      if (!inside(canonicalRoot, canonical)) {
        throw new Error(`plugin path escapes its root: ${relative(root, path)}`)
      }

      entries.push(relative(root, path))
      if (entries.length > maxFiles) {
        throw new Error(`plugin contains more than ${maxFiles} entries`)
      }

      if (metadata.isDirectory()) {
        await visit(path)
      } else {
        expandedBytes += metadata.size
        if (expandedBytes > maxExpandedBytes) {
          throw new Error("plugin exceeds the expanded-size limit")
        }
      }
    }
  }

  await visit(root)
  return entries
}

async function readManifest(pluginRoot) {
  const manifest = JSON.parse(
    await readFile(join(pluginRoot, "plugin.json"), "utf8"),
  )

  return manifest
}

function insideLexically(root, candidate) {
  return inside(resolve(root), resolve(root, candidate))
}

function safeRelative(candidate) {
  return candidate !== "" && insideLexically("/plugin-data", candidate)
}

function validateMcpSemantics(pluginRoot, value) {
  for (const [name, server] of Object.entries(value.mcpServers)) {
    if (name.length === 0) {
      throw new Error(`${pluginRoot}/mcp.json has an empty MCP server name`)
    }

    if (server.type === "stdio") {
      if (server.command.includes("\0")) {
        throw new Error(`${pluginRoot}/mcp.json command contains a NUL byte`)
      }
      if (server.command.startsWith("./")) {
        const command = server.command.slice(2)
        if (!command || !insideLexically(pluginRoot, command)) {
          throw new Error(`${pluginRoot}/mcp.json has an escaping command`)
        }
      } else if (
        server.command.includes("/") ||
        server.command.includes("\\")
      ) {
        throw new Error(
          `${pluginRoot}/mcp.json command must be bare or plugin-relative`,
        )
      }

      const cwd = server.cwd
      if (cwd?.startsWith("./") && !insideLexically(pluginRoot, cwd.slice(2))) {
        throw new Error(`${pluginRoot}/mcp.json has an escaping cwd`)
      }
      if (
        cwd?.startsWith("${PLUGIN_ROOT}/") &&
        !insideLexically(pluginRoot, cwd.slice("${PLUGIN_ROOT}/".length))
      ) {
        throw new Error(`${pluginRoot}/mcp.json has an escaping PLUGIN_ROOT cwd`)
      }
      if (
        cwd?.startsWith("${PLUGIN_DATA}/") &&
        !safeRelative(cwd.slice("${PLUGIN_DATA}/".length))
      ) {
        throw new Error(`${pluginRoot}/mcp.json has an escaping PLUGIN_DATA cwd`)
      }
      continue
    }

    const url = new URL(server.url)
    const loopback =
      url.hostname === "localhost" ||
      url.hostname === "127.0.0.1" ||
      url.hostname === "[::1]"
    if (
      (url.protocol !== "https:" && !(url.protocol === "http:" && loopback)) ||
      url.username ||
      url.password ||
      url.hash
    ) {
      throw new Error(`${pluginRoot}/mcp.json has an unsafe remote MCP URL`)
    }

    const headerNames = new Set()
    for (const [header, headerValue] of Object.entries(server.headers ?? {})) {
      const normalized = header.toLowerCase()
      if (headerNames.has(normalized)) {
        throw new Error(
          `${pluginRoot}/mcp.json has a duplicate case-insensitive header`,
        )
      }
      headerNames.add(normalized)
      try {
        new Headers([[header, headerValue]])
      } catch {
        throw new Error(`${pluginRoot}/mcp.json has an invalid HTTP header`)
      }
    }
  }
}

async function validateComponents(pluginRoot, validateMcp) {
  const mcpPath = join(pluginRoot, "mcp.json")
  try {
    const mcp = JSON.parse(await readFile(mcpPath, "utf8"))
    if (!validateMcp(mcp)) {
      throw validationError(mcpPath, validateMcp)
    }
    validateMcpSemantics(pluginRoot, mcp)
  } catch (error) {
    if (error?.code !== "ENOENT") {
      throw error
    }
  }

  const skillsRoot = join(pluginRoot, "skills")
  let directories
  try {
    directories = (await readdir(skillsRoot, { withFileTypes: true })).filter(
      (entry) => entry.isDirectory(),
    )
  } catch (error) {
    if (error?.code === "ENOENT") {
      return
    }
    throw error
  }

  const names = new Set()
  for (const directory of directories) {
    const text = await readFile(
      join(skillsRoot, directory.name, "SKILL.md"),
      "utf8",
    )
    const match = text.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/)
    if (!match) {
      throw new Error(`${directory.name}/SKILL.md requires YAML frontmatter`)
    }
    const frontmatter = parseYaml(match[1])
    const name = frontmatter?.name
    const description = frontmatter?.description
    if (
      typeof name !== "string" ||
      !skillName.test(name) ||
      name !== directory.name
    ) {
      throw new Error(
        `${directory.name}/SKILL.md name must match its valid directory name`,
      )
    }
    if (
      typeof description !== "string" ||
      description.length === 0 ||
      [...description].length > 1024
    ) {
      throw new Error(
        `${directory.name}/SKILL.md description must contain 1-1024 characters`,
      )
    }
    if (names.has(name)) {
      throw new Error(`duplicate skill name ${name}`)
    }
    names.add(name)
  }
}

function safeArtifactName(name, version) {
  const safeName = name.toLowerCase().replace(/[^a-z0-9.-]+/g, "-")
  const safeVersion = version.replace(/[^a-zA-Z0-9.-]+/g, "-")
  return `${safeName}-${safeVersion}.tar.gz`
}

export async function publishRegistry(options) {
  const pluginsDirectory = resolve(options.pluginsDirectory)
  const outputDirectory = resolve(options.outputDirectory)
  const artifactsDirectory = join(outputDirectory, "artifacts")
  const baseUrl = new URL(options.baseUrl)
  if (
    baseUrl.protocol !== "https:" ||
    baseUrl.username ||
    baseUrl.password ||
    baseUrl.hash
  ) {
    throw new Error("baseUrl must be an HTTPS URL without credentials or a fragment")
  }
  const privateKey = await readFile(resolve(options.privateKeyPath), "utf8")
  const privateKeyObject = createPrivateKey(privateKey)
  // Raw 32-byte Ed25519 public key as base64 — the trust anchor a registry
  // consumer pastes. Published next to the catalog so a preview can auto-fill
  // it; the consumer still verifies it out of band (a registry can vouch for
  // itself).
  const publicKeyBase64 = createPublicKey(privateKeyObject)
    .export({ type: "spki", format: "der" })
    .subarray(-32)
    .toString("base64")
  const schemasRoot = new URL("../../../plugins/schemas/1.0.0/", import.meta.url)
  const [validateManifest, validateMcp, validateCatalog] = await Promise.all([
    schemaValidator(new URL("plugin.schema.json", schemasRoot)),
    schemaValidator(new URL("mcp.schema.json", schemasRoot)),
    schemaValidator(new URL("../schema/catalog-v1.schema.json", import.meta.url)),
  ])
  const directories = (await readdir(pluginsDirectory, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .sort((left, right) => left.name.localeCompare(right.name))

  await mkdir(artifactsDirectory, { recursive: true })

  const plugins = []
  const identities = new Set()
  const artifactNames = new Set()
  for (const directory of directories) {
    const pluginRoot = join(pluginsDirectory, directory.name)
    const [manifest] = await Promise.all([
      readManifest(pluginRoot),
      inspectTree(pluginRoot),
    ])
    if (!validateManifest(manifest)) {
      throw validationError(join(pluginRoot, "plugin.json"), validateManifest)
    }
    if (manifest.$schema !== pluginSchema) {
      throw new Error(`${pluginRoot} targets an unsupported Agent Plugins schema`)
    }
    if (directory.name !== manifest.name) {
      throw new Error(
        `${pluginRoot} directory must match manifest name ${manifest.name}`,
      )
    }
    if (!manifest.version || !semver.test(manifest.version)) {
      throw new Error(`${pluginRoot} requires a valid semantic version`)
    }
    await validateComponents(pluginRoot, validateMcp)
    const archiveEntries = await readdir(pluginRoot)
    archiveEntries.sort((left, right) => left.localeCompare(right))
    const version = manifest.version
    const artifactName = safeArtifactName(manifest.name, version)
    const identity = `${manifest.name}@${version}`
    if (identities.has(identity)) {
      throw new Error(`duplicate registry plugin identity ${identity}`)
    }
    if (artifactNames.has(artifactName)) {
      throw new Error(`duplicate registry artifact name ${artifactName}`)
    }
    identities.add(identity)
    artifactNames.add(artifactName)
    const artifactPath = join(artifactsDirectory, artifactName)

    await tar.create(
      {
        cwd: pluginRoot,
        file: artifactPath,
        gzip: true,
        portable: true,
        mtime: new Date(0),
      },
      archiveEntries,
    )

    const archive = await readFile(artifactPath)
    plugins.push({
      id: basename(directory.name),
      name: manifest.name,
      version,
      description: manifest.description ?? null,
      archiveUrl: new URL(`artifacts/${artifactName}`, baseUrl).href,
      sha256: createHash("sha256").update(archive).digest("hex"),
    })
  }

  const catalog = {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    plugins,
  }
  if (!validateCatalog(catalog)) {
    throw validationError("generated catalog", validateCatalog)
  }
  const bytes = Buffer.from(`${JSON.stringify(catalog, null, 2)}\n`)
  const signature = sign(null, bytes, privateKeyObject)

  await Promise.all([
    writeFile(join(outputDirectory, "catalog.json"), bytes),
    writeFile(join(outputDirectory, "catalog.sig"), signature.toString("base64")),
    writeFile(join(outputDirectory, "public-key.b64"), publicKeyBase64),
  ])

  return catalog
}

async function main() {
  const args = parseArguments(process.argv.slice(2))
  const catalog = await publishRegistry({
    pluginsDirectory: args["plugins-dir"],
    outputDirectory: args.output,
    privateKeyPath: args["private-key"],
    baseUrl: args["base-url"],
  })
  process.stdout.write(`Published ${catalog.plugins.length} plugins.\n`)
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main()
}
