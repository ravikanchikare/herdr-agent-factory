import { describe, expect, it } from "vitest"

import {
  activateVersionTab,
  closeVersionSurface,
  closeVersionTab,
  emptyVersionTabs,
  isVersionSurfaceVisible,
  openVersionTab,
  reconcileVersionTabs,
} from "@/lib/version-tabs"

describe("version tabs", () => {
  it("opens a Version as the active tab", () => {
    const state = openVersionTab(emptyVersionTabs, "v2")
    expect(state).toEqual({ openIds: ["v2"], activeId: "v2" })
    expect(isVersionSurfaceVisible(state)).toBe(true)
  })

  it("activates an already-open Version instead of duplicating it", () => {
    const opened = openVersionTab(
      openVersionTab(emptyVersionTabs, "v2"),
      "v1",
    )
    const again = openVersionTab(opened, "v2")
    expect(again.openIds).toEqual(["v2", "v1"])
    expect(again.activeId).toBe("v2")
  })

  it("switches the active tab without changing open order", () => {
    const opened = openVersionTab(
      openVersionTab(emptyVersionTabs, "v2"),
      "v1",
    )
    expect(activateVersionTab(opened, "v2")).toEqual({
      openIds: ["v2", "v1"],
      activeId: "v2",
    })
    expect(activateVersionTab(opened, "missing")).toEqual(opened)
  })

  it("closes a non-active tab and leaves the active tab", () => {
    const opened = openVersionTab(
      openVersionTab(emptyVersionTabs, "v2"),
      "v1",
    )
    expect(closeVersionTab(opened, "v2")).toEqual({
      openIds: ["v1"],
      activeId: "v1",
    })
  })

  it("activates the next tab when the active tab closes", () => {
    let state = emptyVersionTabs
    for (const id of ["v1", "v2", "v3"]) {
      state = openVersionTab(state, id)
    }
    state = activateVersionTab(state, "v2")
    expect(closeVersionTab(state, "v2")).toEqual({
      openIds: ["v1", "v3"],
      activeId: "v3",
    })
  })

  it("activates the previous tab when the last tab closes", () => {
    const opened = openVersionTab(
      openVersionTab(emptyVersionTabs, "v2"),
      "v1",
    )
    expect(closeVersionTab(opened, "v1")).toEqual({
      openIds: ["v2"],
      activeId: "v2",
    })
  })

  it("hides the Version surface when the last tab closes", () => {
    const opened = openVersionTab(emptyVersionTabs, "v2")
    const closed = closeVersionTab(opened, "v2")
    expect(closed).toEqual(emptyVersionTabs)
    expect(isVersionSurfaceVisible(closed)).toBe(false)
  })

  it("closes the whole Version surface at once", () => {
    const opened = openVersionTab(
      openVersionTab(emptyVersionTabs, "v2"),
      "v1",
    )
    expect(closeVersionSurface()).toEqual(emptyVersionTabs)
    expect(isVersionSurfaceVisible(opened)).toBe(true)
  })

  it("drops tabs whose Version is no longer available", () => {
    const opened = openVersionTab(
      openVersionTab(emptyVersionTabs, "v2"),
      "v1",
    )
    expect(reconcileVersionTabs(opened, new Set(["v1"]))).toEqual({
      openIds: ["v1"],
      activeId: "v1",
    })
    expect(reconcileVersionTabs(opened, new Set())).toEqual(emptyVersionTabs)
    expect(reconcileVersionTabs(opened, new Set(["v2", "v1"]))).toBe(opened)
  })
})
