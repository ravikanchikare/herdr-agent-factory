import assert from "node:assert/strict"
import { generateKeyPairSync, verify } from "node:crypto"
import { mkdir, readFile, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { test } from "node:test"

import { temporaryDirectory } from "tempy"

import { publishRegistry } from "./index.mjs"

test("publishes a hashed and signed plugin catalog", async () => {
  const root = temporaryDirectory()
  const plugins = join(root, "plugins")
  const plugin = join(plugins, "verify")
  const output = join(root, "registry")
  const keyPath = join(root, "private.pem")
  const { privateKey, publicKey } = generateKeyPairSync("ed25519")

  await mkdir(join(plugin, "skills", "verify"), { recursive: true })
  await writeFile(
    join(plugin, "plugin.json"),
    JSON.stringify({
      $schema: "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
      name: "verify",
      version: "1.0.0",
    }),
  )
  await writeFile(
    join(plugin, "skills", "verify", "SKILL.md"),
    "---\nname: verify\ndescription: Verify the requested behavior.\n---\n\nVerify it.\n",
  )
  await writeFile(
    keyPath,
    privateKey.export({ type: "pkcs8", format: "pem" }),
  )

  const catalog = await publishRegistry({
    pluginsDirectory: plugins,
    outputDirectory: output,
    privateKeyPath: keyPath,
    baseUrl: "https://plugins.agentfactory.app/",
  })
  const bytes = await readFile(join(output, "catalog.json"))
  const signature = Buffer.from(
    await readFile(join(output, "catalog.sig"), "utf8"),
    "base64",
  )
  const publicKeyFile = await readFile(join(output, "public-key.b64"), "utf8")

  assert.equal(catalog.plugins.length, 1)
  assert.match(catalog.plugins[0].sha256, /^[a-f0-9]{64}$/)
  assert.equal(verify(null, bytes, publicKey, signature), true)
  // The published key is the raw 32-byte Ed25519 public key as base64 — the
  // exact format agent-factory's registry consumer expects.
  assert.equal(
    publicKeyFile,
    publicKey.export({ type: "spki", format: "der" }).subarray(-32).toString("base64"),
  )
  assert.equal(Buffer.from(publicKeyFile, "base64").length, 32)
})

test("rejects a plugin manifest that only passes shallow validation", async () => {
  const root = temporaryDirectory()
  const plugins = join(root, "plugins")
  const plugin = join(plugins, "invalid")
  const output = join(root, "registry")
  const keyPath = join(root, "private.pem")
  const { privateKey } = generateKeyPairSync("ed25519")

  await mkdir(plugin, { recursive: true })
  await writeFile(
    join(plugin, "plugin.json"),
    JSON.stringify({
      $schema: "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
      name: "invalid",
      version: "1.0.0",
      unexpected: true,
    }),
  )
  await writeFile(
    keyPath,
    privateKey.export({ type: "pkcs8", format: "pem" }),
  )

  await assert.rejects(
    publishRegistry({
      pluginsDirectory: plugins,
      outputDirectory: output,
      privateKeyPath: keyPath,
      baseUrl: "https://plugins.agentfactory.app/",
    }),
    /does not conform to its schema/,
  )
})

test("rejects invalid MCP entries before publication", async () => {
  const root = temporaryDirectory()
  const plugins = join(root, "plugins")
  const plugin = join(plugins, "unsafe")
  const output = join(root, "registry")
  const keyPath = join(root, "private.pem")
  const { privateKey } = generateKeyPairSync("ed25519")

  await mkdir(plugin, { recursive: true })
  await writeFile(
    join(plugin, "plugin.json"),
    JSON.stringify({
      $schema: "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
      name: "unsafe",
      version: "1.0.0",
    }),
  )
  await writeFile(
    join(plugin, "mcp.json"),
    JSON.stringify({
      $schema: "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
      mcpServers: {
        remote: { type: "streamable-http", url: "http://example.com/mcp" },
      },
    }),
  )
  await writeFile(
    keyPath,
    privateKey.export({ type: "pkcs8", format: "pem" }),
  )

  await assert.rejects(
    publishRegistry({
      pluginsDirectory: plugins,
      outputDirectory: output,
      privateKeyPath: keyPath,
      baseUrl: "https://plugins.agentfactory.app/",
    }),
    /unsafe remote MCP URL/,
  )
})
