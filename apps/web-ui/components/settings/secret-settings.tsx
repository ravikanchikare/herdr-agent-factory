import * as React from "react"

import type {
  SecretMetadataDto,
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
import { Badge } from "@agent-factory/ui/components/badge"
import { Button } from "@agent-factory/ui/components/button"
import {
  Field,
  FieldLabel,
} from "@agent-factory/ui/components/field"
import { Input } from "@agent-factory/ui/components/input"
import { Spinner } from "@agent-factory/ui/components/spinner"
import { cn } from "@agent-factory/ui/lib/utils"
import {
  EyeIcon,
  EyeOffIcon,
  KeyRoundIcon,
  XIcon,
} from "lucide-react"

import type { EmitIntent } from "@/components/shell/workspace-shell"
import {
  SettingsPrimaryAction,
  SettingsDetailsActionBar,
  SettingsDetailsNavigation,
  SettingsEmpty,
  SettingsList,
  SettingsRow,
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

export type SecretSettingsSelection = { kind: "draft" }

type EditableSecretEntry =
  | {
      kind: "existing"
      secretRef: string
      label: string
      value: string
      referencedBy: SecretMetadataDto["referencedBy"]
    }
  | { kind: "new"; label: string; value: string }

function SecretValueInput({
  id,
  value,
  required = false,
  maxLength,
  disabled,
  autoComplete,
  className,
  inputProps,
  onChange,
}: {
  id: string
  value: string
  required?: boolean
  maxLength?: number
  disabled?: boolean
  autoComplete?: string
  className?: string
  inputProps?: React.InputHTMLAttributes<HTMLInputElement>
  onChange: (value: string) => void
}) {
  const [visible, setVisible] = React.useState(false)

  return (
    <div className="relative flex items-center">
      <Input
        id={id}
        type={visible ? "text" : "password"}
        value={value}
        required={required}
        maxLength={maxLength}
        disabled={disabled}
        autoComplete={autoComplete}
        className={cn("pr-8", className)}
        {...inputProps}
        onChange={(event) => onChange(event.currentTarget.value)}
      />
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        className="absolute right-0"
        aria-label={visible ? "Hide value" : "Show value"}
        title={visible ? "Hide value" : "Show value"}
        disabled={disabled}
        onClick={() => setVisible((current) => !current)}
      >
        {visible ? <EyeOffIcon size={14} /> : <EyeIcon size={14} />}
      </Button>
    </div>
  )
}

export function SecretSettings({
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
  selection?: SecretSettingsSelection
  onSelectionChange: (selection?: SecretSettingsSelection) => void
}) {
  const [entries, setEntries] = React.useState<EditableSecretEntry[]>([
    { kind: "new", label: "", value: "" },
  ])
  const [confirmDeleteRef, setConfirmDeleteRef] = React.useState<
    string | undefined
  >()
  const [pendingDeletes, setPendingDeletes] = React.useState<Set<string>>(
    new Set(),
  )
  const [isPending, startTransition] = React.useTransition()

  const isEditing = selection?.kind === "draft"
  const hasUnsavedChanges =
    isEditing && isDirty(entries, projection.secrets)

  useSettingsErrorToast("Secret operation failed", projection.secretsError)

  const guard = useUnsavedChangesGuard(hasUnsavedChanges)

  React.useLayoutEffect(() => {
    onDirtyChange?.(hasUnsavedChanges)
  }, [hasUnsavedChanges, onDirtyChange])

  const startEditing = React.useCallback(() => {
    onSelectionChange({ kind: "draft" })
    setEntries(buildEditEntries(projection.secrets))
  }, [onSelectionChange, projection.secrets])

  const didHandleCreateRequest = React.useRef(false)
  React.useEffect(() => {
    if (!createRequested) {
      didHandleCreateRequest.current = false
      return
    }
    if (didHandleCreateRequest.current) return
    didHandleCreateRequest.current = true
    onCreateRequestHandled()
    guard.request(startEditing)
  }, [createRequested, onCreateRequestHandled, guard, startEditing])

  const wasEditing = React.useRef(false)
  React.useLayoutEffect(() => {
    if (isEditing && !wasEditing.current) {
      setEntries(buildEditEntries(projection.secrets))
    }
    wasEditing.current = isEditing
  }, [isEditing, projection.secrets])

  const updateEntry = (index: number, patch: Partial<EditableSecretEntry>) => {
    setEntries((current) => {
      const next = current.map((entry, entryIndex) =>
        entryIndex === index ? { ...entry, ...patch } : entry,
      ) as EditableSecretEntry[]
      const last = next[next.length - 1]
      if (
        last?.kind === "new" &&
        (last.label !== "" || last.value !== "")
      ) {
        return [...next, { kind: "new", label: "", value: "" }]
      }
      return next
    })
  }

  const removeEntry = (index: number) => {
    setEntries((current) => {
      const next = current.filter((_, entryIndex) => entryIndex !== index)
      return next.length > 0 ? next : [{ kind: "new", label: "", value: "" }]
    })
  }


  const cancel = () => {
    guard.request(() => {
      onSelectionChange(undefined)
      setEntries([{ kind: "new", label: "", value: "" }])
    })
  }

  const save = () => {
    const creates = entries.filter(
      (entry): entry is Extract<EditableSecretEntry, { kind: "new" }> =>
        entry.kind === "new" &&
        entry.label.trim().length > 0 &&
        entry.value.length > 0,
    )
    const replacements = entries.filter(
      (
        entry,
      ): entry is Extract<EditableSecretEntry, { kind: "existing" }> =>
        entry.kind === "existing" && entry.value.length > 0,
    )
    if (
      creates.length === 0 &&
      replacements.length === 0 &&
      pendingDeletes.size === 0
    ) {
      return
    }

    startTransition(async () => {
      for (const entry of creates) {
        await emitIntent({
          type: "secret.create",
          label: entry.label,
          value: entry.value,
        })
      }
      for (const entry of replacements) {
        await emitIntent({
          type: "secret.replace",
          secretRef: entry.secretRef,
          value: entry.value,
        })
      }
      for (const secretRef of pendingDeletes) {
        await emitIntent({ type: "secret.delete", secretRef })
      }
      onSelectionChange(undefined)
      setEntries([{ kind: "new", label: "", value: "" }])
      setPendingDeletes(new Set())
    })
  }

  const canSave =
    entries.some(
      (entry) =>
        (entry.kind === "new" &&
          entry.label.trim().length > 0 &&
          entry.value.length > 0) ||
        (entry.kind === "existing" && entry.value.length > 0),
    ) || pendingDeletes.size > 0

  return (
    <>
      {isEditing ? (
        <div className="flex min-h-0 flex-1 flex-col gap-6">
          <SettingsDetailsNavigation
            parent="Secrets"
            current="Edit Secrets"
            onBack={cancel}
          />
          <h2 className="text-lg font-semibold tracking-tight">
            Edit Secrets
          </h2>
          <div className="flex flex-1 flex-col gap-4">
            <p className="text-xs text-muted-foreground">
              Values are write-only. Leaving an existing secret&apos;s value
              empty keeps the current value. Removing an unused secret deletes
              it.
            </p>
            <SettingsList className="gap-2">
              {entries.map((entry, index) => {
                const labelId = `secret-label-${index}`
                const valueId = `secret-value-${index}`
                const isReferenced =
                  entry.kind === "existing" &&
                  entry.referencedBy.length > 0
                return (
                  <div
                    key={
                      entry.kind === "existing"
                        ? entry.secretRef
                        : `new-${index}`
                    }
                    className={cn(
                      "flex flex-wrap items-end gap-2",
                      "py-2 first:pt-0 last:pb-0",
                    )}
                  >
                    <Field className="min-w-[8rem] flex-1 gap-1">
                      <FieldLabel htmlFor={labelId}>Key</FieldLabel>
                      <Input
                        id={labelId}
                        value={entry.label}
                        maxLength={128}
                        disabled={isPending || entry.kind === "existing"}
                        autoComplete="off"
                        placeholder="OLLAMA_API_KEY"
                        aria-label={`Key ${index + 1}`}
                        onChange={(event) =>
                          updateEntry(index, { label: event.currentTarget.value })
                        }
                      />
                    </Field>
                    <Field className="min-w-[8rem] flex-1 gap-1">
                      <FieldLabel htmlFor={valueId}>Value</FieldLabel>
                      <SecretValueInput
                        id={valueId}
                        value={entry.value}
                        maxLength={65_536}
                        disabled={isPending}
                        autoComplete="new-password"
                        inputProps={{ "aria-label": `Value ${index + 1}` }}
                        onChange={(value) => updateEntry(index, { value })}
                      />
                    </Field>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      className="mb-0.5"
                      aria-label={`Remove row ${index + 1}`}
                      disabled={isPending || isReferenced}
                      onClick={() =>
                        entry.kind === "existing"
                          ? setConfirmDeleteRef(entry.secretRef)
                          : removeEntry(index)
                      }
                    >
                      <XIcon className="size-4" />
                    </Button>
                  </div>
                )
              })}
            </SettingsList>
          </div>
          <SettingsDetailsActionBar sticky={false}>
            {isPending ? <Spinner /> : null}
            <Button
              type="button"
              variant="ghost"
              disabled={isPending}
              onClick={cancel}
            >
              Cancel
            </Button>
            <Button
              type="button"
              disabled={isPending || !canSave}
              onClick={save}
            >
              Save
            </Button>
          </SettingsDetailsActionBar>
        </div>
      ) : (
        <SettingsSection
          title="Stored secrets"
          description="Write-only: saving returns an opaque reference, never the value."
        >
          <SettingsList>
            {projection.secrets.length === 0 ? (
              <SettingsEmpty
                icon={<KeyRoundIcon />}
                title="No secrets saved"
                description="Add a secret so Environments can reference sensitive values without embedding them in configuration."
                action={
                  <SettingsPrimaryAction
                    label="Add"
                    onClick={() => guard.request(startEditing)}
                  />
                }
              />
            ) : (
              projection.secrets.map((secret) => (
                <SettingsRow
                  key={secret.secretRef}
                  icon={<KeyRoundIcon />}
                  onOpen={startEditing}
                  hoverable
                >
                  <SettingsRowMain openLabel={`Edit ${secret.label}`}>
                    <div className="flex items-center gap-2">
                      <SettingsRowTitle className="font-mono">
                        {secret.label}
                      </SettingsRowTitle>
                      <Badge variant="secondary">
                        {kindLabel(secret.kind)}
                      </Badge>
                    </div>
                    <SettingsRowMeta>{usageText(secret)}</SettingsRowMeta>
                    <SettingsRowMeta>
                      Updated{" "}
                      {new Date(secret.updatedAtUnixMs).toLocaleString()}
                    </SettingsRowMeta>
                  </SettingsRowMain>
                </SettingsRow>
              ))
            )}
          </SettingsList>
        </SettingsSection>
      )}

      <UnsavedChangesDialog
        open={guard.isOpen}
        onConfirm={guard.confirm}
        onCancel={guard.cancel}
        description="Your unsaved secret changes will be lost."
      />

      <AlertDialog
        open={confirmDeleteRef !== undefined}
        onOpenChange={(open) => {
          if (!open) setConfirmDeleteRef(undefined)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this secret?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently delete the secret. Environments
              referencing it will stop working.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => setConfirmDeleteRef(undefined)}>
              Keep
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={isPending}
              onClick={() => {
                const secretRef = confirmDeleteRef
                if (!secretRef) return
                setPendingDeletes((current) => {
                  const next = new Set(current)
                  next.add(secretRef)
                  return next
                })
                setEntries((current) =>
                  current.filter(
                    (entry) =>
                      entry.kind !== "existing" || entry.secretRef !== secretRef,
                  ),
                )
                setConfirmDeleteRef(undefined)
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function buildEditEntries(
  secrets: WorkspaceProjection["secrets"],
): EditableSecretEntry[] {
  return [
    ...secrets.map((secret) => ({
      kind: "existing" as const,
      secretRef: secret.secretRef,
      label: secret.label,
      value: "",
      referencedBy: secret.referencedBy,
    })),
    { kind: "new" as const, label: "", value: "" },
  ]
}

function isDirty(
  entries: EditableSecretEntry[],
  secrets: WorkspaceProjection["secrets"],
): boolean {
  const existingByRef = new Map(
    secrets.map((secret) => [secret.secretRef, secret]),
  )
  return entries.some((entry) => {
    if (entry.kind === "new") {
      return entry.label.trim().length > 0 || entry.value.length > 0
    }
    const original = existingByRef.get(entry.secretRef)
    if (!original) return true
    return entry.label !== original.label || entry.value.length > 0
  })
}

function kindLabel(kind: SecretMetadataDto["kind"]) {
  if (kind === "api_token") return "API token"
  return kind
}

function usageText(secret: SecretMetadataDto) {
  if (secret.referencedBy.length === 0) return "Not used by any Environment"
  const parts = secret.referencedBy.map(
    (reference) =>
      `${reference.environmentName} / ${
        reference.kind === "llm_provider"
          ? "Intelligence Provider"
          : `Environment variable ${reference.label}`
      }`,
  )
  const summary =
    secret.referencedBy.length === 1
      ? parts[0]
      : `${secret.referencedBy.length} Environments`
  return `Used by ${summary}`
}
