"use client"

import * as React from "react"

import {
  BrowserRuntimeClient,
  type NotificationRequestedDto,
  type RuntimeClient,
  useRuntimeProjection,
} from "@agent-factory/runtime-client"
import { toast } from "@agent-factory/ui/components/toast"

import { WorkspaceShell } from "@/components/shell/workspace-shell"

const browserRuntimeClient = new BrowserRuntimeClient()
const getNativeTerminalServerSnapshot = () => false

export function RuntimeApplication({
  client = browserRuntimeClient,
}: {
  client?: RuntimeClient
}) {
  const projection = useRuntimeProjection(client)
  const nativeTerminalVisible = React.useSyncExternalStore(
    client.subscribeNativeTerminalVisibility,
    client.getNativeTerminalVisibility,
    getNativeTerminalServerSnapshot,
  )
  const emitIntent = React.useCallback(
    (intent: Parameters<RuntimeClient["dispatch"]>[0]) =>
      client.dispatch(intent),
    [client],
  )
  const createTargetAgent = React.useCallback(
    async (
      intent: Extract<
        Parameters<RuntimeClient["dispatch"]>[0],
        { type: "targetAgent.create" }
      >,
    ) => {
      await client.dispatch(intent)
      return client.getSnapshot().targetWorkspaceError === undefined
    },
    [client],
  )
  const startDraftRun = React.useCallback(
    async (
      runId: string,
      agentDraftId: string,
      environmentId: string,
    ) => {
      await client.dispatch({
        type: "factoryRun.create",
        runId,
        agentDraftId,
        environmentId,
      })
    },
    [client],
  )
  const listVersionFiles = React.useCallback(
    (versionId: string) => client.listVersionFiles(versionId),
    [client],
  )
  const readVersionFile = React.useCallback(
    (versionId: string, path: string) =>
      client.readVersionFile(versionId, path),
    [client],
  )
  const readAgentTranscript = React.useCallback(
    (agentSessionId: string) => client.readAgentTranscript(agentSessionId),
    [client],
  )

  React.useEffect(() => {
    void client.connect()
    return () => client.disconnect()
  }, [client])

  React.useEffect(() => {
    return client.subscribeNotifications((notification) => {
      toast.add({
        title: notification.title,
        description: notification.body,
        type: notificationToastType(notification),
      })
    })
  }, [client])

  const theme = projection.settings?.theme
  React.useEffect(() => {
    if (!theme) return
    const root = document.documentElement
    const media = window.matchMedia("(prefers-color-scheme: dark)")
    const applyTheme = () => {
      const isDark =
        theme === "dark" || (theme === "system" && media.matches)
      root.classList.toggle("dark", isDark)
    }

    applyTheme()
    media.addEventListener("change", applyTheme)
    return () => {
      media.removeEventListener("change", applyTheme)
      root.classList.remove("dark")
    }
  }, [theme])

  return (
    <WorkspaceShell
      projection={projection}
      emitIntent={emitIntent}
      createTargetAgent={createTargetAgent}
      startDraftRun={startDraftRun}
      listVersionFiles={listVersionFiles}
      readVersionFile={readVersionFile}
      readAgentTranscript={readAgentTranscript}
      nativeTerminalVisible={nativeTerminalVisible}
    />
  )
}

function notificationToastType(notification: NotificationRequestedDto) {
  if (
    notification.category === "session_failed" ||
    notification.category === "factory_run_failed"
  ) {
    return "error" as const
  }
  if (notification.category === "factory_run_needs_review") {
    return "warning" as const
  }
  return "success" as const
}
