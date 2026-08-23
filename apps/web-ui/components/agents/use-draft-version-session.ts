import * as React from "react"

import type {
  TargetAgentVersionProjection,
  VersionFilesListDto,
} from "@agent-factory/runtime-client"

import type { VersionFilesInspectorState } from "@/components/agents/version-files-inspector"
import {
  activateVersionTab,
  closeVersionSurface,
  closeVersionTab,
  emptyVersionTabs,
  openVersionTab,
  type VersionTabsState,
} from "@/lib/version-tabs"

type ListVersionFiles = (versionId: string) => Promise<VersionFilesListDto>

export function useDraftVersionSession(
  listVersionFiles: ListVersionFiles,
) {
  const [tabs, setTabs] = React.useState<VersionTabsState>(emptyVersionTabs)
  const [filesById, setFilesById] = React.useState<
    Record<string, VersionFilesInspectorState>
  >({})
  const loadedOrLoading = React.useRef(new Set<string>())
  const sequences = React.useRef<Record<string, number>>({})

  const loadFiles = React.useCallback((
    version: TargetAgentVersionProjection,
  ) => {
    if (loadedOrLoading.current.has(version.id)) return
    loadedOrLoading.current.add(version.id)
    const request = (sequences.current[version.id] ?? 0) + 1
    sequences.current[version.id] = request
    setFilesById((current) => ({
      ...current,
      [version.id]: { version, status: "loading" },
    }))
    void listVersionFiles(version.id)
      .then((files) => {
        if (sequences.current[version.id] !== request) return
        setFilesById((current) => ({
          ...current,
          [version.id]: { version, status: "ready", files },
        }))
      })
      .catch((error: unknown) => {
        if (sequences.current[version.id] !== request) return
        setFilesById((current) => ({
          ...current,
          [version.id]: {
            version,
            status: "error",
            error: error instanceof Error
              ? error.message
              : "The runtime rejected the Version file request.",
          },
        }))
      })
  }, [listVersionFiles])

  const openVersion = React.useCallback((
    version: TargetAgentVersionProjection,
  ) => {
    setTabs((current) => openVersionTab(current, version.id))
    loadFiles(version)
  }, [loadFiles])

  const activateTab = React.useCallback((versionId: string) => {
    setTabs((current) => activateVersionTab(current, versionId))
  }, [])

  const closeTab = React.useCallback((versionId: string) => {
    loadedOrLoading.current.delete(versionId)
    sequences.current[versionId] = (sequences.current[versionId] ?? 0) + 1
    setTabs((current) => closeVersionTab(current, versionId))
    setFilesById((current) => {
      if (!(versionId in current)) return current
      const next = { ...current }
      delete next[versionId]
      return next
    })
  }, [])

  const closeSurface = React.useCallback(() => {
    loadedOrLoading.current.clear()
    for (const id of Object.keys(sequences.current)) {
      sequences.current[id] = (sequences.current[id] ?? 0) + 1
    }
    setTabs(closeVersionSurface())
    setFilesById({})
  }, [])

  return {
    tabs,
    filesById,
    openVersion,
    activateTab,
    closeTab,
    closeSurface,
  }
}
