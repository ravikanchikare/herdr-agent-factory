// Reactive store backing the sidebar width.
//
// The sidebar width is external state (persisted in localStorage) that also
// needs live, un-persisted updates during a drag. `useSyncExternalStore` reads
// it during render without a hydration mismatch (the server snapshot is the
// constant default; the client snapshot restores the persisted value after
// mount) and without calling `setState` inside an effect.

import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  clampSidebarWidth,
  persistSidebarWidth,
  readStoredSidebarWidth,
} from "./sidebar-width"

const CHANGE_EVENT = "agent-factory:sidebar-width-change"

// In-memory cache so getSnapshot is stable between changes. Lazily hydrated
// from localStorage on the first client read.
let memory: number | null = null

function snapshot(): number {
  if (memory === null) memory = readStoredSidebarWidth()
  return memory
}

export function subscribeSidebarWidth(callback: () => void): () => void {
  // "storage" fires for cross-tab writes; re-read the canonical value. The
  // custom CHANGE_EVENT is dispatched by our own setters, which have already
  // updated the cache, so it only needs to trigger a re-render.
  const onStorage = () => {
    memory = readStoredSidebarWidth()
    callback()
  }
  const onChange = () => callback()

  window.addEventListener("storage", onStorage)
  window.addEventListener(CHANGE_EVENT, onChange)
  return () => {
    window.removeEventListener("storage", onStorage)
    window.removeEventListener(CHANGE_EVENT, onChange)
  }
}

export function getSidebarWidthSnapshot(): number {
  return snapshot()
}

export function getSidebarWidthServerSnapshot(): number {
  return SIDEBAR_DEFAULT_WIDTH
}

// Update the live width during a drag without touching localStorage. Permissive
// down to zero so the panel tracks the pointer; collapse is decided by the
// handle via shouldCollapseSidebar.
export function setLiveSidebarWidth(width: number): void {
  memory = Math.min(SIDEBAR_MAX_WIDTH, Math.max(0, Math.round(width)))
  window.dispatchEvent(new Event(CHANGE_EVENT))
}

// Settle a width: clamp to the comfortable range, persist, and notify.
export function persistSidebarWidthValue(width: number): void {
  const settled = clampSidebarWidth(width)
  memory = settled
  persistSidebarWidth(settled)
  window.dispatchEvent(new Event(CHANGE_EVENT))
}