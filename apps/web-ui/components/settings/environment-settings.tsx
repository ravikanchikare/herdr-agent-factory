import * as React from "react"

import type {
  PluginListDto,
  RuntimeIntent,
  EnvironmentConfigurationDraftDto,
  EnvironmentDto,
  EnvironmentVariableDto,
  EnvironmentLlmPolicyDto,
  EnvironmentPluginProjection,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"
import { Alert, AlertDescription, AlertTitle } from "@agent-factory/ui/components/alert"
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
import { Badge } from "@agent-factory/ui/components/badge"
import { Button } from "@agent-factory/ui/components/button"
import { Label } from "@agent-factory/ui/components/label"
import { Spinner } from "@agent-factory/ui/components/spinner"
import { Switch } from "@agent-factory/ui/components/switch"
import {
  Globe2Icon,
  PuzzleIcon,
} from "lucide-react"

import type { EmitIntent } from "@/components/shell/workspace-shell"
import {
  SettingsPrimaryAction,
  SettingsDetailsActionBar,
  SettingsDetailsActionMenu,
  SettingsDetailsNavigation,
  SettingsDetailsTitle,
  SettingsDisclosureRow,
  SettingsEmpty,
  SettingsGroup,
  SettingsList,
  SettingsRow,
  SettingsRowActions,
  SettingsRowMain,
  SettingsRowMeta,
  SettingsRowTitle,
  SettingsSection,
  useSettingsErrorToast,
} from "@/components/settings/settings-primitives"
import {
  UnsavedChangesDialog,
  useUnsavedChangesGuard,
} from "@/components/shell/unsaved-changes-dialog"
import { EnvironmentVariableSettings } from "@/components/settings/environment-variable-settings"
import { EnvironmentLlmSettings } from "@/components/settings/environment-llm-settings"

type EnvironmentIntent = Extract<
  RuntimeIntent,
  | { type: "environment.create" }
  | { type: "environment.configuration.set" }
  | { type: "environment.delete" }
>

/// Which Environment the form is editing. A draft is an Environment that does not exist yet;
/// it is edited through the same form, so creating and configuring are the same
/// activity rather than two different ones.
export type EnvironmentSettingsSelection =
  | { kind: "environment"; id: string }
  | { kind: "draft" }

const emptyDraft: EnvironmentConfigurationDraftDto = {
  name: "",
  environmentVariables: [],
  llm: null,
  plugins: [],
  registries: [],
}

export function EnvironmentSettings({
  projection,
  emitIntent,
  createRequested,
  onCreateRequestHandled,
  onDirtyChange,
  selection,
  onSelectionChange,
  onOpenProviders,
}: {
  projection: WorkspaceProjection
  emitIntent: EmitIntent
  /** Set when something outside this section asked to start a new Environment. */
  createRequested: boolean
  onCreateRequestHandled: () => void
  onDirtyChange?: (isDirty: boolean) => void
  selection?: EnvironmentSettingsSelection
  onSelectionChange: (selection?: EnvironmentSettingsSelection) => void
  onOpenProviders: () => void
}) {
  const [draft, setDraft] = React.useState<EnvironmentConfigurationDraftDto>(emptyDraft)
  const [savedDraft, setSavedDraft] =
    React.useState<EnvironmentConfigurationDraftDto>(emptyDraft)
  const [confirmDelete, setConfirmDelete] = React.useState<string>()
  const [isPending, startTransition] = React.useTransition()
  const providersById = new Map(
    projection.llmProviders.map((provider) => [provider.id, provider]),
  )

  const selectedEnvironment =
    selection?.kind === "environment"
      ? projection.environments.find((environment) => environment.id === selection.id)
      : undefined
  const isDraft = selection?.kind === "draft"

  // Dirty means the user changed something. A draft is measured against the
  // configuration a new draft opens with — which may already name the one
  // available provider — so an untouched New Environment warns about nothing.
  const initialDraft = newDraft(projection.llmProviders)
  const isDirty = isDraft
    ? dirtyAgainst(initialDraft, draft)
    : selectedEnvironment
      ? dirtyAgainst(savedDraft, draft)
      : false
  // A new detail opens with its name field showing; a saved Environment shows
  // the field only when Rename asks for it. Adjusted during render.
  const [editingName, setEditingName] = React.useState(isDraft)
  const [wasDraft, setWasDraft] = React.useState(isDraft)
  if (isDraft !== wasDraft) {
    setWasDraft(isDraft)
    setEditingName(isDraft)
  }
  const guard = useUnsavedChangesGuard(isDirty)
  useSettingsErrorToast("Environment operation failed", projection.environmentError)
  useSettingsErrorToast("Plugin operation failed", projection.pluginError)

  const loadDraft = React.useCallback((environment: EnvironmentDto | undefined) => {
    const configuration = environment ? draftOf(environment) : emptyDraft
    setDraft(configuration)
    setSavedDraft(configuration)
  }, [])

  // Reloading the draft is deliberately keyed on the selection alone. It must
  // not react to the projection, or an unrelated `environment.changed` event would
  // wipe the edits in progress.
  const selectedId = selection?.kind === "environment" ? selection.id : undefined
  const environmentsRef = React.useRef(projection.environments)
  // Declared first so it has already run when the effect below reads it.
  React.useLayoutEffect(() => {
    environmentsRef.current = projection.environments
  })
  React.useLayoutEffect(() => {
    if (!selectedId) return
    loadDraft(environmentsRef.current.find((environment) => environment.id === selectedId))
  }, [selectedId, loadDraft])

  React.useEffect(() => {
    if (selection?.kind === "environment" && !selectedEnvironment) {
      onSelectionChange(undefined)
    }
  }, [onSelectionChange, selectedEnvironment, selection])

  React.useLayoutEffect(() => {
    onDirtyChange?.(isDirty)
  }, [isDirty, onDirtyChange])

  const startDraft = React.useCallback(() => {
    onSelectionChange({ kind: "draft" })
    setDraft(newDraft(projection.llmProviders))
  }, [onSelectionChange, projection.llmProviders])

  const didHandleCreateRequest = React.useRef(false)
  React.useEffect(() => {
    if (!createRequested) {
      didHandleCreateRequest.current = false
      return
    }
    if (!createRequested || didHandleCreateRequest.current) return
    didHandleCreateRequest.current = true
    onCreateRequestHandled()
    guard.request(startDraft)
  }, [createRequested, onCreateRequestHandled, guard, startDraft])

  const dispatch = (intent: EnvironmentIntent, after?: () => void) => {
    startTransition(async () => {
      await emitIntent(intent)
      after?.()
    })
  }

  const save = () => {
    if (isDraft) {
      dispatch({ type: "environment.create", configuration: draft }, () =>
        onSelectionChange(undefined),
      )
      return
    }
    if (!selectedEnvironment) return
    const configuration = draft
    dispatch({
      type: "environment.configuration.set",
      environmentId: selectedEnvironment.id,
      configuration,
    }, () => {
      setSavedDraft(configuration)
      setEditingName(false)
    })
  }

  /// Renaming from the title commits on its own, but only when the name is the
  /// single difference: `environment.configuration.set` carries the whole
  /// configuration, so any other pending edit still waits for Save.
  const commitName = () => {
    setEditingName(false)
    const name = draft.name.trim()
    if (isDraft) {
      if (name !== draft.name) setDraft({ ...draft, name })
      return
    }
    if (!selectedEnvironment) return
    if (name.length === 0 || name === selectedEnvironment.name) {
      setDraft({ ...draft, name: selectedEnvironment.name })
      return
    }
    const configuration = { ...draft, name }
    setDraft(configuration)
    const renameOnly = { ...draftOf(selectedEnvironment), name }
    if (dirtyAgainst(renameOnly, configuration)) return
    dispatch({
      type: "environment.configuration.set",
      environmentId: selectedEnvironment.id,
      configuration,
    }, () => setSavedDraft(configuration))
  }

  const cancelName = () => {
    if (selectedEnvironment) {
      setDraft({ ...draft, name: selectedEnvironment.name })
    }
    setEditingName(false)
  }

  const canSave =
    draft.name.trim().length > 0 &&
    isDirty &&
    validLlmPolicy(draft.llm ?? null, projection.llmProviders)

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      {selection === undefined ? (
        projection.environments.length === 0 ? (
          <SettingsEmpty
            icon={<Globe2Icon />}
            title="No Environments yet"
            description="An Environment defines the variables, Intelligence Provider, Skills, Tools, agents, and permissions that bound a Harness agent. Add one to get started."
            action={
              <SettingsPrimaryAction label="Add" onClick={startDraft} />
            }
          />
        ) : (
          <SettingsList>
            {projection.environments.map((environment) => {
              const status = environmentStatus(environment)
              const providerName = environment.llm
                ? providersById.get(environment.llm.providerId)?.name ??
                  "Missing Intelligence Provider"
                : "No Intelligence Provider"
              return (
                <SettingsRow
                  key={environment.id}
                  icon={<Globe2Icon />}
                  onOpen={() =>
                    onSelectionChange({
                      kind: "environment",
                      id: environment.id,
                    })
                  }
                >
                  <SettingsRowMain openLabel={`Open ${environment.name}`}>
                    <SettingsRowTitle>{environment.name}</SettingsRowTitle>
                    <SettingsRowMeta>{providerName}</SettingsRowMeta>
                  </SettingsRowMain>
                  <SettingsRowActions>
                    <Badge variant={status === "ready" ? "secondary" : "outline"}>
                      {statusLabel(status)}
                    </Badge>
                  </SettingsRowActions>
                </SettingsRow>
              )
            })}
          </SettingsList>
        )
      ) : isDraft || selectedEnvironment ? (
        <div className="flex min-h-0 flex-1 flex-col gap-6">
          <SettingsDetailsNavigation
            parent="Environments"
            current={draft.name || "New Environment"}
            onBack={() =>
              guard.request(() => {
                loadDraft(selectedEnvironment)
                setEditingName(false)
                onSelectionChange(undefined)
              })
            }
          />
          <div className="flex items-start justify-between gap-4">
            <div className="flex min-w-0 flex-col gap-1">
              <SettingsDetailsTitle
                name={draft.name}
                placeholder="New Environment"
                editing={editingName}
                disabled={isPending}
                onEdit={() => setEditingName(true)}
                onChange={(name) => setDraft({ ...draft, name })}
                onCommit={commitName}
                onCancel={cancelName}
              />
              {selectedEnvironment ? (
                <p className="text-xs text-muted-foreground">
                  {statusLabel(environmentStatus(selectedEnvironment))} · {" "}
                  {selectedEnvironment.llm
                    ? providersById.get(selectedEnvironment.llm.providerId)
                        ?.name ?? "Missing Intelligence Provider"
                    : "No Intelligence Provider"}
                </p>
              ) : null}
            </div>
            {selectedEnvironment ? (
              <div className="flex shrink-0 items-center gap-1">
                <SettingsDetailsActionMenu
                  name={selectedEnvironment.name}
                  disabled={isPending}
                  onDelete={() =>
                    guard.request(() =>
                      setConfirmDelete(selectedEnvironment.id),
                    )
                  }
                />
              </div>
            ) : null}
          </div>
          <EnvironmentConfigurationForm
            environment={selectedEnvironment}
            draft={draft}
            onDraftChange={setDraft}
            projection={projection}
            emitIntent={emitIntent}
            onOpenProviders={onOpenProviders}
            isPending={isPending}
            isDraft={isDraft}
            isDirty={isDirty}
            canSave={canSave}
            onSave={save}
            onDiscard={() => {
              if (isDraft) {
                onSelectionChange(undefined)
                return
              }
              loadDraft(selectedEnvironment)
              setEditingName(false)
            }}
          />
        </div>
      ) : null}

      <UnsavedChangesDialog
        open={guard.isOpen}
        onConfirm={guard.confirm}
        onCancel={guard.cancel}
      />

      <AlertDialog
        open={confirmDelete !== undefined}
        onOpenChange={(open) => !open && setConfirmDelete(undefined)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this Environment?</AlertDialogTitle>
            <AlertDialogDescription>
              Its environment, provider, models, and plugin selection are removed
              permanently. Past sessions keep their history, and this Environment stops
              being available for new ones.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                const environmentId = confirmDelete
                setConfirmDelete(undefined)
                if (!environmentId) return
                dispatch({ type: "environment.delete", environmentId }, () =>
                  onSelectionChange(undefined),
                )
              }}
            >
              Delete Environment
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

/// One form for both creating and configuring. `environment` is absent for a draft;
/// everything else is identical, which is the point — a new Environment is configured
/// exactly the way an existing one is.
function EnvironmentConfigurationForm({
  environment,
  draft,
  onDraftChange,
  projection,
  emitIntent,
  onOpenProviders,
  isPending,
  isDraft,
  isDirty,
  canSave,
  onSave,
  onDiscard,
}: {
  environment?: EnvironmentDto
  draft: EnvironmentConfigurationDraftDto
  onDraftChange: (draft: EnvironmentConfigurationDraftDto) => void
  projection: WorkspaceProjection
  emitIntent: EmitIntent
  onOpenProviders: () => void
  isPending: boolean
  isDraft: boolean
  isDirty: boolean
  canSave: boolean
  onSave: () => void
  onDiscard: () => void
}) {
  const issues = environment?.readiness.issues ?? []
  const environmentIssues = issues.filter((issue) =>
    issue.startsWith("Environment variable "),
  )
  const providerIssues = issues.filter(
    (issue) => !issue.startsWith("Environment variable "),
  )

  const patch = (next: Partial<EnvironmentConfigurationDraftDto>) =>
    onDraftChange({ ...draft, ...next })

  return (
    // No scroll container of its own: the Settings pane is the scroller, which
    // is what lets the action bar below actually stick to the viewport.
    <div className="flex flex-1 flex-col gap-6">
      <div className="flex flex-1 flex-col gap-12">
        <div className="flex flex-col gap-2">
          <EnvironmentVariableSettings
            environment={draft.environmentVariables}
            secrets={projection.secrets}
            onChange={(environmentVariables) =>
              patch({
                environmentVariables: [...environmentVariables],
              })
            }
            disabled={isPending}
            issues={environmentIssues}
          />
        </div>

        <div className="flex flex-col gap-2">
          <EnvironmentLlmSettings
            policy={draft.llm ?? null}
            providers={projection.llmProviders}
            onChange={(llm) => patch({ llm })}
            onOpenProviders={onOpenProviders}
            disabled={isPending}
            needsSetup={environment?.llmNeedsSetup ?? false}
            issues={providerIssues}
          />
        </div>

        <SettingsSection
          title="Skills & Tools"
          description="Choose which installed plugin skills and MCP tools this Environment uses."
        >
          <EnvironmentPluginsSection
            plugins={draft.plugins}
            installed={projection.plugins.installed}
            emitIntent={emitIntent}
            onChange={(plugins) => patch({ plugins: [...plugins] })}
            disabled={isPending}
          />
        </SettingsSection>
      </div>

      {/* Only an edit in progress earns the sticky bar. Creating keeps its
          actions in the normal flow with nothing drawn above them. */}
      {isDraft || isDirty || isPending ? (
        <SettingsDetailsActionBar sticky={!isDraft && (isDirty || isPending)}>
          {isPending ? <Spinner /> : null}
          <Button
            type="button"
            variant="ghost"
            disabled={isPending}
            onClick={onDiscard}
          >
            Cancel
          </Button>
          <Button type="button" disabled={isPending || !canSave} onClick={onSave}>
            {environment ? "Save" : "Add"}
          </Button>
        </SettingsDetailsActionBar>
      ) : null}
    </div>
  )
}

type InstalledPlugin = PluginListDto["installed"][number]

/// Registries are chosen in Settings › Plugins; an Environment only decides what to do
/// with what is already installed.
function EnvironmentPluginsSection({
  plugins,
  installed,
  emitIntent,
  onChange,
  disabled,
}: {
  plugins: readonly EnvironmentPluginProjection[]
  installed: readonly InstalledPlugin[]
  emitIntent: EmitIntent
  onChange: (plugins: readonly EnvironmentPluginProjection[]) => void
  disabled: boolean
}) {
  const applicable = installed.filter(
    (plugin) => plugin.skills.length > 0 || plugin.mcpServers.length > 0,
  )
  // Rust owns installed plugin offerings. This only asks for the current
  // list when the Environment form mounts so Skills & Tools can render
  // what is actually installed.
  const [pluginsListed, setPluginsListed] = React.useState(false)
  React.useEffect(() => {
    void emitIntent({ type: "plugin.list" }).finally(() =>
      setPluginsListed(true),
    )
  }, [emitIntent])

  const entry = (name: string) =>
    plugins.find((plugin) => plugin.name === name)

  const replaceOrAppend = (next: EnvironmentPluginProjection) => {
    const index = plugins.findIndex((plugin) => plugin.name === next.name)
    if (index === -1) return [...plugins, next]
    const updated = [...plugins]
    updated[index] = next
    return updated
  }

  const setEnabled = (plugin: InstalledPlugin, enabled: boolean) => {
    if (!enabled) {
      onChange(plugins.filter((entry) => entry.name !== plugin.name))
      return
    }
    onChange(
      replaceOrAppend({
        name: plugin.name,
        defaultSkills: plugin.skills.map((skill) => skill.name),
        enabledMcpServers: plugin.mcpServers.map((server) => server.name),
      }),
    )
  }

  const toggleResource = (
    plugin: InstalledPlugin,
    key: "defaultSkills" | "enabledMcpServers",
    name: string,
    checked: boolean,
  ) => {
    const existing = entry(plugin.name) ?? {
      name: plugin.name,
      defaultSkills: [],
      enabledMcpServers: [],
    }
    const current = existing[key]
    onChange(
      replaceOrAppend({
        ...existing,
        [key]: checked
          ? Array.from(new Set([...current, name]))
          : current.filter((candidate) => candidate !== name),
      }),
    )
  }

  const emptyTitle =
    installed.length === 0
      ? "No plugins installed"
      : "No skills or tools available"
  const emptyDescription =
    installed.length === 0
      ? "Install plugins from a registry in Settings › Plugins."
      : "Installed plugins do not offer skills or MCP tools this Environment can use."

  return (
    <SettingsList>
      {!pluginsListed && applicable.length === 0 ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner />
          Loading installed plugins
        </div>
      ) : applicable.length === 0 ? (
        <SettingsEmpty
          icon={<PuzzleIcon />}
          title={emptyTitle}
          description={emptyDescription}
        />
      ) : (
        applicable.map((plugin) => {
          const selected = entry(plugin.name)
          return (
            <InstalledPluginCollapsible
              key={plugin.name}
              plugin={plugin}
              selected={selected}
              disabled={disabled}
              onEnabledChange={(enabled) => setEnabled(plugin, enabled)}
              onResourceChange={(key, name, checked) =>
                toggleResource(plugin, key, name, checked)
              }
            />
          )
        })
      )}
    </SettingsList>
  )
}

function InstalledPluginCollapsible({
  plugin,
  selected,
  disabled,
  onEnabledChange,
  onResourceChange,
}: {
  plugin: InstalledPlugin
  /** Absent while the Environment does not use this plugin. */
  selected?: EnvironmentPluginProjection
  disabled: boolean
  onEnabledChange: (enabled: boolean) => void
  onResourceChange: (
    key: "defaultSkills" | "enabledMcpServers",
    name: string,
    checked: boolean,
  ) => void
}) {
  const enabled = selected !== undefined
  const switchId = `plugin-enabled-${plugin.name}`
  // Skills and tools stay switchable only while the plugin itself is on, but
  // they are always readable, so what a plugin brings is visible before it is
  // enabled.
  const resourcesDisabled = disabled || !enabled
  // Whatever the Environment already uses starts expanded, and enabling a plugin
  // reveals what that just turned on.
  const [open, setOpen] = React.useState(enabled)

  return (
    <SettingsDisclosureRow
      icon={<PuzzleIcon />}
      title={plugin.name}
      meta={`Version ${plugin.activeVersion}`}
      open={open}
      onOpenChange={setOpen}
      actions={
        <Switch
          id={switchId}
          checked={enabled}
          disabled={disabled}
          aria-label={`Enable ${plugin.name}`}
          onCheckedChange={(checked) => {
            onEnabledChange(checked as boolean)
            if (checked) setOpen(true)
          }}
        />
      }
    >
      {plugin.skills.length === 0 && plugin.mcpServers.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          This plugin offers no skills or tools.
        </p>
      ) : null}
      {plugin.skills.length > 0 ? (
        <SettingsGroup label="Skills">
          {plugin.skills.map((skill) => {
            const skillId = `plugin-skill-${plugin.name}-${skill.name}`
            return (
              <div key={skill.name} className="flex items-start gap-3">
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <Label htmlFor={skillId} className="text-xs font-medium">
                    {skill.name}
                  </Label>
                  {skill.description ? (
                    <p className="text-xs text-muted-foreground">
                      {skill.description}
                    </p>
                  ) : null}
                </div>
                <Switch
                  id={skillId}
                  className="mt-0.5"
                  checked={selected?.defaultSkills.includes(skill.name) ?? false}
                  disabled={resourcesDisabled}
                  onCheckedChange={(value) =>
                    onResourceChange("defaultSkills", skill.name, value as boolean)
                  }
                />
              </div>
            )
          })}
        </SettingsGroup>
      ) : null}
      {plugin.mcpServers.length > 0 ? (
        <SettingsGroup label="Tools">
          {plugin.mcpDisabledReason ? (
            <Alert variant="destructive">
              <AlertTitle>Tools disabled</AlertTitle>
              <AlertDescription>{plugin.mcpDisabledReason}</AlertDescription>
            </Alert>
          ) : (
            plugin.mcpServers.map((server) => {
              const serverId = `plugin-mcp-${plugin.name}-${server.name}`
              return (
                <div key={server.name} className="flex items-start gap-3">
                  <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <Label htmlFor={serverId} className="text-xs font-medium">
                      {server.name}
                    </Label>
                    <p className="text-xs text-muted-foreground">{server.kind}</p>
                  </div>
                  <Switch
                    id={serverId}
                    className="mt-0.5"
                    checked={
                      selected?.enabledMcpServers.includes(server.name) ?? false
                    }
                    disabled={resourcesDisabled}
                    onCheckedChange={(value) =>
                      onResourceChange(
                        "enabledMcpServers",
                        server.name,
                        value as boolean,
                      )
                    }
                  />
                </div>
              )
            })
          )}
        </SettingsGroup>
      ) : null}
    </SettingsDisclosureRow>
  )
}

type EnvironmentStatus = "ready" | "needs_setup"

function environmentStatus(environment: EnvironmentDto): EnvironmentStatus {
  return environment.readiness.state
}

function statusLabel(status: EnvironmentStatus) {
  if (status === "ready") return "Ready"
  return "Needs setup"
}

/// What a New Environment starts as. With one provider available there is
/// nothing to choose, so the draft already names it.
function newDraft(
  providers: WorkspaceProjection["llmProviders"],
): EnvironmentConfigurationDraftDto {
  const provider = providers.length === 1 ? providers[0] : undefined
  return {
    ...emptyDraft,
    llm: provider
      ? {
          providerId: provider.id,
          allowedModels: [...provider.allowedModels],
          defaultModel: provider.allowedModels[0] ?? "",
        }
      : null,
  }
}

function draftOf(environment: EnvironmentDto): EnvironmentConfigurationDraftDto {
  return {
    name: environment.name,
    environmentVariables: [...environment.environmentVariables],
    llm: environment.llm ?? null,
    plugins: [...environment.plugins],
    registries: [...environment.registryIds],
  }
}

function dirtyAgainst(
  baseline: EnvironmentConfigurationDraftDto,
  draft: EnvironmentConfigurationDraftDto,
) {
  return (
    baseline.name !== draft.name ||
    environmentDirty(baseline.environmentVariables, draft.environmentVariables) ||
    providerDirty(baseline.llm ?? null, draft.llm ?? null) ||
    pluginsDirty(baseline.plugins, draft.plugins) ||
    !sameStringCollection(baseline.registries, draft.registries)
  )
}

function environmentDirty(
  original: readonly EnvironmentVariableDto[],
  draft: readonly EnvironmentVariableDto[],
) {
  const normalize = (entries: readonly EnvironmentVariableDto[]) =>
    entries
      .map((entry) =>
        JSON.stringify([entry.name, entry.source, entry.value]),
      )
      .toSorted()
  return !sameStringCollection(normalize(original), normalize(draft))
}

function providerDirty(
  original: EnvironmentLlmPolicyDto | null,
  draft: EnvironmentLlmPolicyDto | null,
) {
  if (original === null || draft === null) return original !== draft
  return (
    original.providerId !== draft.providerId ||
    original.defaultModel !== draft.defaultModel ||
    !sameStringCollection(original.allowedModels, draft.allowedModels)
  )
}

function pluginsDirty(
  original: readonly EnvironmentPluginProjection[],
  draft: readonly EnvironmentPluginProjection[],
) {
  const normalize = (plugins: readonly EnvironmentPluginProjection[]) =>
    plugins
      .map((plugin) => ({
        name: plugin.name,
        defaultSkills: plugin.defaultSkills.toSorted(),
        enabledMcpServers: plugin.enabledMcpServers.toSorted(),
      }))
      .toSorted((left, right) => left.name.localeCompare(right.name))
  return JSON.stringify(normalize(original)) !== JSON.stringify(normalize(draft))
}

function sameStringCollection(left: readonly string[], right: readonly string[]) {
  if (left.length !== right.length) return false
  const sortedLeft = left.toSorted()
  const sortedRight = right.toSorted()
  return sortedLeft.every((value, index) => value === sortedRight[index])
}

function validLlmPolicy(
  policy: EnvironmentLlmPolicyDto | null,
  providers: WorkspaceProjection["llmProviders"],
) {
  if (!policy) return false
  const provider = providers.find((candidate) => candidate.id === policy.providerId)
  if (!provider || provider.readiness.state !== "ready") return false
  if (policy.allowedModels.length === 0) return false
  return (
    policy.allowedModels.includes(policy.defaultModel) &&
    policy.allowedModels.every((model) => provider.allowedModels.includes(model))
  )
}
