"use client"

import * as React from "react"
import {
  BinaryIcon,
  CopyIcon,
  FileQuestionIcon,
  FolderTreeIcon,
  XIcon,
} from "lucide-react"

import { FileTree, useFileTree } from "@pierre/trees/react"

import type {
  RuntimeIntent,
  TargetAgentProjection,
  TargetAgentVersionProjection,
  VersionFileReadDto,
  VersionFilesListDto,
} from "@agent-factory/runtime-client"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@agent-factory/ui/components/alert"
import { Badge } from "@agent-factory/ui/components/badge"
import { Button } from "@agent-factory/ui/components/button"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@agent-factory/ui/components/empty"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@agent-factory/ui/components/resizable"
import { ScrollArea } from "@agent-factory/ui/components/scroll-area"
import { Separator } from "@agent-factory/ui/components/separator"
import { Skeleton } from "@agent-factory/ui/components/skeleton"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-factory/ui/components/tooltip"

import { CreateDraftDialog } from "@/components/agents/create-draft-dialog"

export type VersionFilesInspectorState = {
  version: TargetAgentVersionProjection
} & (
  | { status: "loading" }
  | { status: "error"; error: string }
  | { status: "ready"; files: VersionFilesListDto }
)

export function VersionFilesInspector({
  state,
  agent,
  emitIntent,
  onClose,
  readVersionFile,
}: {
  state: VersionFilesInspectorState
  agent: TargetAgentProjection
  emitIntent: (intent: RuntimeIntent) => Promise<void>
  onClose: () => void
  readVersionFile: (
    versionId: string,
    path: string,
  ) => Promise<VersionFileReadDto>
}) {
  const shortCommit = state.version.gitCommit.slice(0, 12)

  return (
    <section
      aria-label={`${agent.name} v${state.version.version} Version inspector`}
      className="flex size-full min-h-0 min-w-0 flex-col bg-background"
    >
      <header
        data-native-drag-region
        className="flex h-11 shrink-0 items-center gap-2 px-3"
      >
        <div className="flex min-w-0 items-center gap-1.5">
          <h2 className="min-w-0 truncate text-sm font-medium">
            {agent.name} v{state.version.version}
          </h2>
          <span aria-hidden="true" className="text-muted-foreground">·</span>
          <code
            className="shrink-0 text-xs text-muted-foreground"
            title={`Full Git commit ${state.version.gitCommit}`}
          >
            {shortCommit}
          </code>
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  data-native-no-drag
                  variant="ghost"
                  size="icon-xs"
                  aria-label="Copy full Git commit"
                  onClick={() =>
                    void navigator.clipboard?.writeText(state.version.gitCommit)}
                />
              }
            >
              <CopyIcon />
            </TooltipTrigger>
            <TooltipContent>Copy full Git commit</TooltipContent>
          </Tooltip>
        </div>
        <div data-native-no-drag className="ml-auto flex shrink-0 items-center gap-1">
          <CreateDraftDialog
            agent={agent}
            version={state.version}
            emitIntent={emitIntent}
          />
          <Badge variant="outline">Read-only</Badge>
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Close Version inspector"
                  onClick={onClose}
                />
              }
            >
              <XIcon />
            </TooltipTrigger>
            <TooltipContent>Close Version inspector</TooltipContent>
          </Tooltip>
        </div>
      </header>
      <Separator className="h-px w-full" />
      <div className="min-h-0 flex-1">
        {state.status === "loading" ? (
          <VersionFilesLoading />
        ) : state.status === "error" ? (
          <div className="p-4">
            <Alert variant="destructive">
              <AlertTitle>Version files unavailable</AlertTitle>
              <AlertDescription>{state.error}</AlertDescription>
            </Alert>
          </div>
        ) : state.files.entries.length === 0 ? (
          <Empty className="size-full">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <FolderTreeIcon />
              </EmptyMedia>
              <EmptyTitle>No files in this Version</EmptyTitle>
              <EmptyDescription>
                The immutable commit contains an empty repository tree.
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <LoadedVersionFiles
            key={state.files.versionId}
            files={state.files}
            readVersionFile={readVersionFile}
          />
        )}
      </div>
    </section>
  )
}

function VersionFilesLoading() {
  return (
    <div className="grid size-full grid-cols-3 gap-4 p-4">
      <div className="flex flex-col gap-2">
        {Array.from({ length: 8 }, (_, index) => (
          <Skeleton key={index} className="h-7 w-full" />
        ))}
      </div>
      <Skeleton className="col-span-2 size-full" />
    </div>
  )
}

type PreviewState =
  | { status: "idle" }
  | { status: "loading"; path: string }
  | { status: "error"; path: string; error: string }
  | { status: "ready"; file: VersionFileReadDto }

function LoadedVersionFiles({
  files,
  readVersionFile,
}: {
  files: VersionFilesListDto
  readVersionFile: (
    versionId: string,
    path: string,
  ) => Promise<VersionFileReadDto>
}) {
  const [preview, setPreview] = React.useState<PreviewState>({ status: "idle" })
  const requestSequence = React.useRef(0)
  const entriesByPath = React.useMemo(
    () => new Map(files.entries.map((entry) => [entry.path, entry])),
    [files.entries],
  )
  const handleSelectionChange = React.useCallback(
    (selectedPaths: readonly string[]) => {
      const path = selectedPaths.at(-1)
      if (!path || !entriesByPath.has(path)) {
        requestSequence.current += 1
        setPreview({ status: "idle" })
        return
      }
      const request = requestSequence.current + 1
      requestSequence.current = request
      setPreview({ status: "loading", path })
      void readVersionFile(files.versionId, path)
        .then((file) => {
          if (requestSequence.current === request) {
            setPreview({ status: "ready", file })
          }
        })
        .catch((error: unknown) => {
          if (requestSequence.current === request) {
            setPreview({
              status: "error",
              path,
              error: errorMessage(error),
            })
          }
        })
    },
    [entriesByPath, files.versionId, readVersionFile],
  )
  const { model } = useFileTree({
    paths: files.entries.map((entry) => entry.path),
    initialExpansion: "open",
    flattenEmptyDirectories: false,
    dragAndDrop: false,
    renaming: false,
    search: false,
    onSelectionChange: handleSelectionChange,
  })

  return (
    <ResizablePanelGroup orientation="horizontal" className="size-full">
      <ResizablePanel defaultSize="36%" minSize="12rem">
        <FileTree
          model={model}
          aria-label="Version files"
          className="size-full"
          style={versionTreeStyle}
        />
      </ResizablePanel>
      <ResizableHandle withHandle aria-label="Resize Version file browser" />
      <ResizablePanel defaultSize="64%" minSize="16rem">
        <VersionFilePreview preview={preview} />
      </ResizablePanel>
    </ResizablePanelGroup>
  )
}

function VersionFilePreview({ preview }: { preview: PreviewState }) {
  if (preview.status === "idle") {
    return (
      <Empty className="size-full">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <FileQuestionIcon />
          </EmptyMedia>
          <EmptyTitle>Select a file</EmptyTitle>
          <EmptyDescription>
            Text is read directly from this Version&apos;s immutable commit.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }
  if (preview.status === "loading") {
    return (
      <div className="flex size-full flex-col gap-3 p-4" aria-label={`Loading ${preview.path}`}>
        <Skeleton className="h-6 w-1/2" />
        <Skeleton className="flex-1" />
      </div>
    )
  }
  if (preview.status === "error") {
    return (
      <div className="p-4">
        <Alert variant="destructive">
          <AlertTitle>Could not read {preview.path}</AlertTitle>
          <AlertDescription>{preview.error}</AlertDescription>
        </Alert>
      </div>
    )
  }

  const { file } = preview
  if (file.kind === "text") {
    return (
      <div className="flex size-full min-h-0 flex-col">
        <FilePreviewHeader path={file.path} size={file.size} />
        <Separator />
        <ScrollArea className="min-h-0 flex-1">
          <pre className="min-w-max p-4 font-mono text-xs leading-relaxed">
            <code>{file.content}</code>
          </pre>
        </ScrollArea>
      </div>
    )
  }

  const title = file.kind === "binary"
    ? "Binary file"
    : file.kind === "too_large"
      ? "File is too large to preview"
      : "Preview unavailable"
  const description = file.kind === "binary"
    ? "Agent Factory does not decode binary blobs in the Version inspector."
    : file.kind === "too_large"
      ? "The blob exceeds the safe 256 KiB text-preview limit."
      : "This Git entry is not a text blob that Agent Factory can preview."

  return (
    <div className="flex size-full min-h-0 flex-col">
      <FilePreviewHeader path={file.path} size={file.size} />
      <Separator />
      <Empty className="flex-1">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <BinaryIcon />
          </EmptyMedia>
          <EmptyTitle>{title}</EmptyTitle>
          <EmptyDescription>{description}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    </div>
  )
}

function FilePreviewHeader({
  path,
  size,
}: {
  path: string
  size?: number | null
}) {
  return (
    <header className="flex h-11 shrink-0 items-center gap-2 px-3">
      <p className="min-w-0 flex-1 truncate text-sm font-medium">{path}</p>
      {size == null ? null : (
        <Badge variant="secondary">{formatBytes(size)}</Badge>
      )}
    </header>
  )
}

const versionTreeStyle = {
  height: "100%",
  "--trees-bg-override": "var(--background)",
  "--trees-bg-muted-override": "var(--muted)",
  "--trees-fg-override": "var(--foreground)",
  "--trees-fg-muted-override": "var(--muted-foreground)",
  "--trees-border-color-override": "var(--border)",
  "--trees-selected-bg-override": "var(--accent)",
  "--trees-selected-fg-override": "var(--accent-foreground)",
  "--trees-focus-ring-color-override": "var(--ring)",
} as React.CSSProperties

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "The runtime rejected the file request."
}
