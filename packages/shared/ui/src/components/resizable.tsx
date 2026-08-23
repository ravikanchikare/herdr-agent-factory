"use client"

import * as ResizablePrimitive from "react-resizable-panels"

import { cn } from "@agent-factory/ui/lib/utils"

function ResizablePanelGroup({
  className,
  ...props
}: ResizablePrimitive.GroupProps) {
  return (
    <ResizablePrimitive.Group
      data-slot="resizable-panel-group"
      className={cn(
        "flex h-full w-full aria-[orientation=vertical]:flex-col",
        className
      )}
      {...props}
    />
  )
}

function ResizablePanel({ ...props }: ResizablePrimitive.PanelProps) {
  return <ResizablePrimitive.Panel data-slot="resizable-panel" {...props} />
}

const lineHandleClassName =
  "w-px bg-border after:absolute after:inset-y-0 after:left-1/2 after:w-1 after:-translate-x-1/2 aria-[orientation=horizontal]:h-px aria-[orientation=horizontal]:w-full aria-[orientation=horizontal]:after:left-0 aria-[orientation=horizontal]:after:h-1 aria-[orientation=horizontal]:after:w-full aria-[orientation=horizontal]:after:translate-x-0 aria-[orientation=horizontal]:after:-translate-y-1/2 [&[aria-orientation=horizontal]>div]:rotate-90"

const hoverHandleClassName = [
  "w-3 bg-transparent",
  "aria-[orientation=horizontal]:h-3 aria-[orientation=horizontal]:w-full",
  "[&>div]:opacity-0 [&>div]:transition-opacity",
  "hover:[&>div]:opacity-100 focus-visible:[&>div]:opacity-100",
  "data-[separator=hover]:[&>div]:opacity-100",
  "data-[separator=active]:[&>div]:opacity-100",
  "data-[separator=focus]:[&>div]:opacity-100",
].join(" ")

function ResizableHandle({
  withHandle,
  reveal = "always",
  className,
  ...props
}: ResizablePrimitive.SeparatorProps & {
  withHandle?: boolean
  /** `hover` hides the full-height rule and shows a short grip on hover. */
  reveal?: "always" | "hover"
}) {
  return (
    <ResizablePrimitive.Separator
      data-slot="resizable-handle"
      data-reveal={reveal}
      className={cn(
        "relative flex items-center justify-center ring-offset-background",
        "focus-visible:ring-1 focus-visible:ring-ring",
        "focus-visible:outline-hidden",
        reveal === "hover" ? hoverHandleClassName : lineHandleClassName,
        className
      )}
      {...props}
    >
      {withHandle && (
        <div className="z-10 flex h-8 w-1 shrink-0 rounded-full bg-border" />
      )}
    </ResizablePrimitive.Separator>
  )
}

export { ResizableHandle, ResizablePanel, ResizablePanelGroup }
