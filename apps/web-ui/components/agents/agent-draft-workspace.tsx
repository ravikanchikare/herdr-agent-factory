"use client"

import * as React from "react"
import {
  ChevronRightIcon,
  CopyIcon,
  FileDiffIcon,
  FolderGit2Icon,
  GitBranchIcon,
  Globe2Icon,
  HistoryIcon,
  TagIcon,
  TriangleAlertIcon,
} from "lucide-react"

import type {
  AgentDraftProjection,
  EnvironmentDto,
  FactoryRunProjection,
  HerdrStatusDto,
  LiveAgentDto,
  ProjectProjection,
  RuntimeIntent,
  SessionProjection,
  TargetAgentProjection,
  TargetAgentVersionProjection,
} from "@agent-factory/runtime-client"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@agent-factory/ui/components/alert"
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
import {
  Conversation,
  ConversationContent,
  ConversationScrollButton,
} from "@agent-factory/ui/components/ai-elements/conversation"
import {
  Message,
  MessageContent,
  MessageLabel,
} from "@agent-factory/ui/components/ai-elements/message"
import {
  PromptInput,
  PromptInputBody,
  PromptInputFooter,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
  type PromptInputMessage,
} from "@agent-factory/ui/components/ai-elements/prompt-input"
import { Shimmer } from "@agent-factory/ui/components/ai-elements/shimmer"
import { Badge } from "@agent-factory/ui/components/badge"
import { Button } from "@agent-factory/ui/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@agent-factory/ui/components/collapsible"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@agent-factory/ui/components/dialog"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@agent-factory/ui/components/empty"
import {
  Field,
  FieldLabel,
} from "@agent-factory/ui/components/field"
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@agent-factory/ui/components/resizable"
import { ScrollArea } from "@agent-factory/ui/components/scroll-area"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@agent-factory/ui/components/select"
import { Separator } from "@agent-factory/ui/components/separator"
import { cn } from "@agent-factory/ui/lib/utils"

import { CreateDraftDialog } from "@/components/agents/create-draft-dialog"
import {
  sessionLifecycleLabel,
  sessionStatusLabel,
  type ReadAgentTranscript,
} from "@/components/agents/session-workspace"
import { sortAgentVersions } from "@/components/agents/agent-version-picker"
import {
  WorkCreationWorkspace,
  type AgentDefinitionFormValues,
} from "@/components/shell/work-creation-workspace"

type EmitIntent = (intent: RuntimeIntent) => Promise<void>

export type DraftWorkflowChrome = {
  draftName: string
  canEdit: boolean
  publishDisabled: boolean
  onEdit: () => void
  onCreateVersion: () => void
  onOpenRunHistory?: () => void
  onCreateDraft?: () => void
  discardDisabled?: boolean
  onDiscardDraft?: () => void
}

/** Display worktree as a sibling-relative path; copy still uses the full path. */
export function relativeSiblingWorktreePath(worktreePath: string): string {
  const name = worktreePath.split(/[/\\]/).filter(Boolean).at(-1)
  return name ? `../${name}` : worktreePath
}

export function AgentDraftWorkspace({
  agent,
  draft,
  project,
  runs,
  sessions,
  liveAgents = [],
  herdr,
  selectedRunId,
  environments,
  runtimeError,
  emitIntent,
  startDraftRun,
  sidebarOpen = true,
  onDraftWorkflowChange,
  onEditStateChange,
  versionSurface,
  versions = [],
  readAgentTranscript,
}: {
  agent: TargetAgentProjection
  draft?: AgentDraftProjection
  project?: ProjectProjection
  runs: readonly FactoryRunProjection[]
  sessions: readonly SessionProjection[]
  liveAgents?: readonly LiveAgentDto[]
  herdr?: HerdrStatusDto
  selectedRunId?: string
  environments: readonly EnvironmentDto[]
  runtimeError?: string
  emitIntent: EmitIntent
  startDraftRun: (
    runId: string,
    draftId: string,
    environmentId: string,
    objective: string,
  ) => Promise<void>
  sidebarOpen?: boolean
  onDraftWorkflowChange?: (chrome: DraftWorkflowChrome | null) => void
  onEditStateChange?: (editing: boolean) => void
  versionSurface?: React.ReactNode
  versions?: readonly TargetAgentVersionProjection[]
  readAgentTranscript?: ReadAgentTranscript
}) {
  const [isEditing, setIsEditing] = React.useState(false)
  const handleEditStateChange = React.useCallback(
    (editing: boolean) => {
      setIsEditing(editing)
      onEditStateChange?.(editing)
    },
    [onEditStateChange],
  )
  const primary = draft?.lifecycle === "active" ? (
    <DraftEditor
      key={draft.id}
      draft={draft}
      project={project}
      runs={runs}
      sessions={sessions}
      liveAgents={liveAgents}
      herdr={herdr}
      selectedRunId={selectedRunId}
      environments={environments}
      runtimeError={runtimeError}
      emitIntent={emitIntent}
      startDraftRun={startDraftRun}
      sidebarOpen={sidebarOpen}
      onDraftWorkflowChange={onDraftWorkflowChange}
      onEditStateChange={handleEditStateChange}
      readAgentTranscript={readAgentTranscript}
    />
  ) : (
    <EmptyDraftWorkspace
      agent={agent}
      draft={draft}
      versions={versions}
      runtimeError={runtimeError}
      emitIntent={emitIntent}
      onDraftWorkflowChange={onDraftWorkflowChange}
    />
  )
  const contextSurfaces = [
    versionSurface ? { key: "version", node: versionSurface } : undefined,
  ].flatMap((surface) => surface ? [surface] : [])
  const contextCount = contextSurfaces.length
  const draftDefaultSize = contextCount === 0
    ? "100%"
    : contextCount === 1
      ? "58%"
      : "40%"
  const contextDefaultSize = contextCount === 1
    ? "42%"
    : "30%"

  return (
    <section
      aria-label={`${agent.name} Draft`}
      className="@container/draft size-full min-h-0"
    >
      {isEditing ? (
        primary
      ) : contextCount > 0 ? (
        <ResizablePanelGroup
          orientation="horizontal"
          className="size-full min-h-0"
          resizeTargetMinimumSize={{ coarse: 28, fine: 16 }}
        >
          <ResizablePanel
            defaultSize={draftDefaultSize}
            minSize="20rem"
            className="min-h-0 min-w-0"
          >
            {primary}
          </ResizablePanel>
          {contextSurfaces.map((surface, index) => (
            <React.Fragment key={surface.key}>
              <ResizableHandle
                data-native-no-drag
                withHandle
                reveal="hover"
                aria-label={contextResizeLabel(
                  index === 0 ? "draft" : contextSurfaces[index - 1]?.key,
                )}
              />
              <ResizablePanel
                defaultSize={contextDefaultSize}
                minSize="16rem"
                className={cn(
                  "min-h-0 min-w-0 bg-muted/20 p-3",
                  index > 0 && "pl-1.5",
                  index < contextCount - 1 && "pr-1.5",
                )}
              >
                <DraftContextSurface>{surface.node}</DraftContextSurface>
              </ResizablePanel>
            </React.Fragment>
          ))}
        </ResizablePanelGroup>
      ) : primary}
    </section>
  )
}

function contextResizeLabel(previous: string | undefined) {
  if (previous === "draft") return "Resize Draft and context"
  return "Resize context panels"
}

function DraftContextSurface({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <div
      data-context-surface
      className="flex size-full min-h-0 min-w-0 overflow-hidden rounded-lg border bg-background shadow-md"
    >
      {children}
    </div>
  )
}

function EmptyDraftWorkspace({
  agent,
  draft,
  versions,
  runtimeError,
  emitIntent,
  onDraftWorkflowChange,
}: {
  agent: TargetAgentProjection
  draft?: AgentDraftProjection
  versions: readonly TargetAgentVersionProjection[]
  runtimeError?: string
  emitIntent: EmitIntent
  onDraftWorkflowChange?: (chrome: DraftWorkflowChrome | null) => void
}) {
  const [createOpen, setCreateOpen] = React.useState(false)
  const [createVersion, setCreateVersion] = React.useState<
    TargetAgentVersionProjection | undefined
  >()
  const latestVersion = sortAgentVersions(versions)[0]
  const openCreate = (version?: TargetAgentVersionProjection) => {
    setCreateVersion(version)
    setCreateOpen(true)
  }

  // Synchronize the parent pane title-bar actions and clear them on unmount.
  React.useEffect(() => {
    if (!onDraftWorkflowChange) return
    onDraftWorkflowChange({
      draftName: agent.name,
      canEdit: false,
      publishDisabled: true,
      onEdit: () => undefined,
      onCreateVersion: () => undefined,
      onCreateDraft: () => openCreate(latestVersion),
    })
    return () => onDraftWorkflowChange(null)
  }, [agent.name, latestVersion, onDraftWorkflowChange])

  const main = (
    <div className="size-full min-w-0 flex-1">
      {runtimeError ? (
        <div className="p-6">
          <Alert variant="destructive">
            <TriangleAlertIcon />
            <AlertTitle>Draft action failed</AlertTitle>
            <AlertDescription>{runtimeError}</AlertDescription>
          </Alert>
        </div>
      ) : draft ? (
        <div className="p-6">
          <Alert>
            <TriangleAlertIcon />
            <AlertTitle>
              {draft.lifecycle === "publishing"
                ? "Creating Version"
                : "Draft cleanup required"}
            </AlertTitle>
            <AlertDescription>
              {draft.cleanupGuidance ??
                "The Version is valid, but Agent Factory could not safely remove the managed worktree and branch. Resolve the Git state; cleanup will be retried after restart."}
            </AlertDescription>
          </Alert>
        </div>
      ) : null}
    </div>
  )
  return (
    <>
      {main}
      <CreateDraftDialog
        key={createVersion?.id ?? "head"}
        agent={agent}
        version={createVersion}
        emitIntent={emitIntent}
        open={createOpen}
        onOpenChange={setCreateOpen}
        showTrigger={false}
      />
    </>
  )
}

export function DraftOverview({
  agent,
  draft,
  versions,
  environments,
  selectedRun,
  emitIntent,
  onOpenCodeChanges,
}: {
  agent: TargetAgentProjection
  draft?: AgentDraftProjection
  versions: readonly TargetAgentVersionProjection[]
  environments: readonly EnvironmentDto[]
  selectedRun?: FactoryRunProjection
  emitIntent: EmitIntent
  onOpenCodeChanges: (run: FactoryRunProjection) => void
}) {
  const [createOpen, setCreateOpen] = React.useState(false)
  const [createVersion, setCreateVersion] = React.useState<
    TargetAgentVersionProjection | undefined
  >()
  const selectedEnvironment = environments.find(
    (environment) => environment.readiness.state === "ready",
  )?.id
  const openCreate = (version?: TargetAgentVersionProjection) => {
    setCreateVersion(version)
    setCreateOpen(true)
  }

  if (draft?.lifecycle === "active") {
    return (
      <DraftDetailsPanel
        draft={draft}
        environments={environments}
        selectedEnvironment={selectedEnvironment}
        selectedRun={selectedRun}
        onOpenCodeChanges={onOpenCodeChanges}
      />
    )
  }

  return (
    <>
      <EmptyDraftOverview
        agent={agent}
        versions={versions}
        onCreateDraft={openCreate}
      />
      <CreateDraftDialog
        key={createVersion?.id ?? "head"}
        agent={agent}
        version={createVersion}
        emitIntent={emitIntent}
        open={createOpen}
        onOpenChange={setCreateOpen}
        showTrigger={false}
      />
    </>
  )
}

function EmptyDraftOverview({
  agent,
  versions,
  onCreateDraft,
}: {
  agent: TargetAgentProjection
  versions: readonly TargetAgentVersionProjection[]
  onCreateDraft: (version?: TargetAgentVersionProjection) => void
}) {
  const sorted = sortAgentVersions(versions)
  return (
    <div
      data-empty="true"
      className="flex flex-col gap-2"
    >
      <div className="flex min-w-0 items-center gap-2 px-1">
        <h2 className="min-w-0 truncate text-sm font-semibold tracking-tight">
          {agent.name}
        </h2>
      </div>
      <Separator />
      {sorted.length === 0 ? (
        <Empty className="py-6">
          <EmptyHeader>
            <EmptyMedia variant="icon"><TagIcon /></EmptyMedia>
            <EmptyTitle>No versions yet</EmptyTitle>
            <EmptyDescription>
              Create the first Draft to start editing {agent.name}.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Button
              size="sm"
              onClick={() => onCreateDraft()}
            >
              Create Draft
            </Button>
          </EmptyContent>
        </Empty>
      ) : (
        <div className="flex flex-col gap-1">
          <p className="px-1 text-xs text-muted-foreground">
            Create a Draft from an immutable Version.
          </p>
          <ul aria-label="Versions">
            {sorted.map((version) => (
              <li key={version.id} className="group/version">
                <div className="flex items-center gap-1 rounded-md px-2 py-1.5 hover:bg-accent/30">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">
                      v{version.version}
                    </p>
                    <p className="truncate font-mono text-xs text-muted-foreground">
                      {version.gitCommit.slice(0, 12)}
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="opacity-0 group-hover/version:opacity-100 group-focus-within/version:opacity-100"
                    onClick={() => onCreateDraft(version)}
                  >
                    Create Draft
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}

function DraftEditor({
  draft,
  project,
  runs,
  sessions,
  liveAgents,
  herdr,
  selectedRunId,
  environments,
  runtimeError,
  emitIntent,
  startDraftRun,
  sidebarOpen,
  onDraftWorkflowChange,
  onEditStateChange,
  readAgentTranscript,
}: {
  draft: AgentDraftProjection
  project?: ProjectProjection
  runs: readonly FactoryRunProjection[]
  sessions: readonly SessionProjection[]
  liveAgents: readonly LiveAgentDto[]
  herdr?: HerdrStatusDto
  selectedRunId?: string
  environments: readonly EnvironmentDto[]
  runtimeError?: string
  emitIntent: EmitIntent
  startDraftRun: (
    runId: string,
    draftId: string,
    environmentId: string,
    objective: string,
  ) => Promise<void>
  sidebarOpen: boolean
  onDraftWorkflowChange?: (chrome: DraftWorkflowChrome | null) => void
  onEditStateChange?: (editing: boolean) => void
  readAgentTranscript?: ReadAgentTranscript
}) {
  const [editOpen, setEditOpen] = React.useState(false)
  const [dialog, setDialog] = React.useState<"publish" | "discard">()
  const readyEnvironments = environments.filter(
    (environment) => environment.readiness.state === "ready",
  )
  // The Draft's own choice, when it still names a ready Environment. A choice
  // that has since been deleted or stopped being ready falls back rather than
  // leaving the Draft unable to start.
  const selectedEnvironment = readyEnvironments.some(
    (environment) => environment.id === draft.environmentId,
  ) ? draft.environmentId ?? undefined : readyEnvironments[0]?.id
  const liveRun = runs.find((run) => !runIsTerminal(run))
  const requestedRun = selectedRunId
    ? runs.find((run) => run.id === selectedRunId)
    : undefined
  const selectedRun = requestedRun && !runIsTerminal(requestedRun)
    ? requestedRun
    : liveRun
  const hasPassingRun = runs.some((run) =>
    run.state === "passed" &&
    run.startingGitHead === draft.gitHead &&
    run.objective === draft.objective &&
    run.acceptanceCriteria.length === draft.acceptanceCriteria.length &&
    run.acceptanceCriteria.every((criterion, index) =>
      criterion === draft.acceptanceCriteria[index]))
  // First Run locks Draft configuration for run consistency.
  const canEdit = runs.length === 0

  // Publish draft workflow actions into the pane title bar.
  React.useEffect(() => {
    if (!onDraftWorkflowChange) return
    onDraftWorkflowChange({
      draftName: draft.name,
      canEdit,
      publishDisabled: Boolean(liveRun),
      onEdit: () => setEditOpen(true),
      onCreateVersion: () => setDialog("publish"),
      discardDisabled: Boolean(liveRun),
      onDiscardDraft: () => setDialog("discard"),
      onOpenRunHistory: () => {
        document.getElementById(`run-history-${draft.id}`)?.scrollIntoView({
          behavior: "smooth",
          block: "start",
        })
      },
    })
    return () => onDraftWorkflowChange(null)
  }, [
    canEdit,
    draft.id,
    draft.name,
    liveRun,
    onDraftWorkflowChange,
  ])

  React.useEffect(() => {
    onEditStateChange?.(editOpen && canEdit)
  }, [editOpen, canEdit, onEditStateChange])

  if (editOpen && canEdit) {
    return (
      <WorkCreationWorkspace
        sidebarOpen={sidebarOpen}
        runtimeError={runtimeError}
        onClose={() => setEditOpen(false)}
        edit={{
          environments,
          initial: {
            name: draft.name,
            objective: draft.objective,
            draftName: draft.name,
            criteria: [...draft.acceptanceCriteria],
            root: draft.worktreePath,
            trusted: project?.trusted ?? false,
            environmentId: selectedEnvironment,
          },
          onSave: async (values: AgentDefinitionFormValues) => {
            await emitIntent({
              type: "agentDraft.update",
              agentDraftId: draft.id,
              name: values.name,
              objective: values.objective,
              acceptanceCriteria: values.criteria,
              trusted: values.trusted,
            })
            if (values.environmentId !== draft.environmentId) {
              await emitIntent({
                type: "agentDraft.environment.set",
                agentDraftId: draft.id,
                environmentId: values.environmentId,
              })
            }
            if (project && values.trusted !== project.trusted) {
              await emitIntent({
                type: "project.trust.set",
                projectId: project.id,
                trusted: values.trusted,
              })
            }
            return true
          },
        }}
      />
    )
  }

  const main = (
    <ScrollArea className="size-full min-w-0 flex-1">
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-6 p-6 @5xl:p-8">
        {runtimeError ? (
          <Alert variant="destructive">
            <TriangleAlertIcon />
            <AlertTitle>Draft action failed</AlertTitle>
            <AlertDescription>{runtimeError}</AlertDescription>
          </Alert>
        ) : null}
        <RunWorkspace
          draft={draft}
          project={project}
          run={selectedRun}
          runs={runs}
          sessions={sessions}
          liveAgents={liveAgents}
          herdr={herdr}
          selectedEnvironment={selectedEnvironment}
          readyEnvironments={readyEnvironments}
          dirty={false}
          emitIntent={emitIntent}
          startDraftRun={startDraftRun}
        />
        <Separator />
        <SessionHistory
          draft={draft}
          runs={runs}
          sessions={sessions}
          readTranscript={readAgentTranscript}
        />
      </div>
    </ScrollArea>
  )

  return (
    <>
      {main}
      <PublishDraftDialog
        draft={draft}
        hasPassingRun={hasPassingRun}
        blocked={Boolean(liveRun)}
        open={dialog === "publish"}
        onOpenChange={(open) => setDialog(open ? "publish" : undefined)}
        showTrigger={false}
        emitIntent={emitIntent}
      />
      <AlertDialog
        open={dialog === "discard"}
        onOpenChange={(open) => setDialog(open ? "discard" : undefined)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Discard this Draft?</AlertDialogTitle>
            <AlertDialogDescription>
              Agent Factory will remove the managed worktree only when Git
              confirms that no authored changes would be lost.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => void emitIntent({
                type: "agentDraft.discard",
                agentDraftId: draft.id,
              })}
            >
              Discard Draft
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function DraftDetailsPanel({
  draft,
  environments,
  selectedEnvironment,
  selectedRun,
  onOpenCodeChanges,
}: {
  draft: AgentDraftProjection
  environments: readonly EnvironmentDto[]
  selectedEnvironment?: string
  selectedRun?: FactoryRunProjection
  onOpenCodeChanges: (run: FactoryRunProjection) => void
}) {
  const frozenEnvironment = selectedRun
    ? environments.find(
        (environment) => environment.id === selectedRun.environmentId,
      )
    : undefined
  const environmentName = selectedRun
    ? (frozenEnvironment?.name ?? selectedRun.environmentId)
    : (environments.find(
        (environment) => environment.id === selectedEnvironment,
      )?.name ?? "No ready Environment")
  const relativeWorktree = relativeSiblingWorktreePath(draft.worktreePath)

  return (
    <div className="flex flex-col gap-4">
      <div className="flex min-w-0 items-center gap-2 px-1">
        <h2 className="min-w-0 truncate text-sm font-semibold tracking-tight">
          {draft.name}
        </h2>
        {draft.baseVersion ? (
          <Badge variant="outline" className="shrink-0">
            Derived from v{draft.baseVersion}
          </Badge>
        ) : null}
      </div>
      <Separator />
      <div className="flex flex-col gap-4">
        <section className="flex flex-col gap-2">
          <h3 className="px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Environment
          </h3>
          <div className="flex min-w-0 items-center gap-2 rounded-md border bg-card px-3 py-2.5">
            <Globe2Icon
              aria-hidden="true"
              className="size-4 shrink-0 text-muted-foreground"
            />
            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
              <p className="truncate text-sm font-medium">{environmentName}</p>
              {selectedRun ? (
                <p className="text-xs text-muted-foreground">
                  Frozen for this Run
                </p>
              ) : null}
            </div>
          </div>
        </section>
        <CodeChangeTotals
          run={selectedRun}
          onOpenCodeChanges={onOpenCodeChanges}
          compact
        />
        <div className="flex flex-col gap-2">
          <DraftDisclosure title="Objective">
            <p className="whitespace-pre-wrap text-sm leading-relaxed">
              {draft.objective}
            </p>
          </DraftDisclosure>
          <DraftDisclosure title="Success criteria">
            <ul className="flex list-disc flex-col gap-1.5 pl-5 text-sm leading-relaxed">
              {draft.acceptanceCriteria.map((criterion, index) => (
                <li key={`${index}-${criterion}`}>{criterion}</li>
              ))}
            </ul>
          </DraftDisclosure>
        </div>
        <section className="flex flex-col gap-2">
          <h3 className="px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            Git
          </h3>
          <div className="flex flex-col rounded-md border divide-y">
            <CopyableIconValue
              icon={<GitBranchIcon />}
              label="Git branch"
              display={draft.branchRef}
              copyValue={draft.branchRef}
            />
            <CopyableIconValue
              icon={<FolderGit2Icon />}
              label="Worktree path"
              display={relativeWorktree}
              copyValue={draft.worktreePath}
            />
          </div>
        </section>
      </div>
    </div>
  )
}

function DraftDisclosure({
  title,
  children,
}: {
  title: string
  children: React.ReactNode
}) {
  return (
    <Collapsible className="group/disclosure rounded-md transition-colors hover:bg-accent/30">
      <CollapsibleTrigger
        render={(
          <button
            type="button"
            className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
        )}
      >
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {title}
        </span>
        <span className="text-muted-foreground transition-transform in-data-panel-open:rotate-90 [&_svg]:size-4">
          <ChevronRightIcon aria-hidden="true" />
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="px-2 pb-2">
        {children}
      </CollapsibleContent>
    </Collapsible>
  )
}

function CopyableIconValue({
  icon,
  label,
  display,
  copyValue,
}: {
  icon: React.ReactNode
  label: string
  display: string
  copyValue: string
}) {
  return (
    <div className="flex items-center gap-2 rounded-md px-2 py-1.5">
      <span className="text-muted-foreground [&_svg]:size-4">{icon}</span>
      <p className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground">
        <span className="sr-only">{label}: </span>
        {display}
      </p>
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        aria-label={`Copy ${label}`}
        onClick={() => void navigator.clipboard?.writeText(copyValue)}
      >
        <CopyIcon />
      </Button>
    </div>
  )
}

function RunWorkspace({
  draft,
  project,
  run,
  runs,
  sessions,
  liveAgents,
  herdr,
  selectedEnvironment,
  readyEnvironments,
  dirty,
  emitIntent,
  startDraftRun,
}: {
  draft: AgentDraftProjection
  project?: ProjectProjection
  run?: FactoryRunProjection
  runs: readonly FactoryRunProjection[]
  sessions: readonly SessionProjection[]
  liveAgents: readonly LiveAgentDto[]
  herdr?: HerdrStatusDto
  selectedEnvironment?: string
  readyEnvironments: readonly EnvironmentDto[]
  dirty: boolean
  emitIntent: EmitIntent
  startDraftRun: (
    runId: string,
    draftId: string,
    environmentId: string,
    objective: string,
  ) => Promise<void>
}) {
  const managedRunGroups = runs
    .map((candidate) => ({
      run: candidate,
      sessions: managedSessionTree(sessions.filter((session) =>
        session.factoryRunId === candidate.id &&
        session.availability !== "historical")),
    }))
    .filter((group) => group.sessions.length > 0)
    .toSorted((left, right) => {
      if (left.run.id === run?.id) return -1
      if (right.run.id === run?.id) return 1
      return left.run.id.localeCompare(right.run.id)
    })
  const selectedRunHasLiveSessions = managedRunGroups.some((group) =>
    group.run.id === run?.id)
  const directManagedSessions = managedSessionTree(sessions.filter((session) =>
    session.workspaceBindingId === draft.workspaceBindingId &&
    !session.factoryRunId &&
    session.availability !== "historical"))
  const otherRuntimeAgents = liveAgents.filter((agent) =>
    agent.workspaceBindingId === draft.workspaceBindingId &&
    !agent.managedSessionId)

  if (!run) {
    return (
      <NewRunComposer
        draft={draft}
        project={project}
        selectedEnvironment={selectedEnvironment}
        readyEnvironments={readyEnvironments}
        dirty={dirty}
        emitIntent={emitIntent}
        startDraftRun={startDraftRun}
      />
    )
  }

  return (
    <section aria-labelledby="herdr-agents-title" className="flex flex-col gap-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
            <h2 id="herdr-agents-title" className="text-sm font-medium">
              Herdr agents
            </h2>
            {run ? <Badge variant="outline">{runStateLabel(run.state)}</Badge> : null}
            {herdr ? (
              <Badge variant="outline">
                {authorityFreshnessLabel(herdr.freshness)}
              </Badge>
            ) : null}
          </div>
          <p className="text-xs text-muted-foreground">
            The Orchestrator runs Coding and Evaluation on its own, and asks
            only when it cannot decide.
          </p>
        </div>
        <RunLifecycleActions
          run={run}
          emitIntent={emitIntent}
        />
      </div>
      <RunConversation run={run} sessions={sessions} />
      {run?.escalation ? (
        <div
          role="status"
          className="flex flex-col gap-1 rounded-md border border-warning/40 bg-warning/10 p-3"
        >
          <p className="text-xs font-medium text-foreground">
            The Orchestrator needs your decision
          </p>
          <p className="text-xs text-muted-foreground">{run.escalation}</p>
          <p className="text-xs text-muted-foreground">
            Answer it in the Orchestrator terminal; it continues from there.
          </p>
        </div>
      ) : null}
      <div className="flex flex-col gap-4">
        {run && !selectedRunHasLiveSessions ? (
          <OrchestratorRow
            run={run}
          />
        ) : null}
        {managedRunGroups.map((group) => (
          <ManagedRunGroup
            key={group.run.id}
            run={group.run}
            selected={group.run.id === run?.id}
            sessions={group.sessions}
          />
        ))}
        {directManagedSessions.length > 0 ? (
          <section
            aria-labelledby="direct-managed-sessions-title"
            className="flex flex-col gap-3"
          >
            <h3 id="direct-managed-sessions-title" className="text-xs font-medium">
              Direct managed sessions
            </h3>
            {directManagedSessions.map(({ session, depth }) => (
              <div key={session.id} className={cn(depth > 0 && "pl-6")}>
                <SessionRow
                  name={managedSessionLabel(session)}
                  purpose={session.purpose}
                  session={session}
                />
              </div>
            ))}
          </section>
        ) : null}
        {otherRuntimeAgents.length > 0 ? (
          <section
            aria-labelledby="other-runtime-activity-title"
            className="flex flex-col gap-3 pt-2"
          >
            <div>
              <h3
                id="other-runtime-activity-title"
                className="text-xs font-medium"
              >
                Other runtime activity
              </h3>
              <p className="text-xs text-muted-foreground">
                Herdr agents observed in this Workspace that are not associated
                with any Factory-managed session.
              </p>
            </div>
            {otherRuntimeAgents.map((agent) => (
              <RuntimeAgentRow key={agent.placement.paneId} agent={agent} />
            ))}
          </section>
        ) : null}
      </div>
    </section>
  )
}

function ManagedRunGroup({
  run,
  selected,
  sessions,
}: {
  run: FactoryRunProjection
  selected: boolean
  sessions: ReturnType<typeof managedSessionTree>
}) {
  return (
    <section
      aria-label={`${selected ? "Selected Run" : "Run"} managed agents`}
      className="flex flex-col gap-3"
    >
      <div className="flex items-center gap-2">
        <h3 className="text-xs font-medium">
          {selected ? "Selected Run" : runLabel(run)}
        </h3>
        <Badge variant="outline">{runStateLabel(run.state)}</Badge>
      </div>
      {sessions.map(({ session, depth }) => (
        <div key={session.id} className={cn(depth > 0 && "pl-6")}>
          <SessionRow
            name={managedSessionLabel(session)}
            purpose={session.purpose}
            session={session}
            run={run}
          />
        </div>
      ))}
    </section>
  )
}

function NewRunComposer({
  draft,
  project,
  selectedEnvironment,
  readyEnvironments,
  dirty,
  emitIntent,
  startDraftRun,
}: {
  draft: AgentDraftProjection
  project?: ProjectProjection
  selectedEnvironment?: string
  readyEnvironments: readonly EnvironmentDto[]
  dirty: boolean
  emitIntent: EmitIntent
  startDraftRun: (
    runId: string,
    draftId: string,
    environmentId: string,
    objective: string,
  ) => Promise<void>
}) {
  const [objective, setObjective] = React.useState("")
  const [startingRunId, setStartingRunId] = React.useState<string>()
  const starting = Boolean(startingRunId)
  const canStart = Boolean(
    objective.trim() &&
    selectedEnvironment &&
    project?.trusted &&
    !dirty &&
    !starting,
  )

  const startRun = ({ text }: PromptInputMessage) => {
    const nextObjective = text.trim()
    if (!canStart || !selectedEnvironment || !nextObjective) return
    const runId = crypto.randomUUID()
    setStartingRunId(runId)
    void startDraftRun(
      runId,
      draft.id,
      selectedEnvironment,
      nextObjective,
    ).finally(() => setStartingRunId((current) =>
      current === runId ? undefined : current))
  }

  return (
    <section
      aria-labelledby="new-run-title"
      className="mx-auto flex w-full max-w-3xl flex-col gap-6 py-8"
    >
      <div className="flex flex-col gap-2 text-center">
        <h2 id="new-run-title" className="text-xl font-semibold tracking-tight">
          What would you like to accomplish?
        </h2>
        <p className="text-sm text-muted-foreground">
          Describe the outcome. The Orchestrator will coordinate the work.
        </p>
      </div>
      <PromptInput onSubmit={startRun}>
        <PromptInputBody className="h-auto">
          <PromptInputTextarea
            value={objective}
            autoFocus
            placeholder="Fix the flaky authentication tests and verify token refresh."
            aria-label="Run objective"
            onChange={(event) => setObjective(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter" || event.shiftKey) return
              event.preventDefault()
              event.currentTarget.form?.requestSubmit()
            }}
          />
          <PromptInputFooter>
            <PromptInputTools>
              <Badge variant="outline">{project?.name ?? "No project"}</Badge>
              {readyEnvironments.length > 1 ? (
                <Select
                  value={selectedEnvironment ?? ""}
                  onValueChange={(environmentId) => {
                    if (!environmentId) return
                    void emitIntent({
                      type: "agentDraft.environment.set",
                      agentDraftId: draft.id,
                      environmentId,
                    })
                  }}
                >
                  <SelectTrigger
                    size="sm"
                    aria-label="Environment for this Run"
                  >
                    <SelectValue>
                      {(value: string | null) =>
                        readyEnvironments.find(
                          (environment) => environment.id === value,
                        )?.name ?? value}
                    </SelectValue>
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {readyEnvironments.map((environment) => (
                        <SelectItem key={environment.id} value={environment.id}>
                          {environment.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              ) : (
                <Badge variant="outline">
                  {readyEnvironments[0]?.name ?? "No ready Environment"}
                </Badge>
              )}
            </PromptInputTools>
            <div className="flex items-center gap-2">
              {startingRunId ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => void emitIntent({
                    type: "run.cancel",
                    runId: startingRunId,
                  })}
                >
                  Cancel Run
                </Button>
              ) : null}
              <PromptInputSubmit pending={starting} disabled={!canStart} />
            </div>
          </PromptInputFooter>
        </PromptInputBody>
      </PromptInput>
      {!project?.trusted ? (
        <p className="text-center text-xs text-muted-foreground">
          Trust this Project before starting a Run.
        </p>
      ) : null}
    </section>
  )
}

function RunConversation({
  run,
  sessions,
}: {
  run: FactoryRunProjection
  sessions: readonly SessionProjection[]
}) {
  const runSessions = sessions.filter((session) =>
    session.factoryRunId === run.id)
  const codingCount = runSessions.filter((session) =>
    session.purpose === "coding").length
  const evaluationCount = runSessions.filter((session) =>
    session.purpose === "evaluation").length
  const activity = [
    "Worktree prepared",
    runSessions.some((session) => session.purpose === "orchestration")
      ? "Orchestrator started"
      : "Starting Orchestrator",
    codingCount > 0
      ? `${codingCount} Coding ${codingCount === 1 ? "agent" : "agents"} started`
      : undefined,
    evaluationCount > 0
      ? "Evaluation started"
      : undefined,
  ].filter((item): item is string => Boolean(item))

  return (
    <div className="flex min-h-64 flex-col rounded-lg border bg-card">
      <Conversation>
        <ConversationContent className="mx-auto w-full max-w-3xl">
          <Message from="user">
            <MessageLabel>You</MessageLabel>
            <MessageContent>{run.objective}</MessageContent>
          </Message>
          <Message from="system">
            <MessageLabel>Agent Factory</MessageLabel>
            <MessageContent>
              {activity.map((item) => `✓ ${item}`).join("\n")}
            </MessageContent>
          </Message>
          {run.escalation ? (
            <Message from="assistant">
              <MessageLabel>Orchestrator</MessageLabel>
              <MessageContent>{run.escalation}</MessageContent>
            </Message>
          ) : null}
        </ConversationContent>
        <ConversationScrollButton />
      </Conversation>
    </div>
  )
}

function RunLifecycleActions({
  run,
  emitIntent,
}: {
  run: FactoryRunProjection
  emitIntent: EmitIntent
}) {
  if (runIsTerminal(run)) return null

  return (
    <div className="flex items-center gap-2">
      <RunIntentButton label="Cancel Run" run={run} emitIntent={emitIntent} />
    </div>
  )
}

function RunIntentButton({
  label,
  run,
  emitIntent,
}: {
  label: "Cancel Run"
  run: FactoryRunProjection
  emitIntent: EmitIntent
}) {
  return (
    <Button
      size="sm"
      variant="outline"
      onClick={() => void emitIntent({ type: "run.cancel", runId: run.id })}
    >
      {label}
    </Button>
  )
}

function SessionRow({ name, purpose, session, run }: {
  name: string
  purpose: "orchestration" | "coding" | "evaluation"
  session?: SessionProjection
  run?: FactoryRunProjection
}) {
  const active = session?.availability === "live" &&
    session.lifecycle === "working"
  const status = session ? sessionStatusLabel(session) : "Waiting"
  const description = session
    ? session.attention[0] ?? session.outcome?.summary ??
      liveAgentDescription(session.lifecycle)
    : waitingDescription(purpose, run)
  const blocked = session?.availability === "live" &&
    session.lifecycle === "blocked"
  const failed = session?.outcome?.kind === "failed"

  return (
    <div className="flex flex-col gap-2 border-b pb-6 last:border-b-0 last:pb-0">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm font-medium">{name}</p>
        <div className="flex items-center gap-2">
          <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
            <span
              aria-hidden="true"
              className={cn(
                "size-2 rounded-full bg-muted-foreground",
                active && "bg-primary",
                blocked && "bg-warning",
                failed && "bg-destructive",
              )}
            />
            {status}
          </span>
        </div>
      </div>
      <p className="text-xs text-muted-foreground">
        {active ? <Shimmer>{description}</Shimmer> : description}
      </p>
    </div>
  )
}

function RuntimeAgentRow({ agent }: { agent: LiveAgentDto }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2">
      <div className="min-w-0">
        <p className="truncate text-sm font-medium">
          {agent.agentName ?? agent.displayAgent ?? agent.agentKind ?? "Herdr agent"}
        </p>
        <p className="truncate text-xs text-muted-foreground">
          {agent.displayAgent ?? agent.agentKind ?? agent.placement.paneId}
        </p>
      </div>
      <span className="inline-flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
        <span
          aria-hidden="true"
          className={cn(
            "size-2 rounded-full bg-muted-foreground",
            agent.lifecycle === "working" && "bg-primary",
            agent.lifecycle === "blocked" && "bg-warning",
          )}
        />
        {sessionLifecycleLabel(agent.lifecycle)}
      </span>
    </div>
  )
}

function CodeChangeTotals({
  run,
  onOpenCodeChanges,
  compact = false,
}: {
  run?: FactoryRunProjection
  onOpenCodeChanges: (run: FactoryRunProjection) => void
  compact?: boolean
}) {
  const totals = run?.changedFiles.reduce(
    (result, file) => {
      for (const hunk of file.diff?.hunks ?? []) {
        for (const line of hunk.lines) {
          if (line.kind === "insert") result.additions += 1
          if (line.kind === "delete") result.deletions += 1
        }
      }
      return result
    },
    { additions: 0, deletions: 0 },
  ) ?? { additions: 0, deletions: 0 }
  const fileCount = run?.changedFiles.length ?? 0
  return (
    <Button
      type="button"
      variant="ghost"
      className={cn(
        "h-auto w-full justify-start gap-2 px-2 py-1.5",
        compact && "min-w-0",
      )}
      disabled={!run}
      aria-label="Inspect Code changes"
      onClick={() => run && onOpenCodeChanges(run)}
    >
      <FileDiffIcon data-icon="inline-start" />
      <span className="min-w-0 flex-1 truncate text-left text-sm font-medium">
        Code changes
      </span>
      <span className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
        <span>{fileCount} files</span>
        <span aria-label={`${totals.additions} additions`}>
          +{totals.additions}
        </span>
        <span aria-label={`${totals.deletions} deletions`}>
          −{totals.deletions}
        </span>
      </span>
      <ChevronRightIcon data-icon="inline-end" />
    </Button>
  )
}

function OrchestratorRow({
  run,
  session,
}: {
  run?: FactoryRunProjection
  session?: SessionProjection
}) {
  if (session) {
    return (
      <SessionRow
        name="Orchestrator"
        purpose="orchestration"
        session={session}
        run={run}
      />
    )
  }
  const failed = run?.state === "failed" || run?.state === "cancelled"
  return (
    <div className="flex flex-col gap-2 border-b pb-6 last:border-b-0 last:pb-0">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm font-medium">Orchestrator</p>
        <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
          <span
            aria-hidden="true"
            className={cn(
              "size-2 rounded-full bg-muted-foreground",
              failed && "bg-destructive",
              run?.state === "passed" && "bg-primary",
            )}
          />
          {run ? runStateLabel(run.state) : "Waiting"}
        </span>
      </div>
      <p className="text-xs text-muted-foreground">
        {orchestratorDescription(run)}
      </p>
    </div>
  )
}

function orchestratorDescription(run?: FactoryRunProjection) {
  if (!run) return "Start a Run to begin orchestration."
  switch (run.state) {
    case "draft":
      return "The Orchestrator is ready to start coding."
    case "orchestrating":
      return "The Orchestrator is coordinating this run."
    case "coding":
      return "The Orchestrator started coding and is waiting for that turn to finish."
    case "evaluating":
      return "The Orchestrator started evaluation and is waiting for a verdict."
    case "escalated":
      return "The Orchestrator is waiting for a decision in its own session."
    case "passed":
      return "Acceptance criteria are satisfied. The run is complete."
    case "failed":
      return "The Run failed. Its managed sessions remain available as history."
    case "needs_review":
      return "The Orchestrator finished with a result that needs review."
    case "cancelled":
      return "The run was cancelled."
  }
}

function liveAgentDescription(lifecycle: SessionProjection["lifecycle"]) {
  switch (lifecycle) {
    case "working":
      return "The agent is working in Herdr. Open the Run workspace to view it."
    case "idle":
      return "The agent is ready for input."
    case "blocked":
      return "The agent is waiting in its own interface."
    case "done":
      return "The agent returned to a ready state with unseen output."
    case "unknown":
      return "Herdr cannot classify this agent confidently."
    case undefined:
      return "This managed session is historical."
  }
}

function waitingDescription(
  purpose: "orchestration" | "coding" | "evaluation",
  run?: FactoryRunProjection,
) {
  if (!run) return "Start a Run to create this session."
  if (purpose === "orchestration") return "The Orchestrator has not started."
  if (purpose === "coding") return "Coding has not started."
  if (["draft", "coding"].includes(run.state)) {
    return "Waiting for the Coding Agent."
  }
  return "No Evaluation session is available for this Run."
}

function runStateLabel(state: FactoryRunProjection["state"]) {
  return state.split("_").map((part) =>
    part.charAt(0).toUpperCase() + part.slice(1)).join(" ")
}

function runLabel(run: FactoryRunProjection) {
  const objective = run.objective.trim().replace(/\s+/g, " ")
  return `Run · ${objective.length > 48 ? `${objective.slice(0, 48)}…` : objective}`
}

function runIsTerminal(run: FactoryRunProjection) {
  return ["passed", "failed", "needs_review", "cancelled"]
    .includes(run.state)
}

function authorityFreshnessLabel(freshness: HerdrStatusDto["freshness"]) {
  switch (freshness) {
    case "live": return "Live"
    case "reconnecting": return "Reconnecting"
    case "last_observed": return "Last observed"
  }
}

function managedSessionLabel(session: SessionProjection) {
  switch (session.purpose) {
    case "orchestration": return "Orchestrator"
    case "coding": return "Coding Agent"
    case "evaluation": return "Evaluation Agent"
  }
}

function managedSessionTree(sessions: readonly SessionProjection[]) {
  const ordered = sessions.toSorted((left, right) =>
    left.createdAtUnixMs - right.createdAtUnixMs)
  const ids = new Set(ordered.map((session) => session.id))
  const children = new Map<string, SessionProjection[]>()
  for (const session of ordered) {
    if (!session.parentSessionId) continue
    const existing = children.get(session.parentSessionId) ?? []
    existing.push(session)
    children.set(session.parentSessionId, existing)
  }
  const result: Array<{ session: SessionProjection; depth: number }> = []
  const visited = new Set<string>()
  const visit = (session: SessionProjection, depth: number) => {
    if (visited.has(session.id)) return
    visited.add(session.id)
    result.push({ session, depth })
    for (const child of children.get(session.id) ?? []) visit(child, depth + 1)
  }
  for (const session of ordered) {
    if (!session.parentSessionId || !ids.has(session.parentSessionId)) {
      visit(session, 0)
    }
  }
  for (const session of ordered) visit(session, 0)
  return result
}

function PublishDraftDialog({
  draft,
  hasPassingRun,
  blocked,
  open,
  onOpenChange,
  showTrigger = true,
  emitIntent,
}: {
  draft: AgentDraftProjection
  hasPassingRun: boolean
  blocked: boolean
  open?: boolean
  onOpenChange?: (open: boolean) => void
  showTrigger?: boolean
  emitIntent: EmitIntent
}) {
  const [bump, setBump] = React.useState<"patch" | "minor" | "major">("patch")
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {showTrigger ? (
        <DialogTrigger render={<Button variant="outline" disabled={blocked} />}>
          <TagIcon data-icon="inline-start" />
          Create Version
        </DialogTrigger>
      ) : null}
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create immutable Version</DialogTitle>
          <DialogDescription>
            Agent Factory commits all non-ignored Draft files and creates an
            annotated Git tag. This Version cannot be edited.
          </DialogDescription>
        </DialogHeader>
        {!hasPassingRun ? (
          <Alert>
            <TriangleAlertIcon />
            <AlertTitle>No passing Run</AlertTitle>
            <AlertDescription>
              You can continue, but this Draft has no passing evaluation evidence.
            </AlertDescription>
          </Alert>
        ) : null}
        <Field>
          <FieldLabel>Version bump</FieldLabel>
          <Select value={bump} onValueChange={(value) =>
            setBump(value as "patch" | "minor" | "major")}>
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value="patch">Patch</SelectItem>
                <SelectItem value="minor">Minor</SelectItem>
                <SelectItem value="major">Major</SelectItem>
              </SelectGroup>
            </SelectContent>
          </Select>
        </Field>
        <DialogFooter showCloseButton>
          <Button onClick={() => void emitIntent({
            type: "agentDraft.publish",
            agentDraftId: draft.id,
            bump,
            confirmWithoutPassingRun: !hasPassingRun,
          })}>
            Create Version
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export type SessionHistoryKind = "orchestrator" | "coding" | "evaluation"

export type SessionHistoryEntry = {
  id: string
  kind: SessionHistoryKind
  runId?: string
  sessionId?: string
  depth: number
  label: string
  status: string
  initialInput: string
  finalOutput?: string
  completed: boolean
}

export type SessionHistoryGroup = {
  id: string
  run?: FactoryRunProjection
  entries: SessionHistoryEntry[]
}

export function draftSessionHistory(
  draft: AgentDraftProjection,
  runs: readonly FactoryRunProjection[],
  sessions: readonly SessionProjection[],
): SessionHistoryGroup[] {
  const groups: SessionHistoryGroup[] = []
  for (const run of runs) {
    const runSessions = sessionsForRun(run, sessions)
    const eligible = runIsTerminal(run)
      ? runSessions
      : runSessions.filter(isHistoricalSession)
    const entries: SessionHistoryEntry[] = []
    const orchestrator = eligible.find((session) =>
      session.purpose === "orchestration")
    if (runIsTerminal(run) || orchestrator) {
      entries.push(orchestratorHistoryEntry(run, eligible))
    }
    for (const { session, depth } of managedSessionTree(eligible)) {
      if (session.purpose === "orchestration") continue
      entries.push(agentHistoryEntry(run.id, session, Math.max(depth, 1)))
    }
    if (entries.length > 0) groups.push({ id: run.id, run, entries })
  }

  const direct = managedSessionTree(sessions.filter((session) =>
    session.workspaceBindingId === draft.workspaceBindingId &&
    !session.factoryRunId &&
    session.purpose !== "orchestration" &&
    isHistoricalSession(session)))
    .map(({ session, depth }) =>
      agentHistoryEntry(undefined, session, depth))
  if (direct.length > 0) {
    groups.push({ id: "direct-managed-sessions", entries: direct })
  }
  return groups
}

function isHistoricalSession(session: SessionProjection) {
  return Boolean(session.outcome) || session.availability === "historical"
}

function sessionsForRun(
  run: FactoryRunProjection,
  sessions: readonly SessionProjection[],
): SessionProjection[] {
  return sessions
    .filter((session) => session.factoryRunId === run.id)
    .toSorted((left, right) =>
      left.createdAtUnixMs - right.createdAtUnixMs)
}

function orchestratorHistoryEntry(
  run: FactoryRunProjection,
  sessions: readonly SessionProjection[],
): SessionHistoryEntry {
  const session = sessions.find((candidate) =>
    candidate.factoryRunId === run.id &&
    candidate.purpose === "orchestration")
  const completed = ["passed", "failed", "needs_review", "cancelled"]
    .includes(run.state)
  return {
    id: session?.id ?? `orchestrator:${run.id}`,
    kind: "orchestrator",
    runId: run.id,
    sessionId: session?.id,
    depth: 0,
    label: "Orchestrator",
    status: session
      ? sessionStatusLabel(session)
      : runStateLabel(run.state),
    initialInput: session?.initialPrompt ?? orchestratorInput(run),
    finalOutput: completed ? orchestratorOutput(run) : undefined,
    completed,
  }
}

function agentHistoryEntry(
  runId: string | undefined,
  session: SessionProjection,
  depth: number,
): SessionHistoryEntry {
  const completed = Boolean(session.outcome) ||
    session.availability === "historical"
  return {
    id: session.id,
    kind: session.purpose === "evaluation" ? "evaluation" : "coding",
    runId,
    sessionId: session.id,
    depth,
    label: session.purpose === "evaluation" ? "Evaluation" : "Coding",
    status: sessionStatusLabel(session),
    initialInput: session.initialPrompt ?? session.title,
    completed,
  }
}

function orchestratorInput(run: FactoryRunProjection) {
  const criteria = run.acceptanceCriteria
    .map((criterion, index) => `${index + 1}. ${criterion}`)
    .join("\n")
  return `Objective:\n${run.objective}\n\nAcceptance criteria:\n${criteria}`
}

function orchestratorOutput(run: FactoryRunProjection) {
  if (run.state === "cancelled") return "The run was cancelled."
  if (!run.evaluation) return runStateLabel(run.state)
  const findings = run.evaluation.findings
    .map((finding) => `- ${finding.title}: ${finding.evidence}`)
    .join("\n")
  return [
    `Verdict: ${run.evaluation.verdict}`,
    run.evaluation.summary,
    findings,
    run.evaluation.validationError,
  ].filter((part) => part && part.length > 0).join("\n\n")
}

function historyKindLabel(kind: SessionHistoryKind) {
  switch (kind) {
    case "orchestrator": return "Orchestrator"
    case "coding": return "Coding"
    case "evaluation": return "Evaluation"
  }
}

function SessionHistory({
  draft,
  runs,
  sessions,
  readTranscript,
}: {
  draft: AgentDraftProjection
  runs: readonly FactoryRunProjection[]
  sessions: readonly SessionProjection[]
  readTranscript?: ReadAgentTranscript
}) {
  const groups = draftSessionHistory(draft, runs, sessions)
  return (
    <section
      id={`run-history-${draft.id}`}
      aria-labelledby="run-history-title"
      className="flex scroll-mt-4 flex-col gap-4"
    >
      <div className="space-y-1">
        <h2 id="run-history-title" className="text-sm font-semibold tracking-tight">
          Run History
        </h2>
        <p className="text-xs text-muted-foreground">
          Orchestrator, Coding, and Evaluation sessions for this Draft.
        </p>
      </div>
      {groups.length === 0 ? (
        <Empty className="border border-dashed">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <HistoryIcon />
            </EmptyMedia>
            <EmptyTitle>No session history yet</EmptyTitle>
            <EmptyDescription>
              Completed Orchestrator, Coding, and Evaluation sessions appear
              here under the Run that created them.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-lg border">
          {groups.map((group) => (
            <section
              key={group.id}
              aria-label={group.run
                ? `${runIsTerminal(group.run) ? "Run" : "Current Run"} session history: ${group.run.objective}`
                : "Direct managed session history"}
              className="flex flex-col gap-3 p-4"
            >
              <div className="flex items-center gap-2">
                <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                  {group.run
                    ? runIsTerminal(group.run) ? "Run" : "Current Run"
                    : "Direct managed sessions"}
                </h3>
                {group.run ? (
                  <Badge variant="outline" className="shrink-0">
                    {runStateLabel(group.run.state)}
                  </Badge>
                ) : null}
                {group.run ? (
                  <span className="min-w-0 truncate text-xs text-muted-foreground">
                    · {group.run.objective}
                  </span>
                ) : null}
              </div>
              <div className="flex flex-col divide-y divide-border/50">
                {group.entries.map((entry) => (
                  <div key={entry.id} className={cn(entry.depth > 0 && "ml-4 border-l pl-4")}>
                    <SessionHistoryRow
                      entry={entry}
                      readTranscript={readTranscript}
                    />
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}
    </section>
  )
}

function SessionHistoryRow({
  entry,
  readTranscript,
}: {
  entry: SessionHistoryEntry
  readTranscript?: ReadAgentTranscript
}) {
  return (
    <Collapsible className="group py-0.5">
      <CollapsibleTrigger
        render={(
          <button
            type="button"
            aria-label={`${entry.label} session, ${entry.status}`}
            className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring"
          />
        )}
      >
        <span className="text-muted-foreground transition-transform group-data-[panel-open]:rotate-90 in-data-panel-open:rotate-90 [&_svg]:size-4">
          <ChevronRightIcon aria-hidden="true" />
        </span>
        <Badge variant="outline" className="shrink-0">{historyKindLabel(entry.kind)}</Badge>
        <span className="min-w-0 flex-1 truncate text-sm font-medium">{entry.label}</span>
        <span className="shrink-0 text-xs text-muted-foreground">
          {entry.status}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="flex flex-col gap-3 px-2 pb-3 pt-1">
        <div className="flex flex-col gap-1">
          <p className="text-xs font-medium">Initial input</p>
          <pre className="whitespace-pre-wrap rounded-md bg-muted/30 p-3 font-mono text-xs">
            {entry.initialInput}
          </pre>
        </div>
        {entry.completed ? (
          <div className="flex flex-col gap-1">
            <p className="text-xs font-medium">Final output</p>
            {entry.finalOutput ? (
              <pre className="whitespace-pre-wrap rounded-md bg-muted/30 p-3 font-mono text-xs">
                {entry.finalOutput}
              </pre>
            ) : entry.sessionId ? (
              <HistoryTranscript
                sessionId={entry.sessionId}
                readTranscript={readTranscript}
              />
            ) : (
              <p className="text-xs text-muted-foreground">
                No final output is available.
              </p>
            )}
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            Final output appears when this session completes.
          </p>
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}

function HistoryTranscript({
  sessionId,
  readTranscript,
}: {
  sessionId: string
  readTranscript?: ReadAgentTranscript
}) {
  const [text, setText] = React.useState<string>()
  const [error, setError] = React.useState<string>()
  const loadedFor = React.useRef<string | undefined>(undefined)
  const bind = React.useCallback((node: HTMLPreElement | null) => {
    if (!node || !readTranscript) return
    if (loadedFor.current === sessionId) return
    loadedFor.current = sessionId
    void readTranscript(sessionId).then(
      (transcript) => {
        setText(transcript.text)
        setError(undefined)
      },
      (reason: unknown) => {
        setError(reason instanceof Error ? reason.message : String(reason))
      },
    )
  }, [readTranscript, sessionId])
  if (!readTranscript) {
    return (
      <p className="text-xs text-muted-foreground">
        Open the session terminal to inspect output.
      </p>
    )
  }
  return (
    <pre
      ref={bind}
      className="whitespace-pre-wrap rounded-md bg-muted/30 p-3 font-mono text-xs"
    >
      {error ?? text ?? "Loading output…"}
    </pre>
  )
}
