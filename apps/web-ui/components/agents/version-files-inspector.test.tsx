import * as React from "react"
import { fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type {
  TargetAgentProjection,
  TargetAgentVersionProjection,
  VersionFileReadDto,
  VersionFilesListDto,
} from "@agent-factory/runtime-client"

import { VersionFilesInspector } from "./version-files-inspector"

vi.mock("@pierre/trees/react", () => ({
  useFileTree: (options: {
    paths: readonly string[]
    onSelectionChange: (paths: readonly string[]) => void
  }) => ({ model: options }),
  FileTree: ({ model, ...props }: {
    model: {
      paths: readonly string[]
      onSelectionChange: (paths: readonly string[]) => void
    }
    "aria-label": string
  }) => React.createElement(
    "div",
    { role: "tree", "aria-label": props["aria-label"] },
    model.paths.map((path) => React.createElement(
      "button",
      {
        key: path,
        role: "treeitem",
        onClick: () => model.onSelectionChange([path]),
      },
      path,
    )),
  ),
}))

const version: TargetAgentVersionProjection = {
  id: "88888888-8888-4888-8888-888888888888",
  targetAgentId: "11111111-1111-4111-8111-111111111111",
  version: "0.1.0",
  name: "Commerce Copilot",
  objective: "Resolve commerce support requests",
  acceptanceCriteria: ["Refunds are classified"],
  sourceDraftId: "22222222-2222-4222-8222-222222222222",
  gitCommit: "abcdef0123456789abcdef0123456789abcdef01",
  gitTag: "agent-factory/agent/v0.1.0",
  createdAtUnixMs: 20,
}

const agent: TargetAgentProjection = {
  id: version.targetAgentId,
  name: "Commerce Copilot",
  repositoryRoot: "/code/commerce-copilot",
  archived: false,
  lastActivityAtUnixMs: 20,
}

const emitIntent = vi.fn(async () => undefined)

const files: VersionFilesListDto = {
  versionId: version.id,
  gitCommit: version.gitCommit,
  entries: [
    { path: "README.md", kind: "file", size: 16 },
    { path: "assets/logo.png", kind: "file", size: 128 },
    { path: "fixtures/large.txt", kind: "file", size: 300_000 },
  ],
}

describe("VersionFilesInspector", () => {
  it("renders the immutable tree and reads selections through versionId", async () => {
    const writeText = vi.fn(async () => undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    })
    const readVersionFile = vi.fn(async (
      versionId: string,
      path: string,
    ): Promise<VersionFileReadDto> => ({
      versionId,
      gitCommit: version.gitCommit,
      path,
      size: 16,
      kind: "text",
      content: "Version content\n",
    }))

    render(
      <VersionFilesInspector
        state={{
          status: "ready",
          version,
          files,
        }}
        agent={agent}
        emitIntent={emitIntent}
        onClose={vi.fn()}
        readVersionFile={readVersionFile}
      />,
    )

    const inspector = screen.getByRole("region", {
      name: "Commerce Copilot v0.1.0 Version inspector",
    })
    expect(inspector).toHaveTextContent("Commerce Copilot v0.1.0")
    expect(inspector).toHaveTextContent(version.gitCommit.slice(0, 12))
    expect(inspector).not.toHaveTextContent(version.gitCommit)
    expect(inspector).not.toHaveTextContent("files")
    expect(screen.queryByRole("dialog")).toBeNull()
    fireEvent.click(screen.getByRole("button", {
      name: "Copy full Git commit",
    }))
    expect(writeText).toHaveBeenCalledWith(version.gitCommit)
    expect(screen.getByRole("tree", { name: "Version files" })).toBeVisible()
    fireEvent.click(screen.getByRole("treeitem", { name: "README.md" }))

    expect(readVersionFile).toHaveBeenCalledWith(version.id, "README.md")
    expect(await screen.findByText("Version content")).toBeVisible()
  })

  it("closes from its panel header without an overlay", () => {
    const onClose = vi.fn()

    render(
      <VersionFilesInspector
        state={{ status: "loading", version }}
        agent={agent}
        emitIntent={emitIntent}
        onClose={onClose}
        readVersionFile={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole("button", {
      name: "Close Version inspector",
    }))

    expect(onClose).toHaveBeenCalledOnce()
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  it("creates a Draft from the inspected Version", () => {
    const intent = vi.fn(async () => undefined)

    render(
      <VersionFilesInspector
        state={{ status: "loading", version }}
        agent={agent}
        emitIntent={intent}
        onClose={vi.fn()}
        readVersionFile={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole("button", { name: "Create Draft" }))
    const dialog = screen.getByRole("dialog", {
      name: "Create Draft from v0.1.0",
    })
    fireEvent.click(within(dialog).getByRole("button", {
      name: "Create Draft",
    }))

    expect(intent).toHaveBeenCalledWith({
      type: "agentDraft.create",
      targetAgentId: agent.id,
      baseVersionId: version.id,
      draftName: "v0.1.0 changes",
    })
  })

  it.each([
    ["assets/logo.png", "binary", "Binary file"],
    ["fixtures/large.txt", "too_large", "File is too large to preview"],
  ] as const)("handles %s as %s without content", async (path, kind, message) => {
    const readVersionFile = vi.fn(async (): Promise<VersionFileReadDto> => ({
      versionId: version.id,
      gitCommit: version.gitCommit,
      path,
      size: kind === "binary" ? 128 : 300_000,
      kind,
      content: null,
    }))

    render(
      <VersionFilesInspector
        state={{
          status: "ready",
          version,
          files,
        }}
        agent={agent}
        emitIntent={emitIntent}
        onClose={vi.fn()}
        readVersionFile={readVersionFile}
      />,
    )

    fireEvent.click(screen.getByRole("treeitem", { name: path }))
    expect(await screen.findByText(message)).toBeVisible()
    expect(screen.queryByRole("textbox")).toBeNull()
  })
})
