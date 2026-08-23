import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { FactoryRunProjection } from "@agent-factory/runtime-client"

import { CodeChangesInspector } from "./code-changes-inspector"

const run: FactoryRunProjection = {
  id: "run-1",
  targetAgentId: "agent-1",
  agentDraftId: "draft-1",
  workspaceBindingId: "binding-1",
  projectId: "project-1",
  environmentId: "environment-1",
  objective: "Update the agent",
  state: "passed",
  acceptanceCriteria: ["Tests pass"],
  startingGitHead: "abcdef0123456789",
  changedFiles: [
    {
      path: "src/index.ts",
      change: "modified",
      diff: {
        hunks: [{
          oldStart: 1,
          oldLines: 1,
          newStart: 1,
          newLines: 1,
          lines: [
            { kind: "delete", oldLine: 1, text: "old" },
            { kind: "insert", newLine: 1, text: "new" },
          ],
        }],
      },
    },
    { path: "assets/logo.png", change: "added" },
  ],
  testEvidence: [],
}

describe("CodeChangesInspector", () => {
  it("renders changed files and their read-only structured diff", () => {
    render(<CodeChangesInspector run={run} onClose={vi.fn()} />)

    expect(screen.getByRole("region", {
      name: "Code changes inspector",
    })).toBeVisible()
    expect(screen.getByText("old")).toBeVisible()
    expect(screen.getByText("new")).toBeVisible()

    fireEvent.click(screen.getByRole("button", {
      name: "View assets/logo.png diff",
    }))
    expect(screen.getByText("Diff unavailable")).toBeVisible()
  })

  it("closes from its header", () => {
    const onClose = vi.fn()
    render(<CodeChangesInspector run={run} onClose={onClose} />)

    fireEvent.click(screen.getByRole("button", {
      name: "Close Code changes inspector",
    }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})
