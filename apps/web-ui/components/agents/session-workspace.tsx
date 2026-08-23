"use client"

import * as React from "react"

import type {
  AgentTranscriptDto,
  SessionProjection,
} from "@agent-factory/runtime-client"
import { Shimmer } from "@agent-factory/ui/components/ai-elements/shimmer"
import { Badge } from "@agent-factory/ui/components/badge"
import { Button } from "@agent-factory/ui/components/button"
import { ScrollArea } from "@agent-factory/ui/components/scroll-area"
import { cn } from "@agent-factory/ui/lib/utils"

export type ReadAgentTranscript = (
  agentSessionId: string,
) => Promise<AgentTranscriptDto>

export type SendAgentKeys = (
  agentSessionId: string,
  keys: readonly string[],
) => void

export function logicalHerdrKey(event: {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  altKey: boolean
}): string | undefined {
  if (event.metaKey || event.altKey) return undefined
  if (event.ctrlKey) {
    if (event.key === "c" || event.key === "C") return "ctrl+c"
    return undefined
  }
  switch (event.key) {
    case "Enter": return "enter"
    case "Escape": return "escape"
    case "ArrowUp": return "up"
    case "ArrowDown": return "down"
    case "ArrowLeft": return "left"
    case "ArrowRight": return "right"
    case "Backspace": return "backspace"
    case "Tab": return "tab"
    case " ": return "space"
    default:
      if (event.key.length === 1) return event.key
      return undefined
  }
}

export function sessionTitle(session: SessionProjection) {
  if (session.purpose === "orchestration") return "Orchestrator"
  return session.purpose === "evaluation" ? "Evaluation Agent" : "Coding Agent"
}

/** Only a fresh Herdr observation may authorize screen or input commands. */
export function sessionHasLiveHerdrPane(session: SessionProjection) {
  return session.availability === "live" && Boolean(session.paneId)
}

export function sessionStatusLabel(session: SessionProjection) {
  if (session.availability === "reconnecting") return "Reconnecting"
  if (session.availability === "last_observed") return "Last observed"
  if (session.availability === "historical") return "Historical"
  return sessionLifecycleLabel(session.lifecycle)
}

export function SessionWorkspace({
  session,
  readTranscript,
  onSendKeys,
  stacked = false,
}: {
  session: SessionProjection
  readTranscript?: ReadAgentTranscript
  onSendKeys?: SendAgentKeys
  stacked?: boolean
}) {
  const title = sessionTitle(session)
  const live = session.availability === "live" &&
    session.lifecycle === "working"
  const description = session.attention[0] ??
    session.outcome?.summary ??
    liveSessionDescription(session)

  return (
    <main
      aria-label={`${title} workspace`}
      className={cn(
        "flex size-full min-h-0 flex-col",
        stacked ? "gap-3 p-3" : "gap-4 p-6",
      )}
    >
      {stacked ? (
        <p className="text-xs text-muted-foreground">
          {live ? <Shimmer>{description}</Shimmer> : description}
        </p>
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-medium">{title}</h2>
              <Badge variant="outline">
                {sessionStatusLabel(session)}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              {live ? <Shimmer>{description}</Shimmer> : description}
            </p>
          </div>
        </div>
      )}
      <SessionOutput
        sessionId={session.id}
        readTranscript={
          sessionHasLiveHerdrPane(session) ? readTranscript : undefined
        }
        onSendKeys={
          sessionHasLiveHerdrPane(session) ? onSendKeys : undefined
        }
        unavailableMessage={
          sessionHasLiveHerdrPane(session)
            ? undefined
            : unavailableSessionMessage(session)
        }
      />
    </main>
  )
}

function SessionOutput({
  sessionId,
  readTranscript,
  onSendKeys,
  unavailableMessage,
}: {
  sessionId: string
  readTranscript?: ReadAgentTranscript
  onSendKeys?: SendAgentKeys
  unavailableMessage?: string
}) {
  const [text, setText] = React.useState<string | undefined>(undefined)
  const [error, setError] = React.useState<string | undefined>(undefined)
  const loadedFor = React.useRef<string | undefined>(undefined)

  const load = React.useCallback(() => {
    if (!readTranscript) return
    void readTranscript(sessionId).then(
      (transcript) => {
        setText(transcript.text)
        setError(undefined)
      },
      (reason: unknown) => {
        setError(reason instanceof Error ? reason.message : String(reason))
      },
    )
  }, [readTranscript, sessionId])

  const bindOutput = React.useCallback((node: HTMLElement | null) => {
    if (!node) return
    if (loadedFor.current !== sessionId) {
      loadedFor.current = sessionId
      load()
    }
    if (onSendKeys) node.focus()
  }, [load, onSendKeys, sessionId])

  return (
    <section
      aria-label="Agent output"
      className="flex min-h-0 flex-1 flex-col gap-2"
    >
      <div className="flex items-center justify-between gap-2">
        {unavailableMessage ? (
          <p className="text-xs text-muted-foreground">{unavailableMessage}</p>
        ) : (
          <>
            <p className="text-xs text-muted-foreground">
              {onSendKeys
                ? "Click the output, then type. Enter confirms, arrows move, Esc cancels."
                : "Recent output from Herdr. Approvals stay in the agent's own interface."}
            </p>
            {readTranscript ? (
              <Button type="button" variant="outline" size="sm" onClick={load}>
                Refresh output
              </Button>
            ) : null}
          </>
        )}
      </div>
      {unavailableMessage ? null : (
      <ScrollArea className="min-h-0 flex-1 rounded-md border bg-muted/20">
        <pre
          ref={bindOutput}
          tabIndex={onSendKeys ? 0 : undefined}
          role={onSendKeys ? "application" : undefined}
          aria-label="Herdr agent interface"
          className={cn(
            "whitespace-pre-wrap p-4 font-mono text-xs text-foreground outline-none",
            onSendKeys && "focus-visible:ring-2 focus-visible:ring-ring/40",
          )}
          onKeyDown={(event) => {
            if (!onSendKeys) return
            const key = logicalHerdrKey(event)
            if (!key) return
            event.preventDefault()
            onSendKeys(sessionId, [key])
            window.setTimeout(() => load(), 150)
          }}
        >
          {error ?? text ?? "Waiting for Herdr output."}
        </pre>
      </ScrollArea>
      )}
    </section>
  )
}

export function sessionLifecycleLabel(lifecycle: SessionProjection["lifecycle"]) {
  switch (lifecycle) {
    case "idle": return "Idle"
    case "working": return "Working"
    case "blocked": return "Needs attention"
    case "done": return "Done"
    case "unknown": return "Unknown"
    case undefined: return "Historical"
  }
}

export function liveSessionDescription(session: SessionProjection) {
  switch (session.lifecycle) {
    case "working":
      return "The agent is working in Herdr."
    case "idle":
      return "The agent is ready for input."
    case "blocked":
      return "The agent is waiting in its own interface."
    case "done":
      return "The agent returned to a ready state with unseen output."
    case "unknown":
      return "Herdr cannot classify this agent confidently."
    case undefined:
      return session.outcome?.summary ?? "This managed session is historical."
  }
}

function unavailableSessionMessage(session: SessionProjection) {
  switch (session.availability) {
    case "reconnecting":
      return "Herdr is reconnecting. Live commands are temporarily unavailable."
    case "last_observed":
      return "This is the last observed Herdr state. Refresh live state before acting."
    case "historical":
      return session.outcome?.summary ??
        "This managed session no longer has a live Herdr agent."
    case "live":
      return "Herdr has not reported a pane for this managed session."
  }
}
