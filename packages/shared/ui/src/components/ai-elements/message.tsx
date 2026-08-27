import * as React from "react"

import { cn } from "@agent-factory/ui/lib/utils"

type MessageRole = "user" | "assistant" | "system"

function Message({
  className,
  from,
  ...props
}: React.ComponentProps<"article"> & { from: MessageRole }) {
  return (
    <article
      data-role={from}
      className={cn(
        "group flex w-full max-w-3xl flex-col gap-1.5",
        from === "user" ? "ml-auto items-end" : "items-start",
        className,
      )}
      {...props}
    />
  )
}

function MessageLabel({
  className,
  ...props
}: React.ComponentProps<"p">) {
  return (
    <p
      className={cn(
        "px-1 text-xs font-medium text-muted-foreground",
        className,
      )}
      {...props}
    />
  )
}

function MessageContent({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return (
    <div
      className={cn(
        "max-w-full whitespace-pre-wrap text-sm leading-relaxed",
        "group-data-[role=user]:rounded-lg",
        "group-data-[role=user]:bg-secondary",
        "group-data-[role=user]:px-4 group-data-[role=user]:py-3",
        className,
      )}
      {...props}
    />
  )
}

export { Message, MessageContent, MessageLabel }
