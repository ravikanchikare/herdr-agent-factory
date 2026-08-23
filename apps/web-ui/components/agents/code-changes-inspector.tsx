"use client"

import * as React from "react"
import { FileDiffIcon, XIcon } from "lucide-react"

import type {
  ChangedFileDto,
  FactoryRunProjection,
} from "@agent-factory/runtime-client"
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
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-factory/ui/components/tooltip"
import { cn } from "@agent-factory/ui/lib/utils"

export function CodeChangesInspector({
  run,
  onClose,
}: {
  run: FactoryRunProjection
  onClose: () => void
}) {
  const [selectedPath, setSelectedPath] = React.useState(
    () => run.changedFiles[0]?.path,
  )
  const selectedFile = run.changedFiles.find(
    (file) => file.path === selectedPath,
  )

  return (
    <section
      aria-label="Code changes inspector"
      className="flex size-full min-h-0 min-w-0 flex-col bg-background"
    >
      <header
        data-native-drag-region
        className="flex h-11 shrink-0 items-center gap-2 px-3"
      >
        <div className="flex min-w-0 items-center gap-2">
          <h2 className="truncate text-sm font-medium">Code changes</h2>
          <span className="text-xs text-muted-foreground">
            {run.changedFiles.length} files
          </span>
        </div>
        <div data-native-no-drag className="ml-auto flex shrink-0 items-center gap-1">
          <Badge variant="outline">Read-only</Badge>
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Close Code changes inspector"
                  onClick={onClose}
                />
              }
            >
              <XIcon />
            </TooltipTrigger>
            <TooltipContent>Close Code changes inspector</TooltipContent>
          </Tooltip>
        </div>
      </header>
      <Separator />
      {run.changedFiles.length === 0 ? (
        <Empty className="min-h-0 flex-1">
          <EmptyHeader>
            <EmptyMedia variant="icon"><FileDiffIcon /></EmptyMedia>
            <EmptyTitle>No code changes</EmptyTitle>
            <EmptyDescription>
              This Run has not changed any files.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
          <ResizablePanel defaultSize="36%" minSize="12rem">
            <ScrollArea className="size-full">
              <div className="flex flex-col gap-1 p-2">
                {run.changedFiles.map((file) => (
                  <Button
                    key={file.path}
                    variant={file.path === selectedPath ? "secondary" : "ghost"}
                    className="h-auto min-w-0 justify-start px-2 py-1.5"
                    aria-label={`View ${file.path} diff`}
                    aria-pressed={file.path === selectedPath}
                    onClick={() => setSelectedPath(file.path)}
                  >
                    <span className="truncate">{file.path}</span>
                    <span className="ml-auto shrink-0 text-xs text-muted-foreground">
                      {changeLabel(file.change)}
                    </span>
                  </Button>
                ))}
              </div>
            </ScrollArea>
          </ResizablePanel>
          <ResizableHandle withHandle aria-label="Resize Code changes browser" />
          <ResizablePanel defaultSize="64%" minSize="16rem">
            <CodeChangePreview file={selectedFile} />
          </ResizablePanel>
        </ResizablePanelGroup>
      )}
    </section>
  )
}

function CodeChangePreview({ file }: { file?: ChangedFileDto }) {
  if (!file) return null

  return (
    <div className="flex size-full min-h-0 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
        <h3 className="min-w-0 flex-1 truncate text-sm font-medium">
          {file.path}
        </h3>
        <Badge variant="secondary">{changeLabel(file.change)}</Badge>
      </div>
      {file.diff?.hunks.length ? (
        <ScrollArea className="min-h-0 flex-1">
          <div className="min-w-max py-2 font-mono text-xs">
            {file.diff.hunks.map((hunk, hunkIndex) => (
              <div key={`${hunk.oldStart}-${hunk.newStart}-${hunkIndex}`}>
                <div className="bg-muted px-3 py-1 text-muted-foreground">
                  @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},
                  {hunk.newLines} @@
                </div>
                {hunk.lines.map((line, lineIndex) => (
                  <div
                    key={`${line.oldLine}-${line.newLine}-${lineIndex}`}
                    className={cn(
                      "grid grid-cols-[3rem_3rem_1rem_minmax(0,1fr)] px-3",
                      line.kind !== "context" && "bg-muted/50",
                    )}
                  >
                    <span className="text-right text-muted-foreground">
                      {line.oldLine ?? ""}
                    </span>
                    <span className="text-right text-muted-foreground">
                      {line.newLine ?? ""}
                    </span>
                    <span>
                      {line.kind === "insert"
                        ? "+"
                        : line.kind === "delete" ? "−" : " "}
                    </span>
                    <span className="whitespace-pre px-1">{line.text}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </ScrollArea>
      ) : (
        <Empty className="min-h-0 flex-1">
          <EmptyHeader>
            <EmptyMedia variant="icon"><FileDiffIcon /></EmptyMedia>
            <EmptyTitle>Diff unavailable</EmptyTitle>
            <EmptyDescription>
              This file changed, but no text diff is available.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
    </div>
  )
}

function changeLabel(change: ChangedFileDto["change"]) {
  return change[0]?.toUpperCase() + change.slice(1)
}
