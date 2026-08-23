import * as React from "react"
import {
  FolderOpenIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react"

import type { EnvironmentDto, RuntimeIntent } from "@agent-factory/runtime-client"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@agent-factory/ui/components/alert"
import { Button } from "@agent-factory/ui/components/button"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@agent-factory/ui/components/field"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
  InputGroupTextarea,
} from "@agent-factory/ui/components/input-group"
import { Input } from "@agent-factory/ui/components/input"
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
import { Spinner } from "@agent-factory/ui/components/spinner"
import { Checkbox } from "@agent-factory/ui/components/checkbox"
import { Textarea } from "@agent-factory/ui/components/textarea"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-factory/ui/components/tooltip"
import { cn } from "@agent-factory/ui/lib/utils"

import {
  SettingsDetailsActionBar,
  SettingsList,
  SettingsRow,
  SettingsRowActions,
  SettingsRowMain,
  SettingsRowMeta,
  SettingsRowTitle,
} from "@/components/settings/settings-primitives"

type CreateTargetAgent = (
  intent: Extract<RuntimeIntent, { type: "targetAgent.create" }>,
) => Promise<boolean>

export type WorkCreationDraft = { kind: "agent" }

export type AgentDefinitionFormValues = {
  name: string
  objective: string
  draftName: string
  criteria: string[]
  root: string
  trusted: boolean
  environmentId?: string
}

export type AgentDefinitionEditConfig = {
  initial: AgentDefinitionFormValues
  environments: readonly EnvironmentDto[]
  onSave: (values: AgentDefinitionFormValues) => Promise<boolean>
}

/** Keep exactly one trailing empty criterion so the next row is always ready. */
export function withTrailingEmptyCriterion(
  criteria: readonly string[],
): string[] {
  const next = [...criteria]
  while (
    next.length > 1 &&
    next[next.length - 1] === "" &&
    next[next.length - 2] === ""
  ) {
    next.pop()
  }
  if (next.length === 0 || next[next.length - 1] !== "") {
    next.push("")
  }
  return next
}

export function WorkCreationWorkspace({
  createTargetAgent,
  runtimeError,
  sidebarOpen,
  onClose,
  edit,
}: {
  createTargetAgent?: CreateTargetAgent
  runtimeError?: string
  sidebarOpen: boolean
  onClose: () => void
  /** When set, the form edits an existing Draft instead of creating an Agent. */
  edit?: AgentDefinitionEditConfig
}) {
  const isEdit = Boolean(edit)
  const [name, setName] = React.useState(edit?.initial.name ?? "")
  const [objective, setObjective] = React.useState(edit?.initial.objective ?? "")
  const [draftName, setDraftName] = React.useState(edit?.initial.draftName ?? "main")
  const [criteria, setCriteria] = React.useState<string[]>(() =>
    withTrailingEmptyCriterion(edit?.initial.criteria ?? [""]),
  )
  const [root, setRoot] = React.useState(edit?.initial.root ?? "")
  const [trusted, setTrusted] = React.useState(edit?.initial.trusted ?? true)
  const readyEnvironments = (edit?.environments ?? []).filter(
    (environment) => environment.readiness.state === "ready",
  )
  const [environmentId, setEnvironmentId] = React.useState(
    edit?.initial.environmentId ?? readyEnvironments[0]?.id ?? "",
  )
  const [isPending, startTransition] = React.useTransition()
  const [isPicking, startPicking] = React.useTransition()
  const [pickerError, setPickerError] = React.useState<string>()
  const [submitted, setSubmitted] = React.useState(false)

  const normalizedCriteria = criteria
    .map((criterion) => criterion.trim())
    .filter(Boolean)
  const canSubmit = Boolean(
    name.trim() &&
      objective.trim() &&
      draftName.trim() &&
      (isEdit || root.trim()) &&
      normalizedCriteria.length > 0 &&
      (!isEdit || environmentId || readyEnvironments.length === 0),
  )
  const submit = () => {
    if (!canSubmit) return
    setSubmitted(true)
    startTransition(async () => {
      const values: AgentDefinitionFormValues = {
        name: name.trim(),
        objective: objective.trim(),
        draftName: draftName.trim(),
        criteria: normalizedCriteria,
        root: root.trim(),
        trusted,
        environmentId: environmentId || undefined,
      }
      if (edit) {
        const saved = await edit.onSave(values)
        if (saved) onClose()
        return
      }
      if (!createTargetAgent) return
      const created = await createTargetAgent({
        type: "targetAgent.create",
        name: values.name,
        objective: values.objective,
        acceptanceCriteria: values.criteria,
        repositoryRoot: values.root,
        draftName: values.draftName,
        trusted: values.trusted,
      })
      if (created) onClose()
    })
  }

  const chooseFolder = () => {
    const bridge = window.zero
    if (!bridge) return
    setPickerError(undefined)
    startPicking(async () => {
      try {
        const selected = await bridge.invoke("native-sdk.dialog.openFile", {
          title: "Choose agent workspace folder",
          defaultPath: root.trim() || undefined,
          allowDirectories: true,
          allowMultiple: false,
        })
        const selectedRoot = selectedFolder(selected)
        if (!selectedRoot) return
        setRoot(selectedRoot)
        if (!name.trim()) {
          setName(selectedRoot.split(/[/\\]/).filter(Boolean).at(-1) ?? "")
        }
      } catch {
        setPickerError(
          "The native folder picker is unavailable. Enter a path manually.",
        )
      }
    })
  }

  return (
    <section
      aria-label="Define your agent"
      className="flex size-full min-h-0 flex-col"
    >
      <header
        data-native-drag-region
        className={cn(
          "flex h-11 shrink-0 items-center gap-2 transition-[padding] duration-200 ease-linear",
          sidebarOpen ? "px-2" : "pl-32 pr-2",
        )}
      >
        <div data-native-no-drag className="ml-auto flex shrink-0 items-center">
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Close"
                  disabled={isPending}
                  onClick={onClose}
                />
              }
            >
              <XIcon />
            </TooltipTrigger>
            <TooltipContent>Close</TooltipContent>
          </Tooltip>
        </div>
      </header>
      <Separator />
      <ScrollArea className="min-h-0 flex-1">
        <div className="mx-auto flex w-full max-w-4xl flex-col gap-6 p-6">
          <div className="flex flex-col gap-1">
            <h2 className="text-lg font-semibold tracking-tight">
              {isEdit ? "Edit draft" : "Define your agent"}
            </h2>
            <p className="text-muted-foreground text-sm">
              {isEdit
                ? "Update Draft configuration before the first Run is created."
                : "Save the initial draft, then configure an Environment before starting a Run."}
            </p>
          </div>
          <form
            className="flex flex-col gap-6"
            onSubmit={(event) => {
              event.preventDefault()
              submit()
            }}
          >
            {submitted && runtimeError ? (
              <Alert variant="destructive">
                <TriangleAlertIcon />
                <AlertTitle>
                  {isEdit
                    ? "Could not update draft"
                    : "Could not save agent draft"}
                </AlertTitle>
                <AlertDescription>{runtimeError}</AlertDescription>
              </Alert>
            ) : null}
            <SettingsList>
              <SettingsRow className="items-start gap-4">
                <SettingsRowMain className="w-[30%] min-w-0 flex-none">
                  <SettingsRowTitle>
                    <FieldLabel htmlFor="agent-name">Name</FieldLabel>
                  </SettingsRowTitle>
                  <SettingsRowMeta className="whitespace-normal">
                    The primary identity shown throughout Agent Factory.
                  </SettingsRowMeta>
                </SettingsRowMain>
                <SettingsRowActions className="w-[70%] min-w-0 flex-none items-stretch pr-4">
                  <Input
                    id="agent-name"
                    className="w-full"
                    value={name}
                    autoComplete="off"
                    placeholder="Commerce Copilot"
                    onChange={(event) => setName(event.target.value)}
                  />
                </SettingsRowActions>
              </SettingsRow>
              <SettingsRow className="items-start gap-4">
                <SettingsRowMain className="w-[30%] min-w-0 flex-none">
                  <SettingsRowTitle>
                    <FieldLabel htmlFor="agent-objective">Objective</FieldLabel>
                  </SettingsRowTitle>
                  <SettingsRowMeta className="whitespace-normal">
                    Keep the outcome specific enough for a Run to implement and
                    evaluate.
                  </SettingsRowMeta>
                </SettingsRowMain>
                <SettingsRowActions className="w-[70%] min-w-0 flex-none items-stretch pr-4">
                  <Textarea
                    id="agent-objective"
                    className="min-h-16 w-full"
                    value={objective}
                    placeholder="What should this Agent accomplish?"
                    onChange={(event) => setObjective(event.target.value)}
                  />
                </SettingsRowActions>
              </SettingsRow>
              <SettingsRow className="items-start gap-4">
                <SettingsRowMain className="w-[30%] min-w-0 flex-none">
                  <SettingsRowTitle>
                    <FieldLabel htmlFor="agent-draft-name">
                      Draft name
                    </FieldLabel>
                  </SettingsRowTitle>
                  <SettingsRowMeta className="whitespace-normal">
                    Names the first Git-backed worktree for this Agent.
                  </SettingsRowMeta>
                </SettingsRowMain>
                <SettingsRowActions className="w-[70%] min-w-0 flex-none items-stretch pr-4">
                  <Input
                    id="agent-draft-name"
                    className="w-full"
                    value={draftName}
                    autoComplete="off"
                    placeholder="main"
                    disabled={isEdit}
                    onChange={(event) => setDraftName(event.target.value)}
                  />
                </SettingsRowActions>
              </SettingsRow>
              <SettingsRow className="items-start gap-4">
                <SettingsRowMain className="w-[30%] min-w-0 flex-none">
                  <SettingsRowTitle>Success criteria</SettingsRowTitle>
                  <SettingsRowMeta className="whitespace-normal">
                    Each observable condition a Run must verify. A new row
                    appears as you fill the current one.
                  </SettingsRowMeta>
                </SettingsRowMain>
                <SettingsRowActions className="w-[70%] min-w-0 flex-none items-stretch pr-4">
                  <SuccessCriteriaInputs
                    criteria={criteria}
                    onChange={setCriteria}
                  />
                </SettingsRowActions>
              </SettingsRow>
              <SettingsRow className="items-start gap-4">
                <SettingsRowMain className="w-[30%] min-w-0 flex-none">
                  <SettingsRowTitle>
                    <FieldLabel htmlFor="agent-root">
                      Repository path
                    </FieldLabel>
                  </SettingsRowTitle>
                  <SettingsRowMeta className="whitespace-normal">
                    Choose an existing Git repository or an empty folder. Empty
                    folders are initialized automatically. The Draft is created
                    in a visible sibling worktree.
                  </SettingsRowMeta>
                </SettingsRowMain>
                <SettingsRowActions className="w-[70%] min-w-0 flex-none items-stretch pr-4">
                  <Field>
                    <InputGroup>
                      <InputGroupInput
                        id="agent-root"
                        value={root}
                        autoComplete="off"
                        placeholder="/Users/you/code/project"
                        disabled={isEdit}
                        onChange={(event) => setRoot(event.target.value)}
                      />
                      {!isEdit ? (
                        <InputGroupAddon align="inline-end">
                          <InputGroupButton
                            size="icon-xs"
                            aria-label="Choose workspace folder"
                            disabled={isPicking}
                            onClick={chooseFolder}
                          >
                            {isPicking ? <Spinner /> : <FolderOpenIcon />}
                          </InputGroupButton>
                        </InputGroupAddon>
                      ) : null}
                    </InputGroup>
                    {pickerError ? (
                      <FieldError>{pickerError}</FieldError>
                    ) : null}
                  </Field>
                </SettingsRowActions>
              </SettingsRow>
              <SettingsRow className="items-start gap-4">
                <SettingsRowMain className="w-[30%] min-w-0 flex-none">
                  <SettingsRowTitle>
                    <FieldLabel htmlFor="agent-trusted">
                      Trust workspace
                    </FieldLabel>
                  </SettingsRowTitle>
                  <SettingsRowMeta className="whitespace-normal">
                    Allows Run filesystem and terminal access in this workspace.
                  </SettingsRowMeta>
                </SettingsRowMain>
                <SettingsRowActions className="w-[70%] min-w-0 flex-none items-center justify-start pr-4">
                  <Checkbox
                    id="agent-trusted"
                    checked={trusted}
                    onCheckedChange={(checked) =>
                      setTrusted(checked === true)
                    }
                  />
                </SettingsRowActions>
              </SettingsRow>
              {isEdit ? (
                <SettingsRow className="items-start gap-4">
                  <SettingsRowMain className="w-[30%] min-w-0 flex-none">
                    <SettingsRowTitle>
                      <FieldLabel htmlFor="agent-environment">
                        Environment
                      </FieldLabel>
                    </SettingsRowTitle>
                    <SettingsRowMeta className="whitespace-normal">
                      Used when starting the next Run from this Draft.
                    </SettingsRowMeta>
                  </SettingsRowMain>
                  <SettingsRowActions className="w-[70%] min-w-0 flex-none items-stretch pr-4">
                    <Select
                      value={environmentId}
                      onValueChange={(value) => setEnvironmentId(value ?? "")}
                      disabled={readyEnvironments.length === 0}
                    >
                      <SelectTrigger
                        id="agent-environment"
                        aria-label="Environment"
                        className="w-full"
                      >
                        <SelectValue placeholder="Select Environment" />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectGroup>
                          {readyEnvironments.map((environment) => (
                            <SelectItem
                              key={environment.id}
                              value={environment.id}
                            >
                              {environment.name}
                            </SelectItem>
                          ))}
                        </SelectGroup>
                      </SelectContent>
                    </Select>
                  </SettingsRowActions>
                </SettingsRow>
              ) : null}
            </SettingsList>
            <SettingsDetailsActionBar sticky={false}>
              <Button
                type="button"
                variant="ghost"
                disabled={isPending}
                onClick={onClose}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={isPending || !canSubmit}>
                {isPending ? <Spinner data-icon="inline-start" /> : null}
                Save
              </Button>
            </SettingsDetailsActionBar>
          </form>
        </div>
      </ScrollArea>
    </section>
  )
}

export function SuccessCriteriaFields({
  criteria,
  onChange,
}: {
  criteria: readonly string[]
  onChange: (criteria: string[]) => void
}) {
  return (
    <FieldSet>
      <FieldLegend>Success criteria</FieldLegend>
      <FieldDescription>
        Each observable condition a Run must verify. A new row appears as you
        fill the current one.
      </FieldDescription>
      <SuccessCriteriaInputs
        criteria={withTrailingEmptyCriterion(criteria)}
        onChange={onChange}
      />
    </FieldSet>
  )
}

function SuccessCriteriaInputs({
  criteria,
  onChange,
}: {
  criteria: readonly string[]
  onChange: (criteria: string[]) => void
}) {
  const rows = withTrailingEmptyCriterion(criteria)
  const canRemove = rows.some((criterion) => criterion.trim().length > 0)

  return (
    <div className="flex w-full flex-col gap-2">
      <FieldGroup className="gap-2">
        {rows.map((criterion, index) => {
          const isTrailingEmpty =
            index === rows.length - 1 && criterion.trim() === ""
          return (
            <Field key={index}>
              <FieldLabel
                className="sr-only"
                htmlFor={`success-criterion-${index}`}
              >
                Success criterion {index + 1}
              </FieldLabel>
              <InputGroup className="items-start">
                <InputGroupTextarea
                  id={`success-criterion-${index}`}
                  value={criterion}
                  rows={1}
                  className="min-h-9 py-2"
                  placeholder={
                    index === 0
                      ? "Success criterion 1"
                      : `Success criterion ${index + 1}`
                  }
                  onChange={(event) => {
                    const next = [...rows]
                    next[index] = event.target.value
                    onChange(withTrailingEmptyCriterion(next))
                  }}
                />
                {canRemove && !isTrailingEmpty ? (
                  <InputGroupAddon align="inline-end" className="pt-2">
                    <InputGroupButton
                      size="icon-xs"
                      aria-label={`Remove success criterion ${index + 1}`}
                      onClick={() =>
                        onChange(
                          withTrailingEmptyCriterion(
                            rows.filter((_, itemIndex) => itemIndex !== index),
                          ),
                        )
                      }
                    >
                      <XIcon />
                    </InputGroupButton>
                  </InputGroupAddon>
                ) : null}
              </InputGroup>
            </Field>
          )
        })}
      </FieldGroup>
    </div>
  )
}

function selectedFolder(value: unknown) {
  if (value === null) return undefined
  if (!Array.isArray(value) || !value.every((item) => typeof item === "string")) {
    throw new Error("The native picker returned an invalid result.")
  }
  return value[0]
}
