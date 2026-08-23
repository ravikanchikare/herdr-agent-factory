import { spawnSync } from "node:child_process"
import { homedir } from "node:os"
import { delimiter, dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const pinnedZigVersion = "0.16.0"

export function nativeSdkEnvironment(source = process.env, home = homedir()) {
  const environment = { ...source }
  const pathKey =
    Object.keys(environment).find((key) => key.toLowerCase() === "path") ??
    "PATH"
  const sdkHome = source.NATIVE_SDK_HOME || join(home, ".native")
  const zigDirectory = source.NATIVE_SDK_ZIG
    ? dirname(resolve(source.NATIVE_SDK_ZIG))
    : join(sdkHome, "toolchains", `zig-${pinnedZigVersion}`)
  const currentPath = environment[pathKey] ?? ""
  const entries = currentPath.split(delimiter).filter(Boolean)
  const resolvedZigDirectory = resolve(zigDirectory)

  environment[pathKey] = [
    zigDirectory,
    ...entries.filter((entry) => resolve(entry) !== resolvedZigDirectory),
  ].join(delimiter)
  return environment
}

export function runNativeSdk(args = process.argv.slice(2)) {
  if (args.length === 0) {
    throw new Error(
      "Pass a Native SDK command such as `build`, `dev`, or `test`.",
    )
  }
  const nativeCli = fileURLToPath(
    new URL("../node_modules/@native-sdk/cli/bin/native.js", import.meta.url),
  )
  const result = spawnSync(process.execPath, [nativeCli, ...args], {
    cwd: process.cwd(),
    env: nativeSdkEnvironment(),
    stdio: "inherit",
  })
  if (result.error) throw result.error
  process.exitCode = result.status ?? 1
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  runNativeSdk()
}
