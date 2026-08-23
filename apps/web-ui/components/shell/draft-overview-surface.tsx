"use client"

import * as React from "react"
import { ListChecksIcon } from "lucide-react"

import { Button } from "@agent-factory/ui/components/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@agent-factory/ui/components/popover"
import { Separator } from "@agent-factory/ui/components/separator"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-factory/ui/components/tooltip"

/**
 * Minimum workspace width (in px) that fits the inline Draft Overview column
 * (20rem, matching the popover width) beside a usable primary Draft column
 * (28rem). Below this the overview leaves the layout and the title-bar toggle
 * opens it as a popover over the Draft view instead.
 */
export const DRAFT_OVERVIEW_INLINE_MIN_WIDTH = 48 * 16

export type DraftOverviewMode = "inline" | "popover"

export type DraftOverviewState = {
  mode: DraftOverviewMode
  open: boolean
  setOpen: (open: boolean) => void
}

/**
 * Width-driven Draft Overview visibility. The first element of the tuple is a
 * ref callback that attaches the workspace element whose width picks the
 * presentation mode; the second is the visibility state for whichever
 * presentation is active. When the workspace is wide the title-bar toggle
 * shows a second column beside the Draft view, and when it is narrow the same
 * toggle opens a popover.
 *
 * The two presentations keep separate visibility so a resize does not throw
 * away what the user opened: narrowing below the threshold hides the inline
 * column (the popover stays available through its trigger, never
 * auto-opened), and widening back restores the inline column on its own as
 * long as the user wanted the overview in either form. Resizing within one
 * range never hides anything.
 */
export function useDraftOverviewSurface(): readonly [
  (element: HTMLElement | null) => void,
  DraftOverviewState,
] {
  const [container, bindContainer] = React.useState<HTMLElement | null>(null)
  const width = useElementWidth(container)
  const mode: DraftOverviewMode = width >= DRAFT_OVERVIEW_INLINE_MIN_WIDTH
    ? "inline"
    : "popover"
  const [inlineOpen, setInlineOpen] = React.useState(false)
  const [popoverOpen, setPopoverOpen] = React.useState(false)
  const [previousMode, setPreviousMode] = React.useState(mode)
  if (previousMode !== mode) {
    // Render-phase adjustment so a resize reconciles the two presentations
    // without an effect.
    setPreviousMode(mode)
    if (mode === "popover") {
      // Wide -> narrow: the inline column no longer fits, so hide it and
      // leave the popover closed (available through its trigger). inlineOpen
      // stays sticky so the column can restore itself when width returns.
      setPopoverOpen(false)
    } else {
      // Narrow -> wide: restore the inline column if the user wanted the
      // overview in either presentation, then clear popover state.
      setInlineOpen(inlineOpen || popoverOpen)
      setPopoverOpen(false)
    }
  }
  const open = mode === "inline" ? inlineOpen : popoverOpen
  const setOpen = mode === "inline" ? setInlineOpen : setPopoverOpen
  return [bindContainer, { mode, open, setOpen }] as const
}

/** Title-bar toggle. Owns the popover presentation when the mode is narrow. */
export function DraftOverviewTrigger({
  id,
  overview,
  children,
}: {
  id: string
  overview: DraftOverviewState
  children?: React.ReactNode
}) {
  const { mode, open, setOpen } = overview
  const label = open ? "Hide Draft Overview" : "Show Draft Overview"
  // Inline mode keeps the Popover out of the tree, so the button toggles the
  // second column directly. In popover mode `onClick` is intentionally absent:
  // an explicit `undefined` would clobber the PopoverTrigger's own handler
  // during prop merging and break toggle-to-close.
  const button = (
    <Button
      variant={open ? "secondary" : "ghost"}
      size="icon-sm"
      aria-label={label}
      aria-expanded={open}
      aria-controls={id}
      {...(mode === "inline" ? { onClick: () => setOpen(!open) } : {})}
    />
  )
  if (mode === "inline") {
    return (
      <Tooltip>
        <TooltipTrigger render={button}>
          <ListChecksIcon />
        </TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
    )
  }
  return (
    <Popover modal={false} open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger
          render={
            <PopoverTrigger
              render={button}
            />
          }
        >
          <ListChecksIcon />
        </TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
      <PopoverContent
        id={id}
        role="complementary"
        aria-label="Draft Overview"
        align="end"
        sideOffset={8}
        className="max-h-(--available-height) w-80 overflow-y-auto p-3"
      >
        {children}
      </PopoverContent>
    </Popover>
  )
}

/** Inline presentation: a second column beside the Draft view. */
export function DraftOverviewPanel({
  id,
  overview,
  children,
}: {
  id: string
  overview: DraftOverviewState
  children?: React.ReactNode
}) {
  if (overview.mode !== "inline" || !overview.open) return null
  return (
    <>
      <Separator orientation="vertical" />
      <aside
        id={id}
        aria-label="Draft Overview"
        className="flex w-80 shrink-0 flex-col overflow-y-auto p-3"
      >
        {children}
      </aside>
    </>
  )
}

/** Width of one element, subscribed through ResizeObserver. */
function useElementWidth(element: HTMLElement | null): number {
  return React.useSyncExternalStore(
    React.useCallback(
      (onChange) => {
        if (!element || typeof ResizeObserver === "undefined") {
          return () => undefined
        }
        const observer = new ResizeObserver(onChange)
        observer.observe(element)
        return () => observer.disconnect()
      },
      [element],
    ),
    () => element?.getBoundingClientRect().width ?? 0,
    () => 0,
  )
}