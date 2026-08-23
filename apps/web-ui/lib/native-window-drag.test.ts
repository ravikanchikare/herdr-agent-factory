import { afterEach, describe, expect, it, vi } from "vitest"

import {
  handleNativeWindowDragPointerDown,
  NATIVE_WINDOW_DRAG_COMMAND,
} from "@/lib/native-window-drag"

function installBridge(result: "resolve" | "reject" = "resolve") {
  const invoke = vi.fn()
  if (result === "reject") {
    invoke.mockRejectedValue(new Error("host unavailable"))
  } else {
    invoke.mockResolvedValue({ version: 1, windowId: 7 })
  }
  Object.defineProperty(window, "zero", {
    configurable: true,
    value: { invoke, on: vi.fn() },
  })
  return invoke
}

function dispatch(target: EventTarget, button = 0) {
  handleNativeWindowDragPointerDown({ button, target })
}

afterEach(() => {
  Reflect.deleteProperty(window, "zero")
  document.body.replaceChildren()
})

describe("native window drag", () => {
  it("invokes the versioned command for primary downs in a drag region", () => {
    const invoke = installBridge()
    const region = document.createElement("div")
    const child = document.createElement("span")
    region.dataset.nativeDragRegion = ""
    region.append(child)
    document.body.append(region)

    dispatch(child)

    expect(invoke).toHaveBeenCalledWith(NATIVE_WINDOW_DRAG_COMMAND, null)
  })

  it("ignores secondary downs and targets outside a drag region", () => {
    const invoke = installBridge()
    const region = document.createElement("div")
    const outside = document.createElement("div")
    region.dataset.nativeDragRegion = ""
    document.body.append(region, outside)

    dispatch(region, 2)
    dispatch(outside)

    expect(invoke).not.toHaveBeenCalled()
  })

  it.each([
    ["button", "<button><span>Button</span></button>"],
    ["link", "<a href=\"/target\"><span>Link</span></a>"],
    ["input", "<input>"],
    ["select", "<select><option>One</option></select>"],
    ["textarea", "<textarea></textarea>"],
    ["label", "<label><span>Label</span></label>"],
    ["summary", "<details><summary><span>Summary</span></summary></details>"],
    ["editable", "<div contenteditable=\"true\"><span>Edit</span></div>"],
    ["button role", "<div role=\"button\"><span>Role</span></div>"],
    ["explicit exclusion", "<div data-native-no-drag><span>No drag</span></div>"],
  ])("keeps %s interactive inside a drag region", (_name, markup) => {
    const invoke = installBridge()
    const region = document.createElement("div")
    region.dataset.nativeDragRegion = ""
    region.innerHTML = markup
    document.body.append(region)

    dispatch(region.querySelector("span, input, select, textarea")!)

    expect(invoke).not.toHaveBeenCalled()
  })

  it("is a browser-development no-op when the native bridge is absent", () => {
    const region = document.createElement("div")
    region.dataset.nativeDragRegion = ""
    document.body.append(region)

    expect(() => dispatch(region)).not.toThrow()
  })

  it("catches bridge failures and forwards two qualifying downs", async () => {
    const invoke = installBridge("reject")
    const region = document.createElement("div")
    region.dataset.nativeDragRegion = ""
    document.body.append(region)

    dispatch(region)
    dispatch(region)
    await Promise.resolve()

    expect(invoke).toHaveBeenCalledTimes(2)
  })
})
