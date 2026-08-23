import * as React from "react"

import type {
  EnvironmentLlmPolicyDto,
  LlmProviderDto,
} from "@agent-factory/runtime-client"
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@agent-factory/ui/components/alert"
import { Button } from "@agent-factory/ui/components/button"
import { Checkbox } from "@agent-factory/ui/components/checkbox"
import { Label } from "@agent-factory/ui/components/label"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@agent-factory/ui/components/select"
import { SparklesIcon, XIcon } from "lucide-react"
import { ScrollArea } from "@agent-factory/ui/components/scroll-area"

import {
  SettingsEmpty,
  SettingsGroup,
  SettingsList,
  SettingsRow,
  SettingsRowActions,
  SettingsRowMain,
  SettingsRowMeta,
  SettingsRowTitle,
  SettingsSection,
} from "@/components/settings/settings-primitives"

export function EnvironmentLlmSettings({
  policy,
  providers,
  onChange,
  onOpenProviders,
  disabled = false,
  needsSetup = false,
  issues = [],
}: {
  policy: EnvironmentLlmPolicyDto | null
  providers: readonly LlmProviderDto[]
  onChange: (policy: EnvironmentLlmPolicyDto | null) => void
  onOpenProviders: () => void
  disabled?: boolean
  needsSetup?: boolean
  issues?: readonly string[]
}) {
  const provider = providers.find((candidate) => candidate.id === policy?.providerId)
  // Models the Environment exposes. A provider's full pool is the starting point;
  // the user deselects any that should not be available here. A model the provider
  // has since retired stays visible so it can be reconciled rather than silently
  // dropped.
  const allowedModels = policy?.allowedModels ?? []
  const modelChoices = Array.from(
    new Set([...(provider?.allowedModels ?? []), ...allowedModels]),
  )
  const actionableIssues = issues.filter(
    (issue) => issue !== "Provider changed—review this Environment",
  )
  const providerWarningKey =
    needsSetup && actionableIssues.length > 0
      ? `${policy?.providerId ?? "none"}:${actionableIssues.join("|")}`
      : undefined
  const [dismissedWarning, setDismissedWarning] = React.useState<string>()
  const showProviderWarning =
    providerWarningKey !== undefined && dismissedWarning !== providerWarningKey
  const showEnvironmentIssues = !needsSetup && actionableIssues.length > 0

  if (providers.length === 0) {
    return (
      <SettingsSection
        title="Intelligence Provider"
        description="Choose a reusable provider and which of its models are available in this Environment."
      >
        <SettingsList>
          <SettingsEmpty
            icon={<SparklesIcon />}
            title="No Intelligence Providers"
            description="Create an application-level provider before configuring this Environment."
            action={
              <Button type="button" variant="outline" size="sm" onClick={onOpenProviders}>
                Open Providers
              </Button>
            }
          />
        </SettingsList>
      </SettingsSection>
    )
  }

  const selectProvider = (providerId: string | null) => {
    if (!providerId) return
    const next = providers.find((candidate) => candidate.id === providerId)
    if (!next) return
    // Selecting a provider exposes every model it offers and defaults to the
    // first one; the user narrows from there.
    onChange({
      providerId,
      allowedModels: [...next.allowedModels],
      defaultModel: next.allowedModels[0] ?? "",
    })
  }

  const toggleModel = (model: string, checked: boolean) => {
    if (!policy) return
    const next = checked
      ? Array.from(new Set([...policy.allowedModels, model]))
      : policy.allowedModels.filter((candidate) => candidate !== model)
    // The default must remain available; if it was deselected, fall back to the
    // first remaining model so the Environment always has a valid default.
    const defaultModel = next.includes(policy.defaultModel)
      ? policy.defaultModel
      : (next[0] ?? "")
    onChange({ ...policy, allowedModels: next, defaultModel })
  }

  return (
    <SettingsSection
      title="Intelligence Provider"
      description="Choose a reusable provider and which of its models are available in this Environment."
    >
      <SettingsList>
        {showProviderWarning || showEnvironmentIssues ? (
          <Alert>
            <AlertTitle>
              {needsSetup
                ? "Provider changed—review this Environment"
                : "Environment needs setup"}
            </AlertTitle>
            <AlertDescription>{actionableIssues.join(" ")}</AlertDescription>
            {showProviderWarning ? (
              <AlertAction>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label="Dismiss provider warning"
                  onClick={() => setDismissedWarning(providerWarningKey)}
                >
                  <XIcon />
                </Button>
              </AlertAction>
            ) : null}
          </Alert>
        ) : null}

        <SettingsRow>
          <SettingsRowMain>
            <SettingsRowTitle>
              <Label htmlFor="environment-provider">Provider</Label>
            </SettingsRowTitle>
            <SettingsRowMeta>
              Several Environments can share the same provider.
            </SettingsRowMeta>
          </SettingsRowMain>
          <SettingsRowActions>
            <Select
              value={policy?.providerId ?? ""}
              disabled={disabled}
              onValueChange={selectProvider}
            >
              <SelectTrigger id="environment-provider" className="w-64" aria-label="Intelligence Provider">
                <SelectValue>
                  {provider?.name ?? "Choose a provider"}
                </SelectValue>
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {providers.map((candidate) => (
                    <SelectItem key={candidate.id} value={candidate.id}>
                      {candidate.name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </SettingsRowActions>
        </SettingsRow>

        {policy && provider ? (
          <>
            <SettingsGroup label="Available models">
              <ScrollArea
                data-testid="environment-model-list"
                className="max-h-56"
                viewportClassName="max-h-56"
              >
                {/* py-4 keeps the first and last models clear of the 1rem
                    scroll fade at rest. pr-6 keeps a long name clear of the
                    overlay scrollbar. */}
                <div className="flex flex-col gap-2 py-4 pr-6">
                  {modelChoices.map((model) => {
                    const id = `environment-model-${model}`
                    const available = provider.allowedModels.includes(model)
                    return (
                      <div key={model} className="flex items-center gap-2">
                        <Checkbox
                          id={id}
                          checked={allowedModels.includes(model)}
                          disabled={disabled}
                          onCheckedChange={(checked) =>
                            toggleModel(model, checked === true)
                          }
                        />
                        <Label htmlFor={id} className="font-normal">
                          {model}
                          {!available ? (
                            <span className="ml-2 text-muted-foreground">
                              No longer allowed by provider
                            </span>
                          ) : null}
                        </Label>
                      </div>
                    )
                  })}
                </div>
              </ScrollArea>
            </SettingsGroup>

            <SettingsRow>
              <SettingsRowMain>
                <SettingsRowTitle>
                  <Label htmlFor="environment-default-model">Default model</Label>
                </SettingsRowTitle>
                <SettingsRowMeta>
                  The model this Environment uses unless a session picks another.
                </SettingsRowMeta>
              </SettingsRowMain>
              <SettingsRowActions>
                <Select
                  value={policy.defaultModel}
                  disabled={disabled || allowedModels.length === 0}
                  onValueChange={(value) => {
                    if (!value || !policy) return
                    onChange({ ...policy, defaultModel: value })
                  }}
                >
                  <SelectTrigger
                    id="environment-default-model"
                    className="w-64"
                    aria-label="Environment default model"
                  >
                    <SelectValue placeholder="Choose a default" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {allowedModels.map((model) => (
                        <SelectItem key={model} value={model}>
                          {model}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </SettingsRowActions>
            </SettingsRow>
          </>
        ) : null}
      </SettingsList>
    </SettingsSection>
  )
}