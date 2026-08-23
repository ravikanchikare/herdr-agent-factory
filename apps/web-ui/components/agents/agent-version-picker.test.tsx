import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { TargetAgentVersionProjection } from "@agent-factory/runtime-client"

import { AgentVersionButtonGroup } from "@/components/agents/agent-version-picker"

const versions: TargetAgentVersionProjection[] = [
  {
    id: "v2",
    name: "Commerce Copilot",
    version: "0.2.0",
    objective: "Ship answers",
    acceptanceCriteria: ["Works"],
    gitCommit: "abc",
    gitTag: "agent-factory/x/v0.2.0",
    createdAtUnixMs: 2,
    sourceDraftId: "draft-2",
    targetAgentId: "agent-1",
  },
  {
    id: "v1",
    name: "Commerce Copilot",
    version: "0.1.0",
    objective: "Ship answers",
    acceptanceCriteria: ["Works"],
    gitCommit: "def",
    gitTag: "agent-factory/x/v0.1.0",
    createdAtUnixMs: 1,
    sourceDraftId: "draft-1",
    targetAgentId: "agent-1",
  },
]

describe("AgentVersionButtonGroup", () => {
  it("always labels Version and never shows a selected version", () => {
    render(
      <AgentVersionButtonGroup
        versions={versions}
        onOpenVersion={vi.fn()}
      />,
    )

    const trigger = screen.getByRole("combobox", { name: "Open version selector" })
    expect(trigger).toHaveTextContent("Version")
    expect(trigger).not.toHaveTextContent("v0.2.0")
    expect(trigger).not.toHaveTextContent("v0.1.0")
  })

  it("opens a searchable Combobox list for Inspector navigation", async () => {
    const onOpenVersion = vi.fn()
    render(
      <AgentVersionButtonGroup
        versions={versions}
        onOpenVersion={onOpenVersion}
      />,
    )

    fireEvent.click(screen.getByRole("combobox", { name: "Open version selector" }))
    // Input-inside-popup Combobox surfaces the popup as a dialog.
    expect(await screen.findByRole("dialog", { name: "Select version" }))
      .toBeTruthy()
    expect(screen.getByRole("combobox", { name: "Search versions" })).toBeTruthy()
    expect(screen.getByRole("listbox")).toBeTruthy()

    // Base UI listboxes select on pointerdown.
    const option = await screen.findByRole("option", { name: /v0\.1\.0/ })
    fireEvent.pointerDown(option)
    fireEvent.click(option)

    await waitFor(() => {
      expect(onOpenVersion).toHaveBeenCalledWith(versions[1])
    })
    // Selection is navigation only — trigger stays unlabeled as Version.
    expect(screen.getByRole("combobox", { name: "Open version selector" }))
      .toHaveTextContent("Version")
  })
})
