import { readFile } from "node:fs/promises"
import { resolve } from "node:path"

import Ajv2020 from "ajv/dist/2020.js"
import addFormats from "ajv-formats"

const root = resolve(import.meta.dirname, "..")
const ajv = new Ajv2020({ allErrors: true, strict: true })
addFormats(ajv)

async function readJson(path) {
  return JSON.parse(await readFile(resolve(root, path), "utf8"))
}

async function validate(schemaPath, documentPath) {
  const [schema, document] = await Promise.all([
    readJson(schemaPath),
    readJson(documentPath),
  ])
  const check = ajv.compile(schema)

  if (!check(document)) {
    const detail = ajv.errorsText(check.errors, { separator: "\n" })
    throw new Error(`${documentPath} failed schema validation:\n${detail}`)
  }

  return document
}

// Herdr owns agent detection, so there is no agent catalog to validate here.
//
// Environments are authored by users at runtime, so there is no fixture to validate
// either. `environment-runtime` compiles the schema into the binary and checks every
// descriptor as it loads, and its Harness references against the set Herdr
// reports — strictly more than a single fixture could cover.
for (const schemaPath of [
  "services/runtime/schema/repository-config-v1.schema.json",
  "crates/environment-runtime/schema/environment.schema.json",
  "plugins/schemas/1.0.0/plugin.schema.json",
  "plugins/schemas/1.0.0/mcp.schema.json",
  "tooling/registry-publisher/schema/catalog-v1.schema.json",
  "crates/update-runtime/schema/update-manifest.schema.json",
  "crates/update-runtime/schema/update-client-config-v1.schema.json",
]) {
  const schema = await readJson(schemaPath)
  ajv.compile(schema)
}

process.stdout.write(
  "Validated repository, environment, plugins, registry, and update schemas.\n",
)
