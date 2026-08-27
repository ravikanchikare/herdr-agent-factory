"use client"

import * as React from "react"
import { ArrowUpIcon, LoaderCircleIcon } from "lucide-react"

import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupTextarea,
} from "@agent-factory/ui/components/input-group"
import { cn } from "@agent-factory/ui/lib/utils"

type PromptInputMessage = {
  text: string
}

function PromptInput({
  className,
  onSubmit,
  ...props
}: Omit<React.ComponentProps<"form">, "onSubmit"> & {
  onSubmit: (message: PromptInputMessage) => void
}) {
  return (
    <form
      className={cn("w-full", className)}
      onSubmit={(event) => {
        event.preventDefault()
        const data = new FormData(event.currentTarget)
        onSubmit({ text: String(data.get("message") ?? "") })
      }}
      {...props}
    />
  )
}

function PromptInputBody(props: React.ComponentProps<typeof InputGroup>) {
  return <InputGroup {...props} />
}

function PromptInputTextarea({
  className,
  name = "message",
  ...props
}: React.ComponentProps<typeof InputGroupTextarea>) {
  return (
    <InputGroupTextarea
      name={name}
      className={cn("min-h-24", className)}
      {...props}
    />
  )
}

function PromptInputFooter({
  className,
  ...props
}: React.ComponentProps<typeof InputGroupAddon>) {
  return (
    <InputGroupAddon
      align="block-end"
      className={cn("justify-between", className)}
      {...props}
    />
  )
}

function PromptInputTools({
  className,
  ...props
}: React.ComponentProps<"div">) {
  return <div className={cn("flex min-w-0 items-center gap-2", className)} {...props} />
}

function PromptInputSubmit({
  pending = false,
  disabled,
  ...props
}: React.ComponentProps<typeof InputGroupButton> & { pending?: boolean }) {
  return (
    <InputGroupButton
      type="submit"
      variant="default"
      size="icon-sm"
      aria-label={pending ? "Starting Run…" : "Start Run"}
      disabled={pending || disabled}
      {...props}
    >
      {pending ? (
        <LoaderCircleIcon className="animate-spin" />
      ) : (
        <ArrowUpIcon />
      )}
    </InputGroupButton>
  )
}

export {
  PromptInput,
  PromptInputBody,
  PromptInputFooter,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
  type PromptInputMessage,
}
