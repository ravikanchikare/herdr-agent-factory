import assert from "node:assert/strict"
import { delimiter, join } from "node:path"
import test from "node:test"

import { nativeSdkEnvironment } from "./native-sdk-command.mjs"

test("prepends the managed Zig toolchain for nested Ghostty commands", () => {
  const environment = nativeSdkEnvironment(
    { PATH: ["/usr/bin", "/bin"].join(delimiter) },
    "/Users/tester",
  )

  assert.deepEqual(environment.PATH.split(delimiter), [
    "/Users/tester/.native/toolchains/zig-0.16.0",
    "/usr/bin",
    "/bin",
  ])
})

test("uses the configured Native SDK home without duplicating PATH", () => {
  const toolchain = "/opt/native/toolchains/zig-0.16.0"
  const environment = nativeSdkEnvironment({
    NATIVE_SDK_HOME: "/opt/native",
    PATH: [toolchain, "/usr/bin"].join(delimiter),
  })

  assert.equal(environment.PATH, [toolchain, "/usr/bin"].join(delimiter))
})

test("treats an empty Native SDK home like the CLI default", () => {
  const environment = nativeSdkEnvironment(
    { NATIVE_SDK_HOME: "", PATH: "/usr/bin" },
    "/Users/tester",
  )

  assert.equal(
    environment.PATH,
    ["/Users/tester/.native/toolchains/zig-0.16.0", "/usr/bin"].join(
      delimiter,
    ),
  )
})

test("prepends the directory containing an explicit Zig override", () => {
  const environment = nativeSdkEnvironment({
    NATIVE_SDK_ZIG: join("/opt", "zig", "zig"),
    PATH: "/usr/bin",
  })

  assert.equal(environment.PATH, ["/opt/zig", "/usr/bin"].join(delimiter))
})
