"use client"

import { XIcon } from "lucide-react"

import type {
  RuntimeIntent,
  TargetAgentProjection,
  TargetAgentVersionProjection,
  VersionFileReadDto,
} from "@agent-factory/runtime-client"
import { Button } from "@agent-factory/ui/components/button"
import { Separator } from "@agent-factory/ui/components/separator"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-factory/ui/components/tooltip"
import { cn } from "@agent-factory/ui/lib/utils"

import { AgentVersionButtonGroup } from "@/components/agents/agent-version-picker"
import {
  VersionFilesInspector,
  type VersionFilesInspectorState,
} from "@/components/agents/version-files-inspector"
import {
  isVersionSurfaceVisible,
  type VersionTabsState,
} from "@/lib/version-tabs"

export function VersionTabSurface({
  agent,
  versions,
  tabs,
  filesById,
  emitIntent,
  readVersionFile,
  onOpenVersion,
  onActivateTab,
  onCloseTab,
  onCloseSurface,
}: {
  agent: TargetAgentProjection
  versions: readonly TargetAgentVersionProjection[]
  tabs: VersionTabsState
  filesById: Readonly<Record<string, VersionFilesInspectorState>>
  emitIntent: (intent: RuntimeIntent) => Promise<void>
  readVersionFile: (
    versionId: string,
    path: string,
  ) => Promise<VersionFileReadDto>
  onOpenVersion: (version: TargetAgentVersionProjection) => void
  onActivateTab: (versionId: string) => void
  onCloseTab: (versionId: string) => void
  onCloseSurface: () => void
}) {
  if (!isVersionSurfaceVisible(tabs)) return null

  const versionsById = new Map(versions.map((version) => [version.id, version]))
  const openVersions = tabs.openIds.flatMap((id) => {
    const version = versionsById.get(id) ?? filesById[id]?.version
    return version ? [version] : []
  })
  const activeId = tabs.activeId && openVersions.some((version) =>
    version.id === tabs.activeId)
    ? tabs.activeId
    : openVersions[openVersions.length - 1]?.id

  if (!activeId || openVersions.length === 0) return null

  return (
    <section
      aria-label="Versions"
      className="flex size-full min-h-0 min-w-0 flex-col"
    >
      <header className="flex h-11 shrink-0 items-center gap-1 px-2">
        <div
          role="tablist"
          aria-label="Open Versions"
          className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto"
        >
          {openVersions.map((version) => {
            const selected = version.id === activeId
            const label = `v${version.version}`
            return (
              <div
                key={version.id}
                className="flex shrink-0 items-center"
              >
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  id={`version-tab-trigger-${version.id}`}
                  role="tab"
                  aria-selected={selected}
                  aria-controls={`version-tab-${version.id}`}
                  className={cn(
                    selected
                      ? "bg-muted/50 text-foreground"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                  onClick={() => onActivateTab(version.id)}
                >
                  {label}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Close ${label}`}
                  onClick={() => onCloseTab(version.id)}
                >
                  <XIcon />
                </Button>
              </div>
            )
          })}
        </div>
        <AgentVersionButtonGroup
          versions={versions}
          onOpenVersion={onOpenVersion}
        />
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Close Versions"
                onClick={onCloseSurface}
              />
            }
          >
            <XIcon />
          </TooltipTrigger>
          <TooltipContent>Close Versions</TooltipContent>
        </Tooltip>
      </header>
      <Separator className="h-px w-full" />
      <div className="min-h-0 flex-1">
        {openVersions.map((version) => {
          const state = filesById[version.id] ?? {
            version,
            status: "loading" as const,
          }
          const selected = version.id === activeId
          return (
            <div
              key={version.id}
              id={`version-tab-${version.id}`}
              role="tabpanel"
              aria-labelledby={`version-tab-trigger-${version.id}`}
              hidden={!selected}
              className={cn(
                "size-full min-h-0 min-w-0",
                selected ? "flex flex-col" : "hidden",
              )}
            >
              <VersionFilesInspector
                state={state}
                agent={agent}
                emitIntent={emitIntent}
                readVersionFile={readVersionFile}
                onClose={() => onCloseTab(version.id)}
              />
            </div>
          )
        })}
      </div>
    </section>
  )
}
