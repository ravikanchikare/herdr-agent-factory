import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { SessionProjection } from "@agent-factory/runtime-client"

import {
  SessionWorkspace,
  logicalHerdrKey,
  sessionHasLiveHerdrPane,
} from "./session-workspace"

const session: SessionProjection = {
  id: "coding-session",
  projectId: "project-1",
  environmentId: "env-1",
  targetAgentId: "agent-1",
  workspaceBindingId: "binding-1",
  title: "coding: Build",
  purpose: "coding",
  lifecycle: "working",
  harnessId: "claude",
  herdrAgentName: "coding-agent",
  availability: "live",
  paneId: "w1:p2",
  attention: [],
  briefDelivered: true,
  createdAtUnixMs: 1,
  lastActivityAtUnixMs: 1,
}

describe("logicalHerdrKey", () => {
  it("maps confirmation and navigation keys", () => {
    expect(logicalHerdrKey({ key: "Enter", ctrlKey: false, metaKey: false, altKey: false }))
      .toBe("enter")
    expect(logicalHerdrKey({ key: "ArrowUp", ctrlKey: false, metaKey: false, altKey: false }))
      .toBe("up")
    expect(logicalHerdrKey({ key: "c", ctrlKey: true, metaKey: false, altKey: false }))
      .toBe("ctrl+c")
  })
})

describe("sessionHasLiveHerdrPane", () => {
  it("requires a pane from a fresh Herdr observation", () => {
    expect(sessionHasLiveHerdrPane(session)).toBe(true)
    expect(sessionHasLiveHerdrPane({
      ...session,
      availability: "last_observed",
    })).toBe(false)
  })
})

describe("SessionWorkspace", () => {
  it("loads Herdr output when the session pane opens", async () => {
    const readTranscript = vi.fn(async () => ({
      agentSessionId: session.id,
      capturedAtUnixMs: 1,
      revision: 1,
      text: "implementing the review agent",
      truncated: false,
    }))
    render(
      <SessionWorkspace
        session={session}
        readTranscript={readTranscript}
      />,
    )
    expect(screen.getByRole("heading", { name: "Coding Agent" })).toBeVisible()
    expect(screen.getByText("Working")).toBeVisible()
    await waitFor(() => {
      expect(screen.getByText("implementing the review agent")).toBeVisible()
    })
    fireEvent.click(screen.getByRole("button", { name: "Refresh output" }))
    expect(readTranscript).toHaveBeenCalledTimes(2)
  })

  it("forwards focused keyboard input to the Herdr agent", () => {
    const onSendKeys = vi.fn()
    render(
      <SessionWorkspace
        session={session}
        onSendKeys={onSendKeys}
      />,
    )
    const surface = screen.getByRole("application", {
      name: "Herdr agent interface",
    })
    fireEvent.keyDown(surface, { key: "ArrowUp" })
    fireEvent.keyDown(surface, { key: "Enter" })
    fireEvent.keyDown(surface, { key: "1" })
    expect(onSendKeys).toHaveBeenNthCalledWith(1, session.id, ["up"])
    expect(onSendKeys).toHaveBeenNthCalledWith(2, session.id, ["enter"])
    expect(onSendKeys).toHaveBeenNthCalledWith(3, session.id, ["1"])
  })

  it("does not poll Herdr when the agent has left its pane", () => {
    render(
      <SessionWorkspace
        session={{
          ...session,
          lifecycle: undefined,
          availability: "historical",
          paneId: undefined,
        }}
      />,
    )
    expect(screen.getByText(
      "This managed session no longer has a live Herdr agent.",
    )).toBeVisible()
    expect(screen.queryByText("Waiting for Herdr output.")).toBeNull()
  })

  it("does not render an embedded Herdr terminal", () => {
    render(<SessionWorkspace session={session} />)
    expect(screen.queryByRole("application", {
      name: "Herdr terminal",
    })).toBeNull()
  })
})
