// Sidebar resize constants and helpers.
//
// Mirrors the Native Web Shell behavior: the sidebar width is driven from React
// state as pixels, persisted to localStorage, and snaps to collapsed when a drag
// crosses the collapse threshold. The threshold sits below the minimum settled
// width so the panel cannot flicker between collapsed and expanded.

export const SIDEBAR_WIDTH_KEY = "agent-factory.sidebar-width"
export const SIDEBAR_COLLAPSE_THRESHOLD = 200
export const SIDEBAR_MIN_WIDTH = 240
export const SIDEBAR_MAX_WIDTH = 360
export const SIDEBAR_DEFAULT_WIDTH = 256

// Clamp a settled width to the comfortable [min, max] range and round to a pixel.
export function clampSidebarWidth(width: number): number {
  return Math.min(
    SIDEBAR_MAX_WIDTH,
    Math.max(SIDEBAR_MIN_WIDTH, Math.round(width)),
  )
}

// During a live drag the width is permissive down to zero so the panel tracks the
// pointer; collapse is decided separately by shouldCollapseSidebar.
export function clampDragSidebarWidth(width: number): number {
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(0, Math.round(width)))
}

// A drag at or below the threshold snaps the sidebar shut.
export function shouldCollapseSidebar(width: number): boolean {
  return width <= SIDEBAR_COLLAPSE_THRESHOLD
}

export function readStoredSidebarWidth(): number {
  if (typeof window === "undefined") return SIDEBAR_DEFAULT_WIDTH
  try {
    const raw = window.localStorage.getItem(SIDEBAR_WIDTH_KEY)
    if (!raw) return SIDEBAR_DEFAULT_WIDTH
    const value = Number.parseInt(raw, 10)
    if (!Number.isFinite(value)) return SIDEBAR_DEFAULT_WIDTH
    return clampSidebarWidth(value)
  } catch {
    return SIDEBAR_DEFAULT_WIDTH
  }
}

export function persistSidebarWidth(width: number): void {
  if (typeof window === "undefined") return
  try {
    window.localStorage.setItem(
      SIDEBAR_WIDTH_KEY,
      String(clampSidebarWidth(width)),
    )
  } catch {
    // Ignore quota or unavailable storage.
  }
}
