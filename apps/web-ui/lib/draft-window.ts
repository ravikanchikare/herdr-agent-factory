/** Open a Draft in a dedicated application window with the same Draft experience. */

export type DraftWindowTarget = {
  draftId: string
  targetAgentId: string
  workspaceBindingId: string
  title?: string
}

const DRAFT_WINDOW_FLAG = "draftWindow"

export function draftWindowLabel(draftId: string): string {
  return `draft-${draftId}`
}

export function subscribeDraftWindowSearch(onChange: () => void): () => void {
  window.addEventListener("popstate", onChange)
  return () => window.removeEventListener("popstate", onChange)
}

export function getDraftWindowSearchSnapshot(): string {
  return window.location.search
}

export function getDraftWindowSearchServerSnapshot(): undefined {
  return undefined
}

export function readDraftWindowTarget(
  search = typeof window === "undefined" ? "" : window.location.search,
): DraftWindowTarget | null {
  const params = new URLSearchParams(search)
  if (params.get(DRAFT_WINDOW_FLAG) !== "1") return null
  const draftId = params.get("draftId")
  const targetAgentId = params.get("targetAgentId")
  const workspaceBindingId = params.get("workspaceBindingId")
  if (!draftId || !targetAgentId || !workspaceBindingId) return null
  const title = params.get("title") ?? undefined
  return { draftId, targetAgentId, workspaceBindingId, title }
}

export function draftWindowUrl(target: DraftWindowTarget): string {
  const url = new URL(
    typeof window === "undefined" ? "http://127.0.0.1:3000/" : window.location.href,
  )
  url.search = ""
  url.hash = ""
  url.searchParams.set(DRAFT_WINDOW_FLAG, "1")
  url.searchParams.set("draftId", target.draftId)
  url.searchParams.set("targetAgentId", target.targetAgentId)
  url.searchParams.set("workspaceBindingId", target.workspaceBindingId)
  if (target.title) url.searchParams.set("title", target.title)
  return url.toString()
}

type WindowBridge = {
  invoke(command: string, payload?: unknown): Promise<unknown>
}

type NativeWindowInfo = {
  id?: number
  label?: string
  open?: boolean
  hidden?: boolean
  x?: number
  y?: number
}

const MAIN_WINDOW_LABEL = "main"
const CASCADE_OFFSET_PX = 48
const DRAFT_WINDOW_WIDTH_PX = 1100
const DRAFT_WINDOW_HEIGHT_PX = 760

function windowBridge(): WindowBridge | undefined {
  const bridge = (globalThis as { window?: { zero?: WindowBridge } }).window?.zero
  return bridge
}

async function listNativeWindows(
  bridge: WindowBridge,
): Promise<readonly NativeWindowInfo[]> {
  try {
    const listed = await bridge.invoke("native-sdk.window.list")
    return Array.isArray(listed) ? listed as NativeWindowInfo[] : []
  } catch {
    return []
  }
}

async function focusNativeWindow(
  bridge: WindowBridge,
  label: string,
): Promise<boolean> {
  try {
    await bridge.invoke("native-sdk.window.focus", { label })
    return true
  } catch {
    return false
  }
}

// Keep the factory host visible behind the Draft window. A hidden main
// window (close_policy = hide) is shown, then the Draft is focused again.
async function revealMainWindow(
  bridge: WindowBridge,
  draftLabel: string,
  windows: readonly NativeWindowInfo[],
): Promise<void> {
  const main = windows.find((candidate) =>
    candidate.label === MAIN_WINDOW_LABEL)
  if (main && main.hidden !== true) return
  await focusNativeWindow(bridge, MAIN_WINDOW_LABEL)
  await focusNativeWindow(bridge, draftLabel)
}

/**
 * Opens (or focuses) a dedicated native window for the Draft.
 * Falls back to `window.open` only when the native bridge is absent
 * (browser dev). Never navigates the calling host window.
 */
export async function openDraftInNewWindow(
  target: DraftWindowTarget,
): Promise<void> {
  const label = draftWindowLabel(target.draftId)
  const url = draftWindowUrl(target)
  const title = target.title?.trim() || "Draft"
  const bridge = windowBridge()

  if (bridge) {
    const windows = await listNativeWindows(bridge)
    const existing = windows.find((candidate) =>
      candidate.label === label && candidate.open !== false)
    if (existing) {
      await focusNativeWindow(bridge, label)
      await revealMainWindow(bridge, label, windows)
      return
    }
    const main = windows.find((candidate) =>
      candidate.label === MAIN_WINDOW_LABEL)
    try {
      await bridge.invoke("native-sdk.window.create", {
        label,
        title,
        width: DRAFT_WINDOW_WIDTH_PX,
        height: DRAFT_WINDOW_HEIGHT_PX,
        x: (main?.x ?? 80) + CASCADE_OFFSET_PX,
        y: (main?.y ?? 80) + CASCADE_OFFSET_PX,
        titlebar: "hidden_inset",
        restoreState: false,
        url,
      })
    } catch {
      await focusNativeWindow(bridge, label)
    }
    await revealMainWindow(
      bridge,
      label,
      await listNativeWindows(bridge),
    )
    return
  }

  if (typeof window !== "undefined") {
    window.open(
      url,
      label,
      "noopener,noreferrer,width=1100,height=760",
    )
  }
}
