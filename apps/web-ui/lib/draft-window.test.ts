import { afterEach, describe, expect, it, vi } from "vitest"

import {
  draftWindowLabel,
  draftWindowUrl,
  openDraftInNewWindow,
  readDraftWindowTarget,
} from "@/lib/draft-window"

const target = {
  draftId: "22222222-2222-4222-8222-222222222222",
  targetAgentId: "11111111-1111-4111-8111-111111111111",
  workspaceBindingId: "33333333-3333-4333-8333-333333333333",
  title: "IPL Expert",
}

describe("draft window helpers", () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it("parses a dedicated Draft window target from the query string", () => {
    expect(readDraftWindowTarget("?foo=1")).toBeNull()
    expect(readDraftWindowTarget(
      `?draftWindow=1&draftId=${target.draftId}&targetAgentId=${target.targetAgentId}&workspaceBindingId=${target.workspaceBindingId}&title=IPL%20Expert`,
    )).toEqual(target)
  })

  it("builds a stable window label and URL for the Draft", () => {
    expect(draftWindowLabel(target.draftId)).toBe(`draft-${target.draftId}`)
    const url = new URL(draftWindowUrl(target))
    expect(url.searchParams.get("draftWindow")).toBe("1")
    expect(url.searchParams.get("draftId")).toBe(target.draftId)
    expect(url.searchParams.get("targetAgentId")).toBe(target.targetAgentId)
    expect(url.searchParams.get("workspaceBindingId")).toBe(
      target.workspaceBindingId,
    )
  })

  it("focuses an existing native window before creating a new one", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "native-sdk.window.list") {
        return [{
          id: 2,
          label: draftWindowLabel(target.draftId),
          open: true,
          hidden: false,
          x: 120,
          y: 80,
        }]
      }
      if (command === "native-sdk.window.focus") return { id: 2 }
      throw new Error("unexpected")
    })
    vi.stubGlobal("window", {
      location: { href: "http://127.0.0.1:3000/" },
      zero: { invoke },
      open: vi.fn(),
    })

    await openDraftInNewWindow(target)

    expect(invoke).toHaveBeenCalledWith("native-sdk.window.focus", {
      label: draftWindowLabel(target.draftId),
    })
    expect(invoke).not.toHaveBeenCalledWith(
      "native-sdk.window.create",
      expect.anything(),
    )
  })

  it("creates a cascaded native window beside the host", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "native-sdk.window.list") {
        return [{
          id: 1,
          label: "main",
          open: true,
          hidden: false,
          x: 100,
          y: 60,
        }]
      }
      if (command === "native-sdk.window.create") {
        return { id: 3, label: draftWindowLabel(target.draftId) }
      }
      throw new Error(`unexpected ${command}`)
    })
    const open = vi.fn()
    vi.stubGlobal("window", {
      location: { href: "http://127.0.0.1:3000/" },
      zero: { invoke },
      open,
    })

    await openDraftInNewWindow(target)

    expect(invoke).toHaveBeenCalledWith(
      "native-sdk.window.create",
      expect.objectContaining({
        label: draftWindowLabel(target.draftId),
        title: "IPL Expert",
        titlebar: "hidden_inset",
        restoreState: false,
        x: 148,
        y: 108,
      }),
    )
    expect(open).not.toHaveBeenCalled()
  })

  it("does not navigate the host window when native create fails", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "native-sdk.window.list") return []
      throw new Error("create failed")
    })
    const open = vi.fn()
    vi.stubGlobal("window", {
      location: { href: "http://127.0.0.1:3000/" },
      zero: { invoke },
      open,
    })

    await openDraftInNewWindow(target)

    expect(open).not.toHaveBeenCalled()
  })

  it("falls back to window.open when the native bridge is absent", async () => {
    const open = vi.fn()
    vi.stubGlobal("window", {
      location: { href: "http://127.0.0.1:3000/" },
      open,
    })

    await openDraftInNewWindow(target)

    expect(open).toHaveBeenCalledWith(
      expect.stringContaining("draftWindow=1"),
      draftWindowLabel(target.draftId),
      expect.stringContaining("width=1100"),
    )
  })
})
