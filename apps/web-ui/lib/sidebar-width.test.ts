import { describe, expect, it, beforeEach } from "vitest"

import {
  SIDEBAR_COLLAPSE_THRESHOLD,
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_WIDTH_KEY,
  clampDragSidebarWidth,
  clampSidebarWidth,
  persistSidebarWidth,
  readStoredSidebarWidth,
  shouldCollapseSidebar,
} from "./sidebar-width"

describe("sidebar-width", () => {
  beforeEach(() => {
    window.localStorage.clear()
  })

  it("clamps a settled width to [min, max] and rounds to pixels", () => {
    expect(clampSidebarWidth(300)).toBe(300)
    expect(clampSidebarWidth(SIDEBAR_MAX_WIDTH + 100)).toBe(SIDEBAR_MAX_WIDTH)
    expect(clampSidebarWidth(SIDEBAR_MIN_WIDTH - 100)).toBe(SIDEBAR_MIN_WIDTH)
    expect(clampSidebarWidth(250.7)).toBe(251)
  })

  it("lets a live drag reach zero so the panel tracks the pointer", () => {
    expect(clampDragSidebarWidth(300)).toBe(300)
    expect(clampDragSidebarWidth(-50)).toBe(0)
    expect(clampDragSidebarWidth(SIDEBAR_MAX_WIDTH + 10)).toBe(SIDEBAR_MAX_WIDTH)
  })

  it("collapses at the threshold but not at the minimum settled width", () => {
    expect(shouldCollapseSidebar(SIDEBAR_COLLAPSE_THRESHOLD)).toBe(true)
    expect(shouldCollapseSidebar(SIDEBAR_COLLAPSE_THRESHOLD + 1)).toBe(false)
    expect(shouldCollapseSidebar(SIDEBAR_MIN_WIDTH)).toBe(false)
  })

  it("falls back to the default width when nothing is stored", () => {
    expect(readStoredSidebarWidth()).toBe(SIDEBAR_DEFAULT_WIDTH)
  })

  it("reads a persisted width", () => {
    window.localStorage.setItem(SIDEBAR_WIDTH_KEY, "272")
    expect(readStoredSidebarWidth()).toBe(272)
  })

  it("ignores invalid persisted values", () => {
    window.localStorage.setItem(SIDEBAR_WIDTH_KEY, "not-a-number")
    expect(readStoredSidebarWidth()).toBe(SIDEBAR_DEFAULT_WIDTH)

    window.localStorage.setItem(
      SIDEBAR_WIDTH_KEY,
      String(SIDEBAR_MAX_WIDTH + 999),
    )
    expect(readStoredSidebarWidth()).toBe(SIDEBAR_MAX_WIDTH)
  })

  it("persists a clamped width", () => {
    persistSidebarWidth(9999)
    expect(window.localStorage.getItem(SIDEBAR_WIDTH_KEY)).toBe(
      String(SIDEBAR_MAX_WIDTH),
    )
  })
})