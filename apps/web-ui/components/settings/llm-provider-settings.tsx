import * as React from "react"

import type {
  LlmProviderConfigurationDto,
  LlmProviderConnectionDto,
  LlmProviderDto,
  LlmProviderType,
  RuntimeIntent,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"
import { llmProviderKey } from "@agent-factory/runtime-client"
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
import { Checkbox } from "@agent-factory/ui/components/checkbox"
import { Input } from "@agent-factory/ui/components/input"
import { Label } from "@agent-factory/ui/components/label"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@agent-factory/ui/components/select"
import { Spinner } from "@agent-factory/ui/components/spinner"
import { RefreshCwIcon, ServerIcon, SparklesIcon } from "lucide-react"
import { ScrollArea } from "@agent-factory/ui/components/scroll-area"

import type { EmitIntent } from "@/components/shell/workspace-shell"
import {
  SettingsPrimaryAction,
  SettingsDetailsActionBar,
  SettingsDetailsActionMenu,
  SettingsDetailsNavigation,
  SettingsDetailsTitle,
  SettingsEmpty,
  SettingsGroup,
  SettingsList,
  SettingsRow,
  SettingsRowActions,
  SettingsRowMain,
  SettingsRowMeta,
  SettingsRowTitle,
  useSettingsErrorToast,
} from "@/components/settings/settings-primitives"
import {
  UnsavedChangesDialog,
  useUnsavedChangesGuard,
} from "@/components/shell/unsaved-changes-dialog"

type ProviderIntent = Extract<
  RuntimeIntent,
  | { type: "llmProvider.create" }
  | { type: "llmProvider.configuration.set" }
  | { type: "llmProvider.delete" }
  | { type: "llmProvider.models.list" }
>

export type LlmProviderSettingsSelection =
  | { kind: "provider"; id: string }
  | { kind: "draft" }

const endpoints: Record<LlmProviderType, string> = {
  ollama: "http://127.0.0.1:11434",
  litellm: "",
  meta: "https://api.meta.ai/v1",
  openai: "https://api.openai.com/v1",
}

const emptyDraft: LlmProviderConfigurationDto = {
  name: "",
  type: "ollama",
  endpoint: endpoints.ollama,
  credentialRef: null,
  allowedModels: [],
}

// Discovery only accepts a connection: type, endpoint, and credentialRef.
// Passing a full provider draft (name, allowedModels) is rejected by the
// backend as an unknown-field error.
function connectionOf(
  source: Pick<LlmProviderConnectionDto, "type" | "endpoint" | "credentialRef">,
): LlmProviderConnectionDto {
  return {
    type: source.type,
    endpoint: source.endpoint,
    credentialRef: source.credentialRef ?? null,
  }
}

// A connection is discoverable once it has an endpoint and either a Secret or
// a provider kind that needs no credentials (Ollama runs unauthenticated on
// the loopback). This is the gate for both auto-discovery and the manual
// Refresh fallback.
function connectionComplete(conn: LlmProviderConnectionDto): boolean {
  return (
    conn.endpoint.trim() !== "" &&
    (conn.credentialRef != null || conn.type === "ollama")
  )
}

export function LlmProviderSettings({
  projection,
  emitIntent,
  createRequested,
  onCreateRequestHandled,
  onDirtyChange,
  selection,
  onSelectionChange,
}: {
  projection: WorkspaceProjection
  emitIntent: EmitIntent
  createRequested: boolean
  onCreateRequestHandled: () => void
  onDirtyChange?: (dirty: boolean) => void
  selection?: LlmProviderSettingsSelection
  onSelectionChange: (selection?: LlmProviderSettingsSelection) => void
}) {
  const [draft, setDraft] = React.useState(emptyDraft)
  const [savedDraft, setSavedDraft] = React.useState(emptyDraft)
  const [isPending, startTransition] = React.useTransition()
  const [confirmImpact, setConfirmImpact] = React.useState(false)
  const [confirmDelete, setConfirmDelete] = React.useState(false)
  const [discovering, setDiscovering] = React.useState(false)
  // Fires a model-discovery request for the given connection. No-op unless the
  // connection is complete, so callers can invoke it unconditionally from
  // field-change handlers and the open effect without re-checking the gate.
  const discoverWith = React.useCallback(
    (providerId: string | undefined, conn: LlmProviderConnectionDto) => {
      const provider = connectionOf(conn)
      if (!connectionComplete(provider)) return
      setDiscovering(true)
      void emitIntent({
        type: "llmProvider.models.list",
        providerId,
        provider,
      }).finally(() => setDiscovering(false))
    },
    [emitIntent],
  )
  const environmentsByProviderId = new Map<
    string,
    Array<WorkspaceProjection["environments"][number]>
  >()
  for (const environment of projection.environments) {
    const providerId = environment.llm?.providerId
    if (!providerId) continue
    const linked = environmentsByProviderId.get(providerId)
    if (linked) linked.push(environment)
    else environmentsByProviderId.set(providerId, [environment])
  }
  const secretsByReference = new Map(
    projection.secrets.map((secret) => [secret.secretRef, secret]),
  )

  const selectedId = selection?.kind === "provider" ? selection.id : undefined
  const selected = projection.llmProviders.find(
    (provider) => provider.id === selectedId,
  )
  const isDraft = selection?.kind === "draft"
  // A draft is dirty once it differs from the configuration it opened with, so
  // an untouched New Provider has nothing to warn about.
  const isDirty = selection !== undefined && !sameConfiguration(savedDraft, draft)
  const [editingName, setEditingName] = React.useState(isDraft)
  const [wasDraft, setWasDraft] = React.useState(isDraft)
  if (isDraft !== wasDraft) {
    setWasDraft(isDraft)
    setEditingName(isDraft)
  }
  const guard = useUnsavedChangesGuard(isDirty)
  useSettingsErrorToast("Provider operation failed", projection.llmProviderError)

  const providersRef = React.useRef(projection.llmProviders)
  React.useLayoutEffect(() => {
    providersRef.current = projection.llmProviders
  })
  React.useLayoutEffect(() => {
    if (!selectedId) return
    const provider = providersRef.current.find((candidate) => candidate.id === selectedId)
    if (!provider) return
    const next = draftOf(provider)
    setDraft(next)
    setSavedDraft(next)
    // Auto-discover on open so an existing, fully-configured provider surfaces
    // its currently-available models without a manual Refresh.
    discoverWith(provider.id, next)
  }, [selectedId, discoverWith])

  React.useEffect(() => {
    if (selection?.kind === "provider" && !selected) {
      onSelectionChange(undefined)
    }
  }, [onSelectionChange, selected, selection])

  React.useLayoutEffect(
    () => onDirtyChange?.(isDirty),
    [isDirty, onDirtyChange],
  )

  const startDraft = React.useCallback(() => {
    onSelectionChange({ kind: "draft" })
    setDraft(emptyDraft)
    setSavedDraft(emptyDraft)
  }, [onSelectionChange])
  const handledCreate = React.useRef(false)
  React.useEffect(() => {
    if (!createRequested) {
      handledCreate.current = false
      return
    }
    if (!createRequested || handledCreate.current) return
    handledCreate.current = true
    onCreateRequestHandled()
    guard.request(startDraft)
  }, [createRequested, guard, onCreateRequestHandled, startDraft])

  const dispatch = (intent: ProviderIntent, after?: () => void) => {
    startTransition(async () => {
      await emitIntent(intent)
      after?.()
    })
  }

  const linkedEnvironments = selected
    ? (environmentsByProviderId.get(selected.id) ?? [])
    : []
  const selectedSecretLabel = draft.credentialRef
    ? (secretsByReference.get(draft.credentialRef)?.label ?? "Secret unavailable")
    : "Select secret"
  const providerSecretUnavailable =
    draft.credentialRef !== null &&
    draft.credentialRef !== undefined &&
    !secretsByReference.has(draft.credentialRef)
  const conflictedEnvironments = linkedEnvironments.filter(
    (environment) =>
      providerSecretUnavailable ||
      environmentPolicyConflicts(environment.llm, draft),
  )
  const canSave =
    isDirty &&
    draft.name.trim().length > 0 &&
    draft.endpoint.trim().length > 0 &&
    draft.allowedModels.length > 0

  const save = () => {
    if (!canSave) return
    if (selected && conflictedEnvironments.length > 0) {
      setConfirmImpact(true)
      return
    }
    persist()
  }

  const persist = () => {
    if (isDraft) {
      dispatch({ type: "llmProvider.create", configuration: draft }, () =>
        onSelectionChange(undefined),
      )
    } else if (selected) {
      const configuration = draft
      dispatch({
        type: "llmProvider.configuration.set",
        providerId: selected.id,
        configuration,
      }, () => {
        setSavedDraft(configuration)
        setEditingName(false)
      })
    }
  }

  /// An inline rename is its own edit and saves itself. The runtime takes a
  /// whole configuration, so it only does that when the name is the single
  /// difference — anything else pending still belongs to Save.
  const commitName = () => {
    setEditingName(false)
    const name = draft.name.trim()
    if (isDraft) {
      if (name !== draft.name) setDraft({ ...draft, name })
      return
    }
    if (!selected) return
    if (name.length === 0 || name === selected.name) {
      setDraft({ ...draft, name: selected.name })
      return
    }
    const configuration = { ...draft, name }
    setDraft(configuration)
    if (!sameConfiguration({ ...draftOf(selected), name }, configuration)) return
    dispatch({
      type: "llmProvider.configuration.set",
      providerId: selected.id,
      configuration,
    }, () => setSavedDraft(configuration))
  }

  const cancelName = () => {
    if (selected) setDraft({ ...draft, name: selected.name })
    setEditingName(false)
  }

  const connection = connectionOf(draft)
  const discovery = projection.llmProviderModelDiscovery
  const discoveredModels =
    discovery?.providerKey === llmProviderKey(connection) ? discovery.models : []

  if (selection === undefined && projection.llmProviders.length === 0) {
    return (
      <SettingsEmpty
        icon={<SparklesIcon />}
        title="No Providers"
        description="Add a provider once, then reuse it across several Environments."
        action={
          <SettingsPrimaryAction label="Add" onClick={startDraft} />
        }
      />
    )
  }

  if (selection === undefined) {
    return (
      <SettingsList>
        {projection.llmProviders.map((provider) => {
          const environmentCount =
            environmentsByProviderId.get(provider.id)?.length ?? 0
          return (
            <SettingsRow
              key={provider.id}
              icon={<ServerIcon />}
              onOpen={() =>
                onSelectionChange({ kind: "provider", id: provider.id })
              }
            >
              <SettingsRowMain openLabel={`Open ${provider.name}`}>
                <SettingsRowTitle>{provider.name}</SettingsRowTitle>
                <SettingsRowMeta>
                  {providerTypeLabel(provider.type)} · {provider.allowedModels.length}{" "}
                  {provider.allowedModels.length === 1 ? "model" : "models"} ·{" "}
                  {environmentCount}{" "}
                  {environmentCount === 1 ? "Environment" : "Environments"}
                </SettingsRowMeta>
              </SettingsRowMain>
              <SettingsRowActions>
                <Badge
                  variant={
                    provider.readiness.state === "ready" ? "secondary" : "outline"
                  }
                >
                  {provider.readiness.state === "ready" ? "Ready" : "Needs setup"}
                </Badge>
              </SettingsRowActions>
            </SettingsRow>
          )
        })}
      </SettingsList>
    )
  }

  if (!isDraft && !selected) return null

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <SettingsDetailsNavigation
        parent="Providers"
        current={draft.name || "New Provider"}
        onBack={() =>
          guard.request(() => {
            setDraft(savedDraft)
            onSelectionChange(undefined)
          })
        }
      />
      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1">
          <SettingsDetailsTitle
            name={draft.name}
            placeholder="New Provider"
            editing={editingName}
            disabled={isPending}
            onEdit={() => setEditingName(true)}
            onChange={(name) => setDraft({ ...draft, name })}
            onCommit={commitName}
            onCancel={cancelName}
          />
          {selected ? (
            <p className="text-xs text-muted-foreground">
              {providerTypeLabel(selected.type)} · {selected.allowedModels.length}{" "}
              {selected.allowedModels.length === 1 ? "model" : "models"}
            </p>
          ) : null}
        </div>
        {selected ? (
          <SettingsDetailsActionMenu
            name={selected.name}
            disabled={isPending}
            onDelete={() => guard.request(() => setConfirmDelete(true))}
          />
        ) : null}
      </div>
      <SettingsList>
        {selected?.readiness.state === "needs_setup" ? (
          <Alert>
            <AlertTitle>Provider needs setup</AlertTitle>
            <AlertDescription>{selected.readiness.issues.join(" ")}</AlertDescription>
          </Alert>
        ) : null}

        <SettingsRow>
          <SettingsRowMain>
            <SettingsRowTitle><Label htmlFor="provider-type">Type</Label></SettingsRowTitle>
            <SettingsRowMeta>Determines the model discovery protocol.</SettingsRowMeta>
          </SettingsRowMain>
          <SettingsRowActions>
            <Select
              value={draft.type}
              disabled={isPending}
              onValueChange={(value) => {
                if (!value || !(value in endpoints)) return
                const type = value as LlmProviderType
                const endpoint = endpoints[type]
                setDraft({
                  ...draft,
                  type,
                  endpoint,
                  allowedModels: [],
                })
                discoverWith(selected?.id, {
                  type,
                  endpoint,
                  credentialRef: draft.credentialRef,
                })
              }}
            >
              <SelectTrigger id="provider-type" className="w-64" aria-label="Provider type">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="ollama">Ollama</SelectItem>
                  <SelectItem value="litellm">LiteLLM</SelectItem>
                  <SelectItem value="meta">Meta</SelectItem>
                  <SelectItem value="openai">OpenAI</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </SettingsRowActions>
        </SettingsRow>

        <SettingsRow>
          <SettingsRowMain>
            <SettingsRowTitle><Label htmlFor="provider-endpoint">Endpoint</Label></SettingsRowTitle>
            <SettingsRowMeta>HTTPS, or HTTP on a loopback host.</SettingsRowMeta>
          </SettingsRowMain>
          <SettingsRowActions>
            <Input
              id="provider-endpoint"
              className="w-64"
              value={draft.endpoint}
              disabled={isPending}
              placeholder="https://gateway.example.com/v1"
              onChange={(event) => setDraft({ ...draft, endpoint: event.currentTarget.value })}
              onBlur={() => discoverWith(selected?.id, connection)}
            />
          </SettingsRowActions>
        </SettingsRow>

        <SettingsRow>
          <SettingsRowMain>
            <SettingsRowTitle><Label htmlFor="provider-secret">Secret</Label></SettingsRowTitle>
            <SettingsRowMeta>Credentials stay write-only in Keychain.</SettingsRowMeta>
          </SettingsRowMain>
          <SettingsRowActions>
            <Select
              value={draft.credentialRef ?? "__none__"}
              disabled={isPending}
              onValueChange={(value) => {
                if (!value) return
                const credentialRef = value === "__none__" ? null : value
                setDraft({ ...draft, credentialRef })
                discoverWith(selected?.id, {
                  type: draft.type,
                  endpoint: draft.endpoint,
                  credentialRef,
                })
              }}
            >
              <SelectTrigger id="provider-secret" className="w-64" aria-label="Provider secret">
                <SelectValue>{selectedSecretLabel}</SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="__none__">Select secret</SelectItem>
                  {projection.secrets.map((secret) => (
                    <SelectItem key={secret.secretRef} value={secret.secretRef}>
                      {secret.label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </SettingsRowActions>
        </SettingsRow>

        <SettingsGroup label="Models">
          <div className="flex items-center justify-between gap-3">
            <p className="text-sm text-muted-foreground">
              Models are discovered automatically once Type, Endpoint, and Secret
              are set. Refresh to reload the list.
            </p>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={isPending || discovering || !connectionComplete(connection)}
              onClick={() => discoverWith(selected?.id, connection)}
            >
              {discovering ? <Spinner /> : <RefreshCwIcon data-icon="inline-start" />}
              Refresh
            </Button>
          </div>
          <ScrollArea
            data-testid="provider-model-list"
            className="max-h-56"
            viewportClassName="max-h-56"
          >
            {/* py-4 keeps the first and last models clear of the 1rem scroll
                fade at rest. pr-6 keeps a long name clear of the overlay
                scrollbar. */}
            <div className="flex flex-col gap-2 py-4 pr-6">
              {(discoveredModels.length > 0
                ? discoveredModels
                : draft.allowedModels
              ).map((model) => {
                const id = `provider-model-${model}`
                return (
                  <div key={model} className="flex items-center gap-2">
                    <Checkbox
                      id={id}
                      checked={draft.allowedModels.includes(model)}
                      disabled={isPending}
                      onCheckedChange={(checked) => {
                        const allowedModels =
                          checked === true
                            ? Array.from(new Set([...draft.allowedModels, model]))
                            : draft.allowedModels.filter(
                                (candidate) => candidate !== model,
                              )
                        setDraft({
                          ...draft,
                          allowedModels,
                        })
                      }}
                    />
                    <Label htmlFor={id} className="font-normal">
                      {model}
                    </Label>
                  </div>
                )
              })}
            </div>
          </ScrollArea>
        </SettingsGroup>
      </SettingsList>

      {/* Creating always offers its way out; configuring shows actions only
          once there is something to save. */}
      {isDraft || isDirty || isPending ? (
        <SettingsDetailsActionBar sticky={!isDraft}>
          {isPending ? <Spinner /> : null}
          <Button
            type="button"
            variant="ghost"
            disabled={isPending}
            onClick={() => {
              if (isDraft) {
                onSelectionChange(undefined)
                return
              }
              setDraft(savedDraft)
              setEditingName(false)
            }}
          >
            Cancel
          </Button>
          <Button type="button" disabled={isPending || !canSave} onClick={save}>
            {isDraft ? "Add" : "Save"}
          </Button>
        </SettingsDetailsActionBar>
      ) : null}

      <UnsavedChangesDialog open={guard.isOpen} onConfirm={guard.confirm} onCancel={guard.cancel} />

      <AlertDialog open={confirmImpact} onOpenChange={setConfirmImpact}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Save provider changes?</AlertDialogTitle>
            <AlertDialogDescription>
              {conflictedEnvironments.length} linked {conflictedEnvironments.length === 1 ? "Environment" : "Environments"}
              {conflictedEnvironments.length > 0
                ? ` (${conflictedEnvironments.map((environment) => environment.name).join(", ")})`
                : ""} will need changes before agents can run.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={persist}>Save and review conflicts</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this provider?</AlertDialogTitle>
            <AlertDialogDescription>
              {linkedEnvironments.length > 0
                ? `${linkedEnvironments.map((environment) => environment.name).join(", ")} will be unlinked and need setup. `
                : ""}
              The underlying Secret is retained.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep Provider</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (!selected) return
                dispatch({ type: "llmProvider.delete", providerId: selected.id }, () =>
                  onSelectionChange(undefined),
                )
              }}
            >
              Delete Provider
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

function providerTypeLabel(type: LlmProviderType) {
  if (type === "litellm") return "LiteLLM"
  if (type === "openai") return "OpenAI"
  if (type === "ollama") return "Ollama"
  return "Meta"
}

function draftOf(provider: LlmProviderDto): LlmProviderConfigurationDto {
  return {
    name: provider.name,
    type: provider.type,
    endpoint: provider.endpoint,
    credentialRef: provider.credentialRef ?? null,
    allowedModels: [...provider.allowedModels],
  }
}

function sameConfiguration(
  left: LlmProviderConfigurationDto,
  right: LlmProviderConfigurationDto,
) {
  return (
    left.name === right.name &&
    left.type === right.type &&
    left.endpoint === right.endpoint &&
    (left.credentialRef ?? null) === (right.credentialRef ?? null) &&
    sameStringCollection(left.allowedModels, right.allowedModels)
  )
}

function sameStringCollection(left: readonly string[], right: readonly string[]) {
  if (left.length !== right.length) return false
  const sortedLeft = left.toSorted()
  const sortedRight = right.toSorted()
  return sortedLeft.every((value, index) => value === sortedRight[index])
}

function environmentPolicyConflicts(
  policy: WorkspaceProjection["environments"][number]["llm"],
  provider: LlmProviderConfigurationDto,
) {
  if (!policy) return false
  return (
    policy.allowedModels.some((model) => !provider.allowedModels.includes(model)) ||
    !policy.allowedModels.includes(policy.defaultModel)
  )
}
