import * as React from "react"

import type {
  SecretMetadataDto,
  EnvironmentVariableDto,
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
import { Card, CardContent } from "@agent-factory/ui/components/card"
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
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-factory/ui/components/dropdown-menu"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@agent-factory/ui/components/empty"
import { Field, FieldLabel } from "@agent-factory/ui/components/field"
import { Input } from "@agent-factory/ui/components/input"
import { Switch } from "@agent-factory/ui/components/switch"
import { MoreHorizontalIcon, PlusIcon, VariableIcon } from "lucide-react"

type DraftEnvironment = EnvironmentVariableDto

export function EnvironmentVariableSettings({
  environment,
  secrets,
  onChange,
  disabled = false,
  issues = [],
}: {
  environment: readonly DraftEnvironment[]
  secrets: readonly SecretMetadataDto[]
  onChange: (environment: readonly DraftEnvironment[]) => void
  disabled?: boolean
  issues?: readonly string[]
}) {
  const [editingIndex, setEditingIndex] = React.useState<number | null>(null)
  /// The entry as it stood when editing began. Absent for a row that was just
  /// created, which Cancel drops instead of restoring.
  const [editingOriginal, setEditingOriginal] =
    React.useState<DraftEnvironment | null>(null)
  const [confirmRemoveIndex, setConfirmRemoveIndex] = React.useState<number | null>(null)
  const [secretPickerIndex, setSecretPickerIndex] = React.useState<number | null>(null)

  const create = () => {
    const next = [
      ...environment,
      { name: "", source: "literal" as const, value: "" },
    ]
    onChange(next)
    setEditingOriginal(null)
    setEditingIndex(next.length - 1)
  }

  const beginEdit = (index: number) => {
    setEditingOriginal(environment[index] ?? null)
    setEditingIndex(index)
  }

  const update = (index: number, patch: Partial<DraftEnvironment>) => {
    onChange(
      environment.map((entry, entryIndex) =>
        entryIndex === index ? { ...entry, ...patch } : entry,
      ),
    )
  }

  const commitEdit = (index: number) => {
    const entry = environment[index]
    if (!entry) return
    if (entry.name.trim() === "" && entry.value.trim() === "") {
      onChange(environment.filter((_, entryIndex) => entryIndex !== index))
    }
    setEditingOriginal(null)
    setEditingIndex(null)
  }

  const cancelEdit = (index: number) => {
    const original = editingOriginal
    onChange(
      original
        ? environment.map((entry, entryIndex) =>
            entryIndex === index ? original : entry,
          )
        : environment.filter((_, entryIndex) => entryIndex !== index),
    )
    setEditingOriginal(null)
    setEditingIndex(null)
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-start justify-between gap-3">
        <div className="flex flex-col gap-1">
          <p className="font-medium">Environment Variables</p>
          <p className="text-xs text-muted-foreground">
            Variables passed to agent sessions for this Environment. Use a secret to
            avoid embedding sensitive values.
          </p>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled}
          aria-label="Add environment variable"
          onClick={create}
        >
          <PlusIcon className="size-3.5" />
          Add
        </Button>
      </div>

      <Card>
        <CardContent className="flex flex-col gap-2 py-2">
          {issues.length > 0 ? (
            <Alert>
              <AlertTitle>Environment needs setup</AlertTitle>
              <AlertDescription>{issues.join(" ")}</AlertDescription>
            </Alert>
          ) : null}

          {environment.length === 0 ? (
            <Empty className="border border-dashed">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <VariableIcon />
                </EmptyMedia>
                <EmptyTitle>No environment variables</EmptyTitle>
                <EmptyDescription>
                  Add a variable to configure agent sessions for this Environment.
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <div className="flex flex-col divide-y">
              {environment.map((entry, index) =>
                editingIndex === index ? (
                  <EnvironmentVariableEditRow
                    key={`edit-${index}`}
                    index={index}
                    entry={entry}
                    secrets={secrets}
                    disabled={disabled}
                    isNew={editingOriginal === null}
                    onChange={(patch) => update(index, patch)}
                    onCommit={() => commitEdit(index)}
                    onCancel={() => cancelEdit(index)}
                    onOpenSecretPicker={() => setSecretPickerIndex(index)}
                  />
                ) : (
                  <EnvironmentVariableViewRow
                    key={`view-${index}`}
                    entry={entry}
                    secrets={secrets}
                    disabled={disabled}
                    onEdit={() => beginEdit(index)}
                    onRemove={() => setConfirmRemoveIndex(index)}
                  />
                ),
              )}
            </div>
          )}
        </CardContent>
      </Card>

      <SecretPicker
        open={secretPickerIndex !== null}
        secrets={secrets}
        onOpenChange={(open) => !open && setSecretPickerIndex(null)}
        onChoose={(secretRef) => {
          if (secretPickerIndex !== null) {
            update(secretPickerIndex, { value: secretRef ?? "" })
          }
          setSecretPickerIndex(null)
        }}
      />

      <AlertDialog
        open={confirmRemoveIndex !== null}
        onOpenChange={(open) => !open && setConfirmRemoveIndex(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove environment variable?</AlertDialogTitle>
            <AlertDialogDescription>
              {confirmRemoveIndex !== null
                ? (environment[confirmRemoveIndex]?.name ||
                  `Variable ${confirmRemoveIndex + 1}`)
                : ""}{" "}
              will no longer be applied to Harness sessions for this Environment.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (confirmRemoveIndex === null) return
                onChange(
                  environment.filter(
                    (_, entryIndex) => entryIndex !== confirmRemoveIndex,
                  ),
                )
                if (editingIndex === confirmRemoveIndex) setEditingIndex(null)
                setConfirmRemoveIndex(null)
              }}
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

function EnvironmentVariableViewRow({
  entry,
  secrets,
  disabled,
  onEdit,
  onRemove,
}: {
  entry: DraftEnvironment
  secrets: readonly SecretMetadataDto[]
  disabled: boolean
  onEdit: () => void
  onRemove: () => void
}) {
  const isSecret = entry.source === "secret"
  const selectedSecret = secrets.find(
    (secret) => secret.secretRef === entry.value,
  )
  const valueLabel = isSecret
    ? (selectedSecret?.label ?? "Secret unavailable")
    : entry.value

  return (
    <div className="flex items-center justify-between gap-4 py-2 first:pt-0 last:pb-0">
      <div className="flex min-w-0 flex-col gap-0.5">
        <div className="flex items-center gap-2">
          <span className="font-mono text-sm">{entry.name}</span>
          {isSecret ? (
            <Badge variant="secondary" className="text-[10px]">
              Secret
            </Badge>
          ) : null}
        </div>
        <p className="text-xs text-muted-foreground">
          {valueLabel || "No value"}
        </p>
      </div>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={(
            <Button
              variant="ghost"
              size="icon-sm"
              disabled={disabled}
              aria-label={`Actions for ${entry.name || "unnamed variable"}`}
            />
          )}
        >
          <MoreHorizontalIcon className="size-4" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={onEdit}>Edit</DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onClick={onRemove}>
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}

function EnvironmentVariableEditRow({
  index,
  entry,
  secrets,
  disabled,
  isNew,
  onChange,
  onCommit,
  onCancel,
  onOpenSecretPicker,
}: {
  index: number
  entry: DraftEnvironment
  secrets: readonly SecretMetadataDto[]
  disabled: boolean
  /// A row that has never been committed, so the confirm action reads "Add".
  isNew: boolean
  onChange: (patch: Partial<DraftEnvironment>) => void
  onCommit: () => void
  onCancel: () => void
  onOpenSecretPicker: () => void
}) {
  const isSecret = entry.source === "secret"
  const selectedSecret = secrets.find(
    (secret) => secret.secretRef === entry.value,
  )
  const secretLabel = !entry.value
    ? "Select secret"
    : (selectedSecret?.label ?? "Secret unavailable")

  // Blur no longer commits: Tab has to reach the Value field, and the explicit
  // actions below are what end the edit.
  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Enter") {
      event.preventDefault()
      onCommit()
    } else if (event.key === "Escape") {
      event.preventDefault()
      onCancel()
    }
  }

  return (
    // Every control shares one horizontal axis, at every width.
    <div className="flex flex-wrap items-end gap-2 py-2 first:pt-0 last:pb-0">
      <Field className="w-auto gap-1">
        <FieldLabel htmlFor={`env-secret-${index}`}>Secret</FieldLabel>
        <div className="flex h-7 items-center">
          <Switch
            id={`env-secret-${index}`}
            checked={isSecret}
            disabled={disabled}
            onCheckedChange={(checked) => {
              onChange({
                source: checked ? "secret" : "literal",
                value: checked ? (secrets[0]?.secretRef ?? "") : "",
              })
            }}
          />
        </div>
      </Field>
      <Field className="min-w-[8rem] flex-1 gap-1">
        <FieldLabel htmlFor={`env-name-${index}`}>Key</FieldLabel>
        <Input
          id={`env-name-${index}`}
          value={entry.name}
          placeholder="CUSTOM_VARIABLE"
          disabled={disabled}
          onChange={(event) => onChange({ name: event.currentTarget.value })}
          onKeyDown={handleKeyDown}
        />
      </Field>
      <Field className="min-w-[8rem] flex-1 gap-1">
        <FieldLabel htmlFor={`env-value-${index}`}>Value</FieldLabel>
        {isSecret ? (
          <Button
            id={`env-value-${index}`}
            type="button"
            variant="outline"
            className="w-full justify-between font-normal"
            disabled={disabled}
            aria-label="Secret value"
            onClick={onOpenSecretPicker}
          >
            {secretLabel}
          </Button>
        ) : (
          <Input
            id={`env-value-${index}`}
            value={entry.value}
            type="text"
            placeholder="http://localhost:11434"
            disabled={disabled}
            onChange={(event) => onChange({ value: event.currentTarget.value })}
            onKeyDown={handleKeyDown}
          />
        )}
      </Field>
      {/* Accessible names stay specific: the section header and the Environment's own
          action bar carry an "Add" and a "Cancel" of their own. */}
      <div className="flex h-7 items-center gap-2">
        <Button
          type="button"
          disabled={disabled}
          aria-label={isNew ? "Add variable" : "Save variable"}
          onClick={onCommit}
        >
          {isNew ? "Add" : "Save"}
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={disabled}
          aria-label={isNew ? "Cancel adding variable" : "Cancel editing variable"}
          onClick={onCancel}
        >
          Cancel
        </Button>
      </div>
    </div>
  )
}

function SecretPicker({
  open,
  secrets,
  onOpenChange,
  onChoose,
}: {
  open: boolean
  secrets: readonly SecretMetadataDto[]
  onOpenChange: (open: boolean) => void
  onChoose: (secretRef?: string) => void
}) {
  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Choose secret"
      description="Raw secret material is never displayed."
    >
      <Command>
        <CommandInput placeholder="Search secrets" />
        <CommandList>
          <CommandEmpty>No secrets available.</CommandEmpty>
          <CommandGroup heading="Secrets">
            {secrets.map((secret) => (
              <CommandItem
                key={secret.secretRef}
                value={secret.label}
                onSelect={() => onChoose(secret.secretRef)}
              >
                <span>{secret.label}</span>
              </CommandItem>
            ))}
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  )
}
