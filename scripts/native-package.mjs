import { accessSync, constants } from "node:fs"
import { homedir } from "node:os"
import { delimiter, join } from "node:path"
import { spawnSync } from "node:child_process"

import { nativeSdkEnvironment } from "./native-sdk-command.mjs"

function executable(path) {
  try {
    accessSync(path, constants.X_OK)
    return true
  } catch {
    return false
  }
}

function pinnedZig(path) {
  if (!executable(path)) return false
  const version = spawnSync(path, ["version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  })
  return version.status === 0 && version.stdout.trim() === "0.16.0"
}

function resolveZig() {
  if (process.env.NATIVE_SDK_ZIG) {
    if (!pinnedZig(process.env.NATIVE_SDK_ZIG)) {
      throw new Error("NATIVE_SDK_ZIG must point to executable Zig 0.16.0.")
    }
    return process.env.NATIVE_SDK_ZIG
  }

  const sdkHome = process.env.NATIVE_SDK_HOME ?? join(homedir(), ".native")
  const managed = join(sdkHome, "toolchains", "zig-0.16.0", "zig")
  if (pinnedZig(managed)) {
    return managed
  }

  for (const directory of (process.env.PATH ?? "").split(delimiter)) {
    const candidate = join(directory, "zig")
    if (directory && pinnedZig(candidate)) {
      return candidate
    }
  }

  throw new Error(
    "Native-SDK Zig 0.16.0 was not found. Run `pnpm exec native build " +
      "apps/native-host --yes` once or set NATIVE_SDK_ZIG.",
  )
}

const target = process.env.NATIVE_PACKAGE_TARGET
if (target && !["aarch64-macos", "x86_64-macos"].includes(target)) {
  throw new Error(
    "NATIVE_PACKAGE_TARGET must be aarch64-macos or x86_64-macos.",
  )
}

const buildArguments = ["build", "package", "-Doptimize=ReleaseFast"]
if (target) buildArguments.push(`-Dtarget=${target}`)

const result = spawnSync(
  resolveZig(),
  buildArguments,
  {
    cwd: new URL("../apps/native-host/", import.meta.url),
    env: nativeSdkEnvironment(),
    stdio: "inherit",
  },
)

if (result.error) {
  throw result.error
}
process.exitCode = result.status ?? 1
