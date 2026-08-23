"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"

import type {
  RuntimeIntent,
  TargetAgentProjection,
  TargetAgentVersionProjection,
} from "@agent-factory/runtime-client"
import { Button } from "@agent-factory/ui/components/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@agent-factory/ui/components/dialog"
import {
  Field,
  FieldDescription,
  FieldLabel,
} from "@agent-factory/ui/components/field"
import { Input } from "@agent-factory/ui/components/input"

type EmitIntent = (intent: RuntimeIntent) => Promise<void>

export function CreateDraftDialog({
  agent,
  version,
  emitIntent,
  open,
  onOpenChange,
  showTrigger = true,
}: {
  agent: TargetAgentProjection
  version?: TargetAgentVersionProjection
  emitIntent: EmitIntent
  open?: boolean
  onOpenChange?: (open: boolean) => void
  showTrigger?: boolean
}) {
  const defaultName = version
    ? `v${version.version} changes`
    : "Draft"
  const [name, setName] = React.useState(defaultName)
  const inputId = version ? `new-draft-${version.id}` : "new-draft-initial"

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) setName(defaultName)
        onOpenChange?.(next)
      }}
    >
      {showTrigger ? (
        <DialogTrigger render={<Button variant="outline" size="sm" />}>
          <PlusIcon data-icon="inline-start" />
          Create Draft
        </DialogTrigger>
      ) : null}
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {version
              ? `Create Draft from v${version.version}`
              : "Create Draft"}
          </DialogTitle>
          <DialogDescription>
            {version
              ? `A sibling worktree and dedicated branch will start at ${version.gitCommit.slice(0, 12)}.`
              : "A sibling worktree and dedicated branch will start at the repository HEAD."}
          </DialogDescription>
        </DialogHeader>
        <Field>
          <FieldLabel htmlFor={inputId}>
            Draft name
          </FieldLabel>
          <Input
            id={inputId}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <FieldDescription>
            Branch: agent-factory/{agent.id}/drafts/&lt;new-draft-id&gt;
            <br />
            Worktree: {draftPathPreview(
              agent.repositoryRoot,
              version?.name ?? agent.name,
              name,
            )}
          </FieldDescription>
        </Field>
        <DialogFooter showCloseButton>
          <Button
            disabled={!name.trim()}
            onClick={() => {
              void emitIntent({
                type: "agentDraft.create",
                targetAgentId: agent.id,
                ...(version ? { baseVersionId: version.id } : {}),
                draftName: name.trim(),
              })
              onOpenChange?.(false)
            }}
          >
            Create Draft
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function draftPathPreview(
  repositoryRoot: string,
  agentName: string,
  draftName: string,
) {
  const separator = repositoryRoot.lastIndexOf("/")
  const parent = separator > 0 ? repositoryRoot.slice(0, separator) : ""
  const repository = repositoryRoot.slice(separator + 1)
  const slug = (value: string) => value.trim().toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 48)
  return `${parent}/${slug(repository)}-${slug(agentName)}-${
    slug(draftName)
  }-<draft-id>`
}
