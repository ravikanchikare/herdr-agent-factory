export const NATIVE_WINDOW_DRAG_COMMAND = "desktop.window.startDrag.v1"

export type NativeWindowDragRequestV1 = null

export interface NativeWindowDragResponseV1 {
  version: 1
  windowId: number
}

type NativeWindowDragBridge = {
  invoke(
    command: typeof NATIVE_WINDOW_DRAG_COMMAND,
    request: NativeWindowDragRequestV1,
  ): Promise<NativeWindowDragResponseV1>
}

type WindowDragPointerEvent = {
  button: number
  target: EventTarget | null
}

const dragRegionSelector = "[data-native-drag-region]"
const noDragSelector = [
  "[data-native-no-drag]",
  "button",
  "a[href]",
  "input",
  "select",
  "textarea",
  "label",
  "summary",
  "[contenteditable]:not([contenteditable=\"false\"])",
  "[role=\"button\"]",
].join(",")

export function handleNativeWindowDragPointerDown(
  event: WindowDragPointerEvent,
) {
  if (event.button !== 0) return

  const target = event.target instanceof Element
    ? event.target
    : event.target instanceof Node
      ? event.target.parentElement
      : null
  if (!target?.closest(dragRegionSelector)) return
  if (target.closest(noDragSelector)) return

  startNativeWindowDrag()
}

export function startNativeWindowDrag() {
  const bridge = window.zero as unknown as NativeWindowDragBridge | undefined
  if (!bridge) return

  try {
    void bridge.invoke(NATIVE_WINDOW_DRAG_COMMAND, null).catch(() => undefined)
  } catch {
    // A native drag is best-effort platform behavior. The pointer event must
    // continue through normal DOM dispatch even if the host is unavailable.
  }
}
