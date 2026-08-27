import * as React from "react"
import dynamic from "next/dynamic"
import {
  BotIcon,
  FolderOpenIcon,
  Globe2Icon,
  GitBranchIcon,
  GripVerticalIcon,
  HistoryIcon,
  MoreHorizontalIcon,
  PanelLeftIcon,
  PencilIcon,
  PlusIcon,
  SearchIcon,
  TagIcon,
  TerminalIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react"

import type {
  AgentDraftProjection,
  FactoryRunProjection,
  RuntimeIntent,
  TargetAgentProjection,
  TargetAgentVersionProjection,
  TargetAgentWorkGroupProjection,
  VersionFileReadDto,
  VersionFilesListDto,
  WorkContextProjection,
  WorkspacePaneProjection,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@agent-factory/ui/components/alert-dialog"
import { Alert, AlertDescription, AlertTitle } from "@agent-factory/ui/components/alert"
import { Badge } from "@agent-factory/ui/components/badge"
import { Button } from "@agent-factory/ui/components/button"
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@agent-factory/ui/components/command"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@agent-factory/ui/components/dropdown-menu"
import {
  Empty,
  EmptyContent,
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
import { Separator } from "@agent-factory/ui/components/separator"
import {
  SidebarInset,
  SidebarProvider,
  useSidebar,
} from "@agent-factory/ui/components/sidebar"
import { Skeleton } from "@agent-factory/ui/components/skeleton"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-factory/ui/components/tooltip"
import { cn } from "@agent-factory/ui/lib/utils"

import {
  AgentDraftWorkspace,
  DraftOverview,
  type DraftWorkflowChrome,
} from "@/components/agents/agent-draft-workspace"
import {
  SessionWorkspace,
  type ReadAgentTranscript,
} from "@/components/agents/session-workspace"
import { sortAgentVersions } from "@/components/agents/agent-version-picker"

import { CreateDraftDialog } from "@/components/agents/create-draft-dialog"
import { useDraftVersionSession } from "@/components/agents/use-draft-version-session"
import { AgentFactorySidebar } from "@/components/shell/agent-factory-sidebar"
import {
  DraftOverviewPanel,
  DraftOverviewTrigger,
  useDraftOverviewSurface,
} from "@/components/shell/draft-overview-surface"
import { SidebarResizeHandle } from "@/components/shell/sidebar-resize-handle"
import {
  WorkCreationWorkspace,
  type WorkCreationDraft,
} from "@/components/shell/work-creation-workspace"
import {
  draftWindowLabel,
  getDraftWindowSearchServerSnapshot,
  getDraftWindowSearchSnapshot,
  readDraftWindowTarget,
  subscribeDraftWindowSearch,
  type DraftWindowTarget,
} from "@/lib/draft-window"
import {
  getSidebarWidthServerSnapshot,
  getSidebarWidthSnapshot,
  subscribeSidebarWidth,
} from "@/lib/sidebar-width-store"
import { handleNativeWindowDragPointerDown } from "@/lib/native-window-drag"

const SettingsView = dynamic(
  () => import("@/components/settings/settings-view").then((module) => module.SettingsView),
  { ssr: false },
)

const VersionTabSurface = dynamic(
  () => import("@/components/agents/version-tab-surface").then(
    (module) => module.VersionTabSurface,
  ),
  { ssr: false },
)

const CodeChangesInspector = dynamic(
  () => import("@/components/agents/code-changes-inspector").then(
    (module) => module.CodeChangesInspector,
  ),
  { ssr: false },
)

type WorkspaceInspectorState = { kind: "code_changes"; run: FactoryRunProjection }

type EmitIntent = (intent: RuntimeIntent) => Promise<void>
type CreateTargetAgent = (
  intent: Extract<RuntimeIntent, { type: "targetAgent.create" }>,
) => Promise<boolean>
type StartDraftRun = (
  runId: string,
  agentDraftId: string,
  environmentId: string,
  objective: string,
) => Promise<void>
type ListVersionFiles = (versionId: string) => Promise<VersionFilesListDto>
type ReadVersionFile = (
  versionId: string,
  path: string,
) => Promise<VersionFileReadDto>
type CreateDraft = (
  agent: TargetAgentProjection,
  version?: TargetAgentVersionProjection,
) => void
type OpenDraft = (
  targetAgentId: string,
  workspaceBindingId: string,
  draftId?: string,
) => void

export function WorkspaceShell({
  projection,
  emitIntent,
  createTargetAgent,
  startDraftRun,
  listVersionFiles,
  readVersionFile,
  readAgentTranscript,
  nativeTerminalVisible = false,
}: {
  projection: WorkspaceProjection
  emitIntent: EmitIntent
  createTargetAgent: CreateTargetAgent
  startDraftRun: StartDraftRun
  listVersionFiles: ListVersionFiles
  readVersionFile: ReadVersionFile
  readAgentTranscript?: ReadAgentTranscript
  nativeTerminalVisible?: boolean
}) {
  const draftWindowSearch = React.useSyncExternalStore(
    subscribeDraftWindowSearch,
    getDraftWindowSearchSnapshot,
    getDraftWindowSearchServerSnapshot,
  )
  const draftWindowTarget = readDraftWindowTarget(draftWindowSearch ?? "")
  const [searchOpen, setSearchOpen] = React.useState(false)
  const [creationDraft, setCreationDraft] = React.useState<WorkCreationDraft>()
  // `"environments"` opens Settings straight into a new-Environment draft, for the
  // first-run nudge below.
  const [settingsOpen, setSettingsOpen] = React.useState<false | true | "environments">(false)
  const [sidebarOpen, setSidebarOpen] = React.useState(
    !draftWindowTarget && !nativeTerminalVisible,
  )
  // Opening the external native workspace is a user event, so hide the
  // sidebar at that boundary. Afterward the user may reopen it normally;
  // native visibility must not become a permanent override of React state.
  const emitIntentWithShellPresentation = React.useCallback(
    async (intent: RuntimeIntent) => {
      if (
        intent.type === "agentDraft.toggleWorkspace" &&
        !nativeTerminalVisible
      ) {
        setSidebarOpen(false)
      }
      await emitIntent(intent)
    },
    [emitIntent, nativeTerminalVisible],
  )
  const effectiveSidebarOpen = sidebarOpen
  const [inspector, setInspector] = React.useState<WorkspaceInspectorState>()
  // Restore whatever the user had open after Define your agent closes.
  const sidebarOpenBeforeCreationRef = React.useRef(true)
  const openAgentCreation = React.useCallback(() => {
    sidebarOpenBeforeCreationRef.current = sidebarOpen
    setSidebarOpen(false)
    setCreationDraft({ kind: "agent" })
  }, [sidebarOpen])
  const closeAgentCreation = React.useCallback(() => {
    setCreationDraft(undefined)
    setSidebarOpen(sidebarOpenBeforeCreationRef.current)
  }, [])
  const sidebarWidth = React.useSyncExternalStore(
    subscribeSidebarWidth,
    getSidebarWidthSnapshot,
    getSidebarWidthServerSnapshot,
  )
  const closeInspector = React.useCallback(() => {
    setInspector(undefined)
  }, [])
  const openCodeChangesInspector = React.useCallback((
    run: FactoryRunProjection,
  ) => {
    setInspector({ kind: "code_changes", run })
  }, [])
  const [createDraft, setCreateDraft] = React.useState<{
    agent: TargetAgentProjection
    version?: TargetAgentVersionProjection
  }>()
  const [removeAgentId, setRemoveAgentId] = React.useState<string>()
  const openDraft = React.useCallback((
    targetAgentId: string,
    workspaceBindingId: string,
    draftId?: string,
  ) => {
    closeInspector()
    void emitIntent({
      type: "workspacePane.openPrimary",
      targetAgentId,
      workspaceBindingId,
      workItemId: draftId,
      workItemKind: draftId ? "agent_draft" : undefined,
    })
  }, [closeInspector, emitIntent])

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        setSearchOpen(true)
      }
      if (event.key === "," && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        setSettingsOpen(true)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [])

  if (draftWindowTarget) {
    return (
      <DedicatedDraftWindow
        target={draftWindowTarget}
        projection={projection}
        emitIntent={emitIntent}
        startDraftRun={startDraftRun}
        listVersionFiles={listVersionFiles}
        readVersionFile={readVersionFile}
        inspector={inspector}
        nativeTerminalVisible={nativeTerminalVisible}
        onOpenCodeChanges={openCodeChangesInspector}
        onCloseInspector={closeInspector}
      />
    )
  }

  return (
    <SidebarProvider
      open={effectiveSidebarOpen}
      onOpenChange={setSidebarOpen}
      onPointerDownCapture={handleNativeWindowDragPointerDown}
      className="relative"
      style={{ "--sidebar-width": `${sidebarWidth}px` } as React.CSSProperties}
    >
      {!settingsOpen ? <WindowActions onSearch={() => setSearchOpen(true)} /> : null}
      {!settingsOpen ? (
        <AgentFactorySidebar
          projection={projection}
          onAddTarget={openAgentCreation}
          onOpenSettings={() => setSettingsOpen(true)}
          onOpenDraft={openDraft}
          onCreateDraft={(agent, version) =>
            setCreateDraft({ agent, version })
          }
          onRemoveAgent={setRemoveAgentId}
        />
      ) : null}
      {!settingsOpen && effectiveSidebarOpen ? (
        <nav aria-label="Sidebar resize" className="contents">
          <SidebarResizeHandle width={sidebarWidth} onCollapse={() => setSidebarOpen(false)} />
        </nav>
      ) : null}
      <SidebarInset className="h-svh min-w-0 overflow-hidden">
        <h1 className="sr-only">Agent Factory</h1>
        {settingsOpen ? (
          <SettingsView
            projection={projection}
            emitIntent={emitIntent}
            initialSection={settingsOpen === "environments" ? "environments" : "general"}
            createEnvironmentRequested={settingsOpen === "environments"}
            onClose={() => setSettingsOpen(false)}
          />
        ) : (
          <ResizablePanelGroup
            id="workspace-inspector-columns"
            role="region"
            aria-label="Agent Factory workspace"
            orientation="horizontal"
            className="min-h-0 flex-1"
          >
            <ResizablePanel
              id="workspace-content"
              defaultSize={inspector ? "58%" : "100%"}
              minSize="20rem"
              className="flex min-h-0 min-w-0"
            >
              <WorkspaceContent
                projection={projection}
                emitIntent={emitIntentWithShellPresentation}
                createTargetAgent={createTargetAgent}
                startDraftRun={startDraftRun}
                listVersionFiles={listVersionFiles}
                readVersionFile={readVersionFile}
                readAgentTranscript={readAgentTranscript}
                nativeTerminalVisible={nativeTerminalVisible}
                onOpenCodeChanges={openCodeChangesInspector}
                creationDraft={creationDraft}
                onAddTarget={openAgentCreation}
                onCloseCreation={closeAgentCreation}
                onCreateEnvironment={() => setSettingsOpen("environments")}
                onCreateDraft={(agent, version) =>
                  setCreateDraft({ agent, version })
                }
                onOpenDraft={openDraft}
                sidebarOpen={effectiveSidebarOpen}
              />
            </ResizablePanel>
            {inspector ? (
              <>
                <ResizableHandle
                  data-native-no-drag
                  withHandle
                  aria-label="Resize workspace and Inspector"
                />
                <ResizablePanel
                  id="workspace-inspector"
                  defaultSize="42%"
                  minSize="24rem"
                  className="min-h-0 min-w-0"
                >
                  <CodeChangesInspector
                    key={inspector.run.id}
                    run={inspector.run}
                    onClose={closeInspector}
                  />
                </ResizablePanel>
              </>
            ) : null}
          </ResizablePanelGroup>
        )}
      </SidebarInset>
      <TargetSearch
        open={searchOpen}
        onOpenChange={setSearchOpen}
        projection={projection}
        emitIntent={emitIntent}
      />
      {createDraft ? (
        <CreateDraftDialog
          agent={createDraft.agent}
          version={createDraft.version}
          emitIntent={emitIntent}
          open
          onOpenChange={(open) => {
            if (!open) setCreateDraft(undefined)
          }}
          showTrigger={false}
        />
      ) : null}
      <AlertDialog
        open={removeAgentId !== undefined}
        onOpenChange={(open) => {
          if (!open) setRemoveAgentId(undefined)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove this Agent?</AlertDialogTitle>
            <AlertDialogDescription>
              The Agent leaves Agent Factory. Project files on disk are not
              deleted.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (!removeAgentId) return
                void emitIntent({
                  type: "targetAgent.remove",
                  targetAgentId: removeAgentId,
                })
                setRemoveAgentId(undefined)
              }}
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SidebarProvider>
  )
}

function WorkspaceContent({ projection, emitIntent, createTargetAgent, startDraftRun, creationDraft, onAddTarget, onCloseCreation, onCreateEnvironment, onCreateDraft, onOpenDraft, onOpenCodeChanges, listVersionFiles, readVersionFile, readAgentTranscript, nativeTerminalVisible, sidebarOpen }: { projection: WorkspaceProjection; emitIntent: EmitIntent; createTargetAgent: CreateTargetAgent; startDraftRun: StartDraftRun; creationDraft?: WorkCreationDraft; onAddTarget: () => void; onCloseCreation: () => void; onCreateEnvironment: () => void; onCreateDraft: CreateDraft; onOpenDraft: OpenDraft; onOpenCodeChanges: (run: FactoryRunProjection) => void; listVersionFiles: ListVersionFiles; readVersionFile: ReadVersionFile; readAgentTranscript?: ReadAgentTranscript; nativeTerminalVisible: boolean; sidebarOpen: boolean }) {

  if (projection.connection === "loading") {
    return (
      <EmptyWorkspaceFrame sidebarOpen={sidebarOpen}>
        <WorkspaceLoading detail={projection.connectionDetail} />
      </EmptyWorkspaceFrame>
    )
  }
  if (projection.connection === "error") {
    return (
      <EmptyWorkspaceFrame sidebarOpen={sidebarOpen}><div className="flex flex-1 items-center justify-center p-6">
        <Alert variant="destructive" className="max-w-lg">
          <AlertTitle>Runtime connection failed</AlertTitle>
          <AlertDescription>{projection.connectionDetail ?? "The Rust runtime is unavailable."}</AlertDescription>
          <Button variant="outline" onClick={() => void emitIntent({ type: "runtime.retry" })}>Retry connection</Button>
        </Alert>
      </div></EmptyWorkspaceFrame>
    )
  }
  if (creationDraft) {
    return (
      <WorkCreationWorkspace
        key="agent"
        createTargetAgent={createTargetAgent}
        runtimeError={projection.targetWorkspaceError}
        sidebarOpen={sidebarOpen}
        onClose={onCloseCreation}
      />
    )
  }
  if (projection.targetWorkspace.targetGroups.length === 0) {
    return (
      <EmptyWorkspaceFrame sidebarOpen={false}>
        <div className="flex flex-1 items-center justify-center overflow-hidden p-6">
          <Empty className="max-w-md border">
            <EmptyHeader>
              <EmptyMedia variant="icon"><FolderOpenIcon /></EmptyMedia>
              <EmptyTitle>Create your first Agent</EmptyTitle>
              <EmptyDescription>Define its objective and success criteria, then bind it to a workspace.</EmptyDescription>
            </EmptyHeader>
            <EmptyContent><Button disabled={projection.connection !== "ready"} onClick={onAddTarget}>Create Agent</Button></EmptyContent>
          </Empty>
        </div>
      </EmptyWorkspaceFrame>
    )
  }
  // Environment setup follows the initial Agent draft. Once an Agent exists,
  // this nudges the user into the Environment form before they can run it.
  if (projection.connection === "ready" && projection.environments.length === 0) {
    return (
      <EmptyWorkspaceFrame sidebarOpen={sidebarOpen}><Empty className="m-4 flex-1 border">
        <EmptyHeader>
          <EmptyMedia variant="icon"><Globe2Icon /></EmptyMedia>
          <EmptyTitle>Create your first Environment</EmptyTitle>
          <EmptyDescription>
            An Environment composes the Intelligence Provider, Environment
            Variables, Skills, Tools, agents, and permissions used by runs. Runs
            can start with any ready Environment.
          </EmptyDescription>
        </EmptyHeader>
        <EmptyContent><Button onClick={onCreateEnvironment}>Create Environment</Button></EmptyContent>
      </Empty></EmptyWorkspaceFrame>
    )
  }
  const panes = projection.targetWorkspace.panes.toSorted(
    (a, b) => a.position - b.position,
  )
  if (panes.length === 0) {
    return (
      <RecentAgents
        groups={projection.targetWorkspace.targetGroups}
        onOpenDraft={onOpenDraft}
        onCreateDraft={onCreateDraft}
      />
    )
  }
  return (
    <ResizablePanelGroup
      id="target-workspace-columns"
      role="region"
      aria-label="Target workspace"
      orientation="horizontal"
      className="min-h-0 flex-1"
      defaultLayout={Object.fromEntries(
        panes.map((pane) => [pane.id, pane.widthBasisPoints / 10_000]),
      )}
      onLayoutChanged={(layout, meta) => {
        if (!meta.isUserInteraction) return
        const total = panes.reduce(
          (sum, pane) => sum + (layout[pane.id] ?? 0),
          0,
        )
        if (total <= 0) return
        let assigned = 0
        const persisted = panes.map((pane, index) => {
          const widthBasisPoints = index + 1 === panes.length
            ? 10_000 - assigned
            : Math.max(
                1,
                Math.floor(((layout[pane.id] ?? 0) / total) * 10_000),
              )
          assigned += widthBasisPoints
          return { workspacePaneId: pane.id, widthBasisPoints }
        })
        void emitIntent({ type: "workspacePane.resize", layout: persisted })
      }}
    >
      {panes.map((pane, index) => (
        <React.Fragment key={pane.id}>
          {index > 0 ? (
            <ResizableHandle
              data-native-no-drag
              withHandle
              aria-label={`Resize columns ${index} and ${index + 1}`}
            />
          ) : null}
          <ResizablePanel
            id={pane.id}
            defaultSize={`${pane.widthBasisPoints / 100}%`}
            minSize="16rem"
            className="min-h-0 min-w-0"
          >
            <WorkspacePane
              pane={pane}
              position={index}
              paneCount={panes.length}
              projection={projection}
              emitIntent={emitIntent}
              startDraftRun={startDraftRun}
              listVersionFiles={listVersionFiles}
              readVersionFile={readVersionFile}
              readAgentTranscript={readAgentTranscript}
              nativeTerminalVisible={nativeTerminalVisible}
              onOpenCodeChanges={onOpenCodeChanges}
              sidebarOpen={sidebarOpen}
            />
          </ResizablePanel>
        </React.Fragment>
      ))}
    </ResizablePanelGroup>
  )
}

function WorkspacePane({ pane, position, paneCount, projection, emitIntent, startDraftRun, onOpenCodeChanges, listVersionFiles, readVersionFile, readAgentTranscript, nativeTerminalVisible, sidebarOpen }: { pane: WorkspacePaneProjection; position: number; paneCount: number; projection: WorkspaceProjection; emitIntent: EmitIntent; startDraftRun: StartDraftRun; onOpenCodeChanges: (run: FactoryRunProjection) => void; listVersionFiles: ListVersionFiles; readVersionFile: ReadVersionFile; readAgentTranscript?: ReadAgentTranscript; nativeTerminalVisible: boolean; sidebarOpen: boolean }) {
  const context = projection.targetWorkspace.workContexts.find((candidate) => candidate.id === pane.workContextId)
  const data = context ? paneData(projection, context) : undefined
  if (!context || !data) return null
  return (
    <WorkspacePaneSession
      key={context.id}
      pane={pane}
      position={position}
      paneCount={paneCount}
      projection={projection}
      emitIntent={emitIntent}
      startDraftRun={startDraftRun}
      onOpenCodeChanges={onOpenCodeChanges}
      listVersionFiles={listVersionFiles}
      readVersionFile={readVersionFile}
      readAgentTranscript={readAgentTranscript}
      nativeTerminalVisible={nativeTerminalVisible}
      sidebarOpen={sidebarOpen}
      data={data}
    />
  )
}

function WorkspacePaneSession({ pane, position, paneCount, projection, emitIntent, startDraftRun, onOpenCodeChanges, listVersionFiles, readVersionFile, readAgentTranscript, nativeTerminalVisible, sidebarOpen, data }: { pane: WorkspacePaneProjection; position: number; paneCount: number; projection: WorkspaceProjection; emitIntent: EmitIntent; startDraftRun: StartDraftRun; onOpenCodeChanges: (run: FactoryRunProjection) => void; listVersionFiles: ListVersionFiles; readVersionFile: ReadVersionFile; readAgentTranscript?: ReadAgentTranscript; nativeTerminalVisible: boolean; sidebarOpen: boolean; data: PaneData }) {
  const [overviewBind, overview] = useDraftOverviewSurface()
  const [draftWorkflow, setDraftWorkflow] = React.useState<DraftWorkflowChrome | null>(
    null,
  )
  const [isEditing, setIsEditing] = React.useState(false)
  const versions = useDraftVersionSession(listVersionFiles)

  const focused = pane.id === projection.targetWorkspace.focusedPaneId
  const draftOverviewAvailable = !data.item ||
    data.item.kind === "agent_draft" ||
    data.item.kind === "factory_run"
  const draftOverviewId = `draft-overview-${pane.id}`
  const focusedOverviewRun = data.item?.kind === "factory_run"
    ? projection.factoryRuns.find((run) => run.id === data.item?.id)
    : undefined
  const overviewDraftId = data.draft?.id ?? focusedOverviewRun?.agentDraftId
  const overviewDraft = data.draft ?? (overviewDraftId
    ? data.group.drafts.find((draft) => draft.id === overviewDraftId)
    : undefined)
  const overviewRun = focusedOverviewRun ?? projection.factoryRuns.find(
    (run) =>
      run.agentDraftId === overviewDraftId &&
      !["passed", "cancelled"].includes(run.state),
  )
  const draftOverviewContent = draftOverviewAvailable ? (
    <DraftOverview
      agent={data.group.targetAgent}
      draft={overviewDraft}
      versions={data.group.versions}
      environments={projection.environments}
      selectedRun={overviewRun}
      emitIntent={emitIntent}
      onOpenCodeChanges={(run) => {
        overview.setOpen(false)
        onOpenCodeChanges(run)
      }}
    />
  ) : null
  const draftOverview = draftOverviewContent ? (
    <DraftOverviewTrigger id={draftOverviewId} overview={overview}>
      {draftOverviewContent}
    </DraftOverviewTrigger>
  ) : null
  const versionSurface = versions.tabs.openIds.length > 0 ? (
    <VersionTabSurface
      agent={data.group.targetAgent}
      versions={data.group.versions}
      tabs={versions.tabs}
      filesById={versions.filesById}
      emitIntent={emitIntent}
      readVersionFile={readVersionFile}
      onOpenVersion={versions.openVersion}
      onActivateTab={versions.activateTab}
      onCloseTab={versions.closeTab}
      onCloseSurface={versions.closeSurface}
    />
  ) : null
  return (
    <section
      ref={overviewBind}
      aria-label={`${data.title} pane`}
      className={cn("flex size-full min-h-0 min-w-0 flex-col", focused && "bg-muted/20")}
      onPointerDown={() => !focused && void emitIntent({ type: "workspacePane.focus", workspacePaneId: pane.id })}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes("application/x-agent-factory-pane")) {
          event.preventDefault()
        }
      }}
      onDrop={(event) => {
        const draggedPaneId = event.dataTransfer.getData(
          "application/x-agent-factory-pane",
        )
        if (draggedPaneId && draggedPaneId !== pane.id) {
          event.preventDefault()
          void emitIntent({
            type: "workspacePane.move",
            workspacePaneId: draggedPaneId,
            position,
          })
        }
      }}
    >
      {isEditing ? null : (
        <PaneHeader
          pane={pane}
          data={data}
          emitIntent={emitIntent}
          position={position}
          paneCount={paneCount}
          sidebarOpen={sidebarOpen}
          draftWorkflow={draftWorkflow}
          draftOverview={draftOverview}
          terminalDraft={overviewDraft}
          nativeTerminalVisible={nativeTerminalVisible}
        />
      )}
      {isEditing ? null : <Separator />}
      <div className="flex min-h-0 min-w-0 flex-1">
        <div className="min-h-0 min-w-0 flex-1">
          <PaneBody
            data={data}
            projection={projection}
            emitIntent={emitIntent}
            startDraftRun={startDraftRun}
            sidebarOpen={sidebarOpen}
            onDraftWorkflowChange={setDraftWorkflow}
            onEditStateChange={setIsEditing}
            versionSurface={versionSurface}
            readAgentTranscript={readAgentTranscript}
          />
        </div>
        {isEditing ? null : draftOverviewContent ? (
          <DraftOverviewPanel id={draftOverviewId} overview={overview}>
            {draftOverviewContent}
          </DraftOverviewPanel>
        ) : null}
      </div>
    </section>
  )
}

function PaneHeader({ pane, data, emitIntent, position, paneCount, sidebarOpen, draftWorkflow, draftOverview, terminalDraft, nativeTerminalVisible }: { pane: WorkspacePaneProjection; data: PaneData; emitIntent: EmitIntent; position: number; paneCount: number; sidebarOpen: boolean; draftWorkflow: DraftWorkflowChrome | null; draftOverview: React.ReactNode; terminalDraft?: AgentDraftProjection; nativeTerminalVisible: boolean }) {
  const terminalAvailable = Boolean(terminalDraft)
  const title = data.title
  const showDraftBadge = data.item?.kind === "agent_draft" ||
    data.item?.kind === "factory_run"
  return (
    <header
      data-native-drag-region
      className={cn(
        "flex h-11 shrink-0 items-center gap-1 overflow-hidden transition-[padding] duration-200 ease-linear",
        position === 0 && !sidebarOpen ? "pl-32 pr-2" : "px-2",
      )}
    >
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              data-native-no-drag
              variant="ghost"
              size="icon-sm"
              draggable
              aria-label={`Move ${title} column`}
              aria-keyshortcuts="ArrowLeft ArrowRight"
              onDragStart={(event) => {
                event.dataTransfer.effectAllowed = "move"
                event.dataTransfer.setData(
                  "application/x-agent-factory-pane",
                  pane.id,
                )
              }}
              onKeyDown={(event) => {
                const delta = event.key === "ArrowLeft"
                  ? -1
                  : event.key === "ArrowRight"
                    ? 1
                    : 0
                if (delta === 0) return
                event.preventDefault()
                const destination = Math.max(
                  0,
                  Math.min(paneCount - 1, position + delta),
                )
                if (destination !== position) {
                  void emitIntent({
                    type: "workspacePane.move",
                    workspacePaneId: pane.id,
                    position: destination,
                  })
                }
              }}
            />
          }
        >
          <GripVerticalIcon />
        </TooltipTrigger>
        <TooltipContent>Drag or use Left/Right to move column</TooltipContent>
      </Tooltip>
      <p className="min-w-0 truncate text-sm font-medium">{title}</p>
      {showDraftBadge ? (
        <Badge variant="secondary" className="shrink-0">
          Draft
        </Badge>
      ) : null}
      {draftWorkflow ? (
        <div data-native-no-drag className="shrink-0">
          <DropdownMenu>
            <DropdownMenuTrigger
              render={(
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Actions for ${draftWorkflow.draftName}`}
                />
              )}
            >
              <MoreHorizontalIcon />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuGroup>
                {draftWorkflow.canEdit ? (
                  <DropdownMenuItem onClick={draftWorkflow.onEdit}>
                    <PencilIcon />
                    Edit
                  </DropdownMenuItem>
                ) : null}
                {draftWorkflow.onCreateDraft ? (
                  <DropdownMenuItem onClick={draftWorkflow.onCreateDraft}>
                    <PlusIcon />
                    Create Draft
                  </DropdownMenuItem>
                ) : (
                  <DropdownMenuItem
                    disabled={draftWorkflow.publishDisabled}
                    onClick={draftWorkflow.onCreateVersion}
                  >
                    <TagIcon />
                    Create Version
                  </DropdownMenuItem>
                )}
                {draftWorkflow.onDiscardDraft ? (
                  <DropdownMenuItem
                    variant="destructive"
                    disabled={draftWorkflow.discardDisabled}
                    onClick={draftWorkflow.onDiscardDraft}
                  >
                    <Trash2Icon />
                    Discard Draft
                  </DropdownMenuItem>
                ) : null}
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ) : null}
      <div data-native-no-drag className="ml-auto flex shrink-0 items-center gap-1">
        {draftWorkflow?.onOpenRunHistory ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={draftWorkflow.onOpenRunHistory}
          >
            <HistoryIcon data-icon="inline-start" />
            Run History
          </Button>
        ) : null}
        {draftOverview}
        <Tooltip>
          <TooltipTrigger render={
            <Button
              variant={nativeTerminalVisible ? "secondary" : "ghost"}
              size="icon-sm"
              aria-label={nativeTerminalVisible ? "Close Terminal" : "Open Terminal"}
              aria-pressed={nativeTerminalVisible}
              disabled={!terminalAvailable}
              onClick={() => terminalDraft && void emitIntent({
                type: "agentDraft.toggleWorkspace",
                agentDraftId: terminalDraft.id,
              })}
            />
          }>
            <TerminalIcon />
          </TooltipTrigger>
          <TooltipContent>
            {!terminalAvailable
              ? "Open a Draft to use its Herdr terminal"
              : nativeTerminalVisible ? "Close Terminal" : "Open Terminal"}
          </TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger render={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="Close column"
              onClick={() => void emitIntent({
                type: "workspacePane.close",
                workspacePaneId: pane.id,
              })}
            />
          }>
            <XIcon />
          </TooltipTrigger>
          <TooltipContent>Close column</TooltipContent>
        </Tooltip>
      </div>
    </header>
  )
}

function PaneBody({ data, projection, emitIntent, startDraftRun, sidebarOpen, onDraftWorkflowChange, onEditStateChange, versionSurface, readAgentTranscript }: { data: PaneData; projection: WorkspaceProjection; emitIntent: EmitIntent; startDraftRun: StartDraftRun; sidebarOpen: boolean; onDraftWorkflowChange: (chrome: DraftWorkflowChrome | null) => void; onEditStateChange?: (editing: boolean) => void; versionSurface?: React.ReactNode; readAgentTranscript?: ReadAgentTranscript }) {

  if (data.item?.kind === "coding_thread" || data.item?.kind === "evaluation_thread") {
    const session = projection.sessions.find((candidate) => candidate.id === data.item?.id)
    if (!session) return <UnavailableWork />
    return (
      <SessionWorkspace
        session={session}
        readTranscript={readAgentTranscript}
      />
    )
  }
  if (
    !data.item ||
    data.item.kind === "agent_draft" ||
    data.item.kind === "factory_run"
  ) {
    const selectedRun = data.item?.kind === "factory_run"
      ? projection.factoryRuns.find((run) => run.id === data.item?.id)
      : undefined
    const draftId = data.item?.kind === "agent_draft"
      ? data.item.id
      : selectedRun?.agentDraftId
    const draft = draftId
      ? data.group.drafts.find((candidate) => candidate.id === draftId)
      : undefined
    if (data.item?.kind === "factory_run" && (!selectedRun || !draft)) {
      return <UnavailableWork />
    }
    return (
      <AgentDraftWorkspace
        agent={data.group.targetAgent}
        draft={draft}
        project={data.project}
        runs={projection.factoryRuns.filter(
          (run) => run.agentDraftId === draft?.id,
        )}
        sessions={projection.sessions}
        liveAgents={projection.liveAgents}
        herdr={projection.herdr}
        selectedRunId={selectedRun?.id}
        environments={projection.environments}
        runtimeError={projection.targetWorkspaceError ?? projection.runError}
        emitIntent={emitIntent}
        startDraftRun={startDraftRun}
        sidebarOpen={sidebarOpen}
        onDraftWorkflowChange={onDraftWorkflowChange}
        onEditStateChange={onEditStateChange}
        versionSurface={versionSurface}
        versions={data.group.versions}
        readAgentTranscript={readAgentTranscript}
      />
    )
  }
  return <UnavailableWork />
}

function UnavailableWork() {
  return (
    <Empty className="size-full">
      <EmptyHeader>
        <EmptyTitle>Work unavailable</EmptyTitle>
        <EmptyDescription>
          This internal work item is no longer exposed in Agent navigation.
        </EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}

/** Secondary OS window bound to one Draft; independent of main-window panes. */
function DedicatedDraftWindow({
  target,
  projection,
  emitIntent,
  startDraftRun,
  listVersionFiles,
  readVersionFile,
  nativeTerminalVisible,
  inspector,
  onOpenCodeChanges,
  onCloseInspector,
}: {
  target: DraftWindowTarget
  projection: WorkspaceProjection
  emitIntent: EmitIntent
  startDraftRun: StartDraftRun
  listVersionFiles: ListVersionFiles
  readVersionFile: ReadVersionFile
  nativeTerminalVisible: boolean
  inspector?: WorkspaceInspectorState
  onOpenCodeChanges: (run: FactoryRunProjection) => void
  onCloseInspector: () => void
}) {
  const [draftWorkflow, setDraftWorkflow] = React.useState<DraftWorkflowChrome | null>(
    null,
  )
  const versionSession = useDraftVersionSession(listVersionFiles)
  const [overviewBind, overview] = useDraftOverviewSurface()
  const draftOverviewId = "draft-overview-window"

  const group = projection.targetWorkspace.targetGroups.find(
    (candidate) => candidate.targetAgent.id === target.targetAgentId,
  )
  const draft = group?.drafts.find((candidate) => candidate.id === target.draftId)
  const binding = group?.workspaceBindings.find(
    (candidate) => candidate.id === target.workspaceBindingId,
  )
  const project = binding
    ? projection.projects.find((candidate) => candidate.id === binding.projectId)
    : undefined
  const versions = group?.versions ?? []
  const draftRuns = draft
    ? projection.factoryRuns.filter((run) => run.agentDraftId === draft.id)
    : []
  const liveRun = draftRuns.find(
    (run) => !["passed", "cancelled"].includes(run.state),
  )
  const selectedRun = liveRun
  const title = target.title ?? draft?.name ?? group?.targetAgent.name ?? "Draft"
  const versionSurface = group && versionSession.tabs.openIds.length > 0 ? (
    <VersionTabSurface
      agent={group.targetAgent}
      versions={versions}
      tabs={versionSession.tabs}
      filesById={versionSession.filesById}
      emitIntent={emitIntent}
      readVersionFile={readVersionFile}
      onOpenVersion={versionSession.openVersion}
      onActivateTab={versionSession.activateTab}
      onCloseTab={versionSession.closeTab}
      onCloseSurface={versionSession.closeSurface}
    />
  ) : null
  const draftOverview = group ? (
    <DraftOverview
      agent={group.targetAgent}
      draft={draft}
      versions={versions}
      environments={projection.environments}
      selectedRun={selectedRun}
      emitIntent={emitIntent}
      onOpenCodeChanges={(run) => {
        overview.setOpen(false)
        onOpenCodeChanges(run)
      }}
    />
  ) : null

  const closeWindow = () => {
    const bridge = window.zero as
      | { invoke?: (command: string, payload: unknown) => Promise<unknown> }
      | undefined
    const label = draftWindowLabel(target.draftId)
    if (bridge?.invoke) {
      void bridge.invoke("native-sdk.window.close", { label }).catch(() => {
        window.close()
      })
      return
    }
    window.close()
  }

  return (
    <div
      className="flex h-svh min-w-0 flex-col overflow-hidden bg-background"
      onPointerDownCapture={handleNativeWindowDragPointerDown}
    >
      <h1 className="sr-only">{title}</h1>
      <header
        data-native-drag-region
        className="flex h-11 shrink-0 items-center gap-1 overflow-hidden pl-32 pr-2"
      >
        <p className="min-w-0 truncate text-sm font-medium">{title}</p>
        <Badge variant="secondary" className="shrink-0">
          Draft
        </Badge>
        {draftWorkflow ? (
          <div data-native-no-drag className="shrink-0">
            <DropdownMenu>
              <DropdownMenuTrigger
                render={(
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label={`Actions for ${draftWorkflow.draftName}`}
                  />
                )}
              >
                <MoreHorizontalIcon />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start">
                <DropdownMenuGroup>
                  {draftWorkflow.canEdit ? (
                    <DropdownMenuItem onClick={draftWorkflow.onEdit}>
                      <PencilIcon />
                      Edit
                    </DropdownMenuItem>
                  ) : null}
                  {draftWorkflow.onCreateDraft ? (
                    <DropdownMenuItem onClick={draftWorkflow.onCreateDraft}>
                      <PlusIcon />
                      Create Draft
                    </DropdownMenuItem>
                  ) : (
                    <DropdownMenuItem
                      disabled={draftWorkflow.publishDisabled}
                      onClick={draftWorkflow.onCreateVersion}
                    >
                      <TagIcon />
                      Create Version
                    </DropdownMenuItem>
                  )}
                  {draftWorkflow.onDiscardDraft ? (
                    <DropdownMenuItem
                      variant="destructive"
                      disabled={draftWorkflow.discardDisabled}
                      onClick={draftWorkflow.onDiscardDraft}
                    >
                      <Trash2Icon />
                      Discard Draft
                    </DropdownMenuItem>
                  ) : null}
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        ) : null}
        <div data-native-no-drag className="ml-auto flex shrink-0 items-center gap-1">
          {draftOverview ? (
            <DraftOverviewTrigger id={draftOverviewId} overview={overview}>
              {draftOverview}
            </DraftOverviewTrigger>
          ) : null}
          {draft ? (
            <Tooltip>
              <TooltipTrigger render={
                <Button
                  variant={nativeTerminalVisible ? "secondary" : "ghost"}
                  size="icon-sm"
                  aria-label={nativeTerminalVisible ? "Close Terminal" : "Open Terminal"}
                  aria-pressed={nativeTerminalVisible}
                  onClick={() => void emitIntent({
                    type: "agentDraft.toggleWorkspace",
                    agentDraftId: draft.id,
                  })}
                />
              }>
                <TerminalIcon />
              </TooltipTrigger>
              <TooltipContent>
                {nativeTerminalVisible ? "Close Terminal" : "Open Terminal"}
              </TooltipContent>
            </Tooltip>
          ) : null}
          <Tooltip>
            <TooltipTrigger render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Close window"
                onClick={closeWindow}
              />
            }>
              <XIcon />
            </TooltipTrigger>
            <TooltipContent>Close window</TooltipContent>
          </Tooltip>
        </div>
      </header>
      <Separator />
      <div className="flex min-h-0 min-w-0 flex-1">
        <div
          ref={overviewBind}
          className="flex min-h-0 min-w-0 flex-1 overflow-hidden"
        >
          <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
            {group && draft && binding ? (
              <AgentDraftWorkspace
                agent={group.targetAgent}
                draft={draft}
                project={project}
                runs={draftRuns}
                sessions={projection.sessions}
                liveAgents={projection.liveAgents}
                herdr={projection.herdr}
                environments={projection.environments}
                runtimeError={projection.targetWorkspaceError ?? projection.runError}
                emitIntent={emitIntent}
                startDraftRun={startDraftRun}
                sidebarOpen={false}
                onDraftWorkflowChange={setDraftWorkflow}
                versionSurface={versionSurface}
                versions={versions}
              />
            ) : (
              <Empty className="size-full">
                <EmptyHeader>
                  <EmptyTitle>Draft unavailable</EmptyTitle>
                  <EmptyDescription>
                    This Draft is no longer available in the current projection.
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            )}
          </div>
          {draftOverview ? (
            <DraftOverviewPanel id={draftOverviewId} overview={overview}>
              {draftOverview}
            </DraftOverviewPanel>
          ) : null}
        </div>
        {inspector ? (
          <>
            <Separator orientation="vertical" />
            <div
              className="flex w-[min(42rem,46%)] shrink-0 flex-col overflow-hidden"
              id="workspace-inspector"
            >
              <CodeChangesInspector
                key={inspector.run.id}
                run={inspector.run}
                onClose={onCloseInspector}
              />
            </div>
          </>
        ) : null}
      </div>
    </div>
  )
}

function TargetSearch({ open, onOpenChange, projection, emitIntent }: { open: boolean; onOpenChange: (open: boolean) => void; projection: WorkspaceProjection; emitIntent: EmitIntent }) {
  return (
    <CommandDialog open={open} onOpenChange={onOpenChange} title="Search Agents" description="Open an Agent or active Draft.">
      <Command><CommandInput placeholder="Search Agents and Drafts…" /><CommandList><CommandEmpty>No matching Agent.</CommandEmpty>
        {projection.targetWorkspace.targetGroups.map((group) => {
          const recentDraft = group.drafts
            .filter((draft) => draft.lifecycle !== "archived")
            .toSorted((left, right) =>
              right.updatedAtUnixMs - left.updatedAtUnixMs)[0]
          const binding = recentDraft
            ? group.workspaceBindings.find((candidate) =>
                candidate.id === recentDraft.workspaceBindingId)
            : group.workspaceBindings
                .filter((candidate) => !candidate.archived)
                .toSorted((left, right) =>
                  right.lastUsedAtUnixMs - left.lastUsedAtUnixMs)[0]
          if (!binding) return null
          return (
            <CommandGroup
              key={group.targetAgent.id}
              heading={group.targetAgent.name}
            >
              <CommandItem
                value={group.targetAgent.name}
                onSelect={() => {
                  void emitIntent({
                    type: "workspacePane.openPrimary",
                    targetAgentId: group.targetAgent.id,
                    workspaceBindingId: binding.id,
                    workItemId: recentDraft?.id,
                    workItemKind: recentDraft ? "agent_draft" : undefined,
                  })
                  onOpenChange(false)
                }}
              >
                <BotIcon aria-hidden="true" />
                <span>{group.targetAgent.name}</span>
              </CommandItem>
              {group.drafts.filter((draft) => draft.lifecycle !== "archived").map((draft) => {
                const draftBinding = group.workspaceBindings.find(
                  (candidate) => candidate.id === draft.workspaceBindingId,
                )
                if (!draftBinding) return null
                return (
                <CommandItem
                  key={draft.id}
                  value={`${group.targetAgent.name} Draft ${draft.name}`}
                  onSelect={() => {
                    void emitIntent({
                      type: "workspacePane.openPrimary",
                      targetAgentId: group.targetAgent.id,
                      workspaceBindingId: draftBinding.id,
                      workItemId: draft.id,
                      workItemKind: "agent_draft",
                    })
                    onOpenChange(false)
                  }}
                >
                  <GitBranchIcon aria-hidden="true" />
                  <span>{draft.name}</span>
                </CommandItem>
              )})}
            </CommandGroup>
          )
        })}
      </CommandList></Command>
    </CommandDialog>
  )
}

function WindowActions({ onSearch }: { onSearch: () => void }) {
  const { toggleSidebar, open, isMobile } = useSidebar()
  const active = open && !isMobile
  return (
    <div data-native-no-drag className="absolute left-20 top-1 z-40 flex h-6 items-center gap-1">
      <Button variant={active ? "secondary" : "ghost"} size="icon-sm" onClick={toggleSidebar} aria-label={active ? "Hide sidebar" : "Show sidebar"}><PanelLeftIcon /></Button>
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={onSearch}
        aria-label="Search Agents"
      >
        <SearchIcon />
      </Button>
    </div>
  )
}

type PaneData = NonNullable<ReturnType<typeof paneData>>

function paneData(projection: WorkspaceProjection, context: WorkContextProjection) {
  const group = projection.targetWorkspace.targetGroups.find((candidate) => candidate.targetAgent.id === context.targetAgentId)
  const binding = group?.workspaceBindings.find((candidate) => candidate.id === context.workspaceBindingId)
  if (!group || !binding) return undefined
  const item = group.workItems.find(
    (candidate) => candidate.id === context.workItemId,
  )
  const draft = item?.kind === "agent_draft"
    ? group.drafts.find((candidate) => candidate.id === item.id)
    : item?.agentDraftId
      ? group.drafts.find((candidate) => candidate.id === item.agentDraftId)
      : undefined
  return {
    group,
    binding,
    project: projection.projects.find(
      (candidate) => candidate.id === binding.projectId,
    ),
    item,
    draft,
    title: draft?.name ?? item?.title ?? `${group.targetAgent.name} Draft`,
  }
}

function EmptyWorkspaceFrame({ sidebarOpen, children }: { sidebarOpen: boolean; children: React.ReactNode }) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        data-native-drag-region
        className={cn(
          "h-11 shrink-0 transition-[padding] duration-200 ease-linear",
          sidebarOpen ? "px-2" : "pl-32 pr-2",
        )}
      />
      <Separator />
      {children}
    </div>
  )
}

function RecentAgents({
  groups,
  onOpenDraft,
  onCreateDraft,
}: {
  groups: readonly TargetAgentWorkGroupProjection[]
  onOpenDraft: (
    targetAgentId: string,
    workspaceBindingId: string,
    draftId?: string,
  ) => void
  onCreateDraft: (
    agent: TargetAgentProjection,
    version?: TargetAgentVersionProjection,
  ) => void
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div
        data-native-drag-region
        className="flex h-11 shrink-0 items-center px-2"
      >
        <h2 className="text-sm font-medium text-muted-foreground">
          Recent Agents
        </h2>
      </div>
      <Separator />
      <div className="flex min-h-0 flex-1 items-start gap-4 overflow-x-auto p-4">
        {groups.map((group) => (
          <AgentCard
            key={group.targetAgent.id}
            group={group}
            onOpenDraft={onOpenDraft}
            onCreateDraft={onCreateDraft}
          />
        ))}
      </div>
    </div>
  )
}

function AgentCard({
  group,
  onOpenDraft,
  onCreateDraft,
}: {
  group: TargetAgentWorkGroupProjection
  onOpenDraft: (
    targetAgentId: string,
    workspaceBindingId: string,
    draftId?: string,
  ) => void
  onCreateDraft: (
    agent: TargetAgentProjection,
    version?: TargetAgentVersionProjection,
  ) => void
}) {
  const { targetAgent, drafts, versions, workspaceBindings } = group
  const activeDrafts = drafts.filter(
    (draft) => draft.lifecycle !== "archived",
  )
  const latestVersion = sortAgentVersions(versions)[0]
  const binding = workspaceBindings.find(
    (candidate) => !candidate.archived,
  ) ?? workspaceBindings[0]

  return (
    <div className="flex w-72 shrink-0 flex-col overflow-hidden rounded-lg border bg-background">
      <div className="flex items-center gap-2 border-b px-3 py-2.5">
        <BotIcon className="size-4 shrink-0 text-muted-foreground" />
        <p className="min-w-0 truncate text-sm font-medium">
          {targetAgent.name}
        </p>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-0.5 p-2">
        {activeDrafts.length > 0 ? (
          activeDrafts.map((draft) => {
            const draftBinding = workspaceBindings.find(
              (candidate) => candidate.id === draft.workspaceBindingId,
            )
            if (!draftBinding) return null
            const baseVersion = draft.baseVersion
              ? versions.find((v) => v.version === draft.baseVersion)
              : undefined
            return (
              <button
                key={draft.id}
                type="button"
                className="flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent/30 focus-visible:bg-accent/30 focus-visible:outline-none"
                onClick={() =>
                  onOpenDraft(
                    targetAgent.id,
                    draftBinding.id,
                    draft.id,
                  )
                }
              >
                <GitBranchIcon className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate">
                  {draftBinding.name}
                </span>
                {baseVersion ? (
                  <Badge variant="outline" className="shrink-0 text-xs">
                    v{baseVersion.version}
                  </Badge>
                ) : null}
              </button>
            )
          })
        ) : (
          <p className="px-2 py-1.5 text-sm text-muted-foreground">
            No drafts
          </p>
        )}
      </div>
      <div className="border-t p-2">
        <Button
          variant="ghost"
          size="sm"
          className="w-full justify-start"
          disabled={!binding}
          onClick={() => onCreateDraft(targetAgent, latestVersion)}
        >
          <PlusIcon data-icon="inline-start" />
          Create Draft
        </Button>
      </div>
    </div>
  )
}

function WorkspaceLoading({ detail }: { detail?: string }) {
  return (
    <main aria-label="Loading workspace" className="flex min-h-0 flex-1 flex-col gap-4 p-4">
      {/* While the runtime is still coming up, say so rather than showing a
          spinner that looks identical to a hang. */}
      {detail ? <p className="text-xs text-muted-foreground">{detail}</p> : null}
      <div className="flex min-h-0 flex-1 gap-4">
        <Skeleton className="h-full flex-1" />
        <Skeleton className="h-full flex-1" />
      </div>
    </main>
  )
}

export type { CreateTargetAgent, EmitIntent }
