"use client"

import * as React from "react"
import { ArrowDownIcon } from "lucide-react"
import {
  StickToBottom,
  useStickToBottomContext,
} from "use-stick-to-bottom"

import { Button } from "@agent-factory/ui/components/button"
import { cn } from "@agent-factory/ui/lib/utils"

function Conversation({
  className,
  ...props
}: React.ComponentProps<typeof StickToBottom>) {
  return (
    <StickToBottom
      className={cn("relative min-h-0 flex-1 overflow-y-hidden", className)}
      initial="instant"
      resize="smooth"
      role="log"
      {...props}
    />
  )
}

function ConversationContent({
  className,
  ...props
}: React.ComponentProps<typeof StickToBottom.Content>) {
  return (
    <StickToBottom.Content
      className={cn("flex flex-col gap-6 p-4", className)}
      {...props}
    />
  )
}

function ConversationScrollButton({
  className,
  ...props
}: React.ComponentProps<typeof Button>) {
  const { isAtBottom, scrollToBottom } = useStickToBottomContext()
  if (isAtBottom) return null

  return (
    <Button
      type="button"
      variant="outline"
      size="icon-sm"
      className={cn(
        "absolute bottom-4 left-1/2 -translate-x-1/2 rounded-full",
        className,
      )}
      aria-label="Scroll to latest"
      onClick={() => scrollToBottom()}
      {...props}
    >
      <ArrowDownIcon />
    </Button>
  )
}

export {
  Conversation,
  ConversationContent,
  ConversationScrollButton,
}
