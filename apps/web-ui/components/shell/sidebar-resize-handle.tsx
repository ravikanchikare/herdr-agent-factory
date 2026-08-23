import * as React from "react"

import { cn } from "@agent-factory/ui/lib/utils"

import {
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  clampDragSidebarWidth,
  shouldCollapseSidebar,
} from "@/lib/sidebar-width"
import {
  persistSidebarWidthValue,
  setLiveSidebarWidth,
} from "@/lib/sidebar-width-store"

type SidebarResizeHandleProps = {
  width: number
  onCollapse: () => void
}

// Drag handle for the sidebar boundary.
//
// Reuses the visual treatment of the project's `ResizableHandle` (a centered
// grip knob plus focus ring) but owns its own pointer/keyboard logic because the
// sidebar is a fixed-position shell element, not a `react-resizable-panels`
// member. Dragging below the collapse threshold snaps the sidebar shut and
// restores the pre-drag width so the sidebar reopens at its last good size.
export function SidebarResizeHandle({
  width,
  onCollapse,
}: SidebarResizeHandleProps) {
  const onPointerDown = React.useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()

      const handle = event.currentTarget
      const startX = event.clientX
      const startWidth = width
      let latest = startWidth
      let collapsed = false

      handle.setPointerCapture(event.pointerId)
      document.body.classList.add("sidebar-resizing")

      const endDrag = (pointerId?: number) => {
        handle.removeEventListener("pointermove", onMove)
        handle.removeEventListener("pointerup", onUp)
        handle.removeEventListener("pointercancel", onUp)
        document.body.classList.remove("sidebar-resizing")
        if (pointerId !== undefined) {
          try {
            handle.releasePointerCapture(pointerId)
          } catch {
            // Pointer capture may already be released; ignore.
          }
        }
      }

      const onMove = (moveEvent: PointerEvent) => {
        const next = clampDragSidebarWidth(
          startWidth + (moveEvent.clientX - startX),
        )
        if (shouldCollapseSidebar(next)) {
          collapsed = true
          endDrag(moveEvent.pointerId)
          // Restore the pre-drag width so reopening keeps the last good size.
          setLiveSidebarWidth(startWidth)
          onCollapse()
          return
        }
        latest = next
        setLiveSidebarWidth(next)
      }

      const onUp = (upEvent: PointerEvent) => {
        endDrag(upEvent.pointerId)
        if (collapsed) return
        persistSidebarWidthValue(latest)
      }

      handle.addEventListener("pointermove", onMove)
      handle.addEventListener("pointerup", onUp)
      handle.addEventListener("pointercancel", onUp)
    },
    [width, onCollapse],
  )

  const onKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      const step = event.shiftKey ? 24 : 8
      let next = width
      switch (event.key) {
        case "ArrowLeft":
          next = width - step
          break
        case "ArrowRight":
          next = width + step
          break
        case "Home":
          next = SIDEBAR_MIN_WIDTH
          break
        case "End":
          next = SIDEBAR_MAX_WIDTH
          break
        default:
          return
      }
      event.preventDefault()

      if (shouldCollapseSidebar(next)) {
        // Keep the last persisted (pre-collapse) width for reopen; just hide.
        onCollapse()
        return
      }

      persistSidebarWidthValue(next)
    },
    [width, onCollapse],
  )

  return (
    <div
      data-native-no-drag
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize project sidebar"
      aria-valuemin={SIDEBAR_MIN_WIDTH}
      aria-valuemax={SIDEBAR_MAX_WIDTH}
      aria-valuenow={width}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      className={cn(
        "absolute top-0 z-30 hidden h-svh w-1 -translate-x-1/2 cursor-col-resize touch-none items-center justify-center md:flex",
        "focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-hidden",
      )}
      style={{ left: `${width}px` }}
    >
      <div className="z-10 flex h-6 w-1 shrink-0 rounded-lg bg-sidebar-border transition-colors hover:bg-sidebar-accent-foreground/40" />
    </div>
  )
}
