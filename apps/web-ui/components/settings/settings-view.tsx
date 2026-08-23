import * as React from "react"

import {
  ArrowLeftIcon,
  BoxIcon,
  BrainIcon,
  CopyIcon,
  KeyRoundIcon,
  PuzzleIcon,
  Settings2Icon,
  TerminalIcon,
} from "lucide-react"

import type {
  RuntimeIntent,
  ThemePreference,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"
import { Badge } from "@agent-factory/ui/components/badge"
import { Button } from "@agent-factory/ui/components/button"
import { ScrollArea } from "@agent-factory/ui/components/scroll-area"
import { Spinner } from "@agent-factory/ui/components/spinner"
import { Switch } from "@agent-factory/ui/components/switch"
import {
  ToggleGroup,
  ToggleGroupItem,
} from "@agent-factory/ui/components/toggle-group"

import type { EmitIntent } from "@/components/shell/workspace-shell"
import {
  SettingsPrimaryAction,
  SettingsEmpty,
  SettingsList,
  SettingsPageHeader,
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
import {
  SecretSettings,
  type SecretSettingsSelection,
} from "@/components/settings/secret-settings"
import {
  PluginSettings,
  type PluginSettingsSelection,
} from "@/components/settings/plugin-settings"
import {
  EnvironmentSettings,
  type EnvironmentSettingsSelection,
} from "@/components/settings/environment-settings"
import {
  LlmProviderSettings,
  type LlmProviderSettingsSelection,
} from "@/components/settings/llm-provider-settings"

const themeOptions: readonly ThemePreference[] = ["light", "dark", "system"]
const settingsSections = [
  {
    id: "general",
    label: "General",
    icon: Settings2Icon,
  },
  {
    id: "llmProviders",
    label: "Providers",
    icon: BrainIcon,
  },
  {
    id: "environments",
    label: "Environments",
    icon: BoxIcon,
  },
  {
    id: "secrets",
    label: "Secrets",
    icon: KeyRoundIcon,
  },
  {
    id: "agents",
    label: "Harnesses",
    icon: TerminalIcon,
  },
  {
    id: "plugins",
    label: "Plugins",
    icon: PuzzleIcon,
  },
] as const
type SettingsSection = (typeof settingsSections)[number]["id"]
type DialogIntent = Extract<
  RuntimeIntent,
  { type: "settings.theme" | "settings.notifications" }
>

export function SettingsView({
  onClose,
  projection,
  emitIntent,
  initialSection = "general",
  createEnvironmentRequested = false,
}: {
  onClose: () => void
  projection: WorkspaceProjection
  emitIntent: EmitIntent
  initialSection?: SettingsSection
  /** Set when the caller opened Settings in order to create an Environment. */
  createEnvironmentRequested?: boolean
}) {
  const settings = projection.settings
  const [isPending, startTransition] = React.useTransition()
  const [createEnvironment, setCreateEnvironment] = React.useState(createEnvironmentRequested)
  const [createProvider, setCreateProvider] = React.useState(false)
  const [environmentSelection, setEnvironmentSelection] =
    React.useState<EnvironmentSettingsSelection>()
  const [providerSelection, setProviderSelection] =
    React.useState<LlmProviderSettingsSelection>()
  const [pluginSelection, setPluginSelection] =
    React.useState<PluginSettingsSelection>()
  const [environmentsDirty, setEnvironmentsDirty] = React.useState(false)
  const [providersDirty, setProvidersDirty] = React.useState(false)
  const [createSecret, setCreateSecret] = React.useState(false)
  const [secretSelection, setSecretSelection] =
    React.useState<SecretSettingsSelection>()
  const [secretsDirty, setSecretsDirty] = React.useState(false)
  const [activeSection, setActiveSection] =
    React.useState<SettingsSection>(initialSection)
  // Leaving the Environments section, by any route, has to go through the same guard
  // that switching Environments does — otherwise the back button is a way to lose
  // edits that the in-section navigation protects.
  const guard = useUnsavedChangesGuard(
    (activeSection === "environments" && environmentsDirty) ||
      (activeSection === "llmProviders" && providersDirty) ||
      (activeSection === "secrets" && secretsDirty),
  )
  useSettingsErrorToast("Settings update failed", projection.settingsError)

  const dispatchSetting = (intent: DialogIntent) => {
    startTransition(async () => emitIntent(intent))
  }

  const active =
    settingsSections.find((section) => section.id === activeSection) ??
    settingsSections[0]
  const environmentDetails =
    activeSection === "environments" && environmentSelection !== undefined
  const providerDetails =
    activeSection === "llmProviders" && providerSelection !== undefined
  const pluginDetails =
    activeSection === "plugins" && pluginSelection !== undefined
  const secretDetails =
    activeSection === "secrets" && secretSelection !== undefined
  const pageTitle = active.label

  const navigateToSection = (section: SettingsSection) => {
    guard.request(() => {
      setActiveSection(section)
      setEnvironmentSelection(undefined)
      setProviderSelection(undefined)
      setPluginSelection(undefined)
      setSecretSelection(undefined)
      setEnvironmentsDirty(false)
      setProvidersDirty(false)
      setSecretsDirty(false)
    })
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <nav
        aria-label="Settings sections"
        className="flex h-full w-[var(--settings-nav-width)] flex-col overflow-y-auto border-r bg-sidebar"
      >
        {/* Reserve the hidden-inset titlebar band so content starts below the
           traffic lights and the strip stays draggable. */}
        <div
          data-native-drag-region
          className="h-11 shrink-0"
          aria-hidden="true"
        />
        <div className="flex flex-col gap-2 px-2 pb-3 pt-0.5">
          <Button
            className="h-8 w-full justify-start gap-2 px-2.5"
            onClick={() => guard.request(onClose)}
            size="sm"
            type="button"
            variant="ghost"
          >
            <ArrowLeftIcon
              data-icon="inline-start"
              className="size-4 shrink-0"
            />
            Back
          </Button>
          <div className="flex flex-col">
            <p className="px-2.5 py-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">
              Settings
            </p>
            <ul className="flex flex-col">
              {settingsSections.map((section) => (
                <li key={section.id}>
                  <Button
                    variant="ghost"
                    size="sm"
                    data-active={activeSection === section.id ? "true" : undefined}
                    aria-current={
                      activeSection === section.id ? "page" : undefined
                    }
                    onClick={() => navigateToSection(section.id)}
                    className="h-8 w-full justify-start gap-2 px-2.5 data-[active=true]:bg-sidebar-accent data-[active=true]:font-medium data-[active=true]:text-sidebar-accent-foreground"
                  >
                    <section.icon className="size-4 shrink-0" />
                    <span>{section.label}</span>
                  </Button>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </nav>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {pluginDetails ||
          environmentDetails ||
          providerDetails ||
          secretDetails ? null : (
          <header className="mx-auto w-full max-w-[var(--content-max-width)] shrink-0 px-5 pb-4 pt-6">
            {/* One primary action per page, in one place. The dialog it opens is
                its own way out, so the label never becomes "Close". */}
            <SettingsPageHeader
              title={pageTitle}
              navigation={undefined}
              action={
                activeSection === "llmProviders" && !providerDetails ? (
                  <SettingsPrimaryAction
                    label="Add"
                    onClick={() => setCreateProvider(true)}
                  />
                ) : activeSection === "environments" && !environmentDetails ? (
                  <SettingsPrimaryAction
                    label="Add"
                    onClick={() => setCreateEnvironment(true)}
                  />
                ) : activeSection === "secrets" && !secretDetails ? (
                  <SettingsPrimaryAction
                    label="Add"
                    onClick={() => setCreateSecret(true)}
                  />
                ) : undefined
              }
            />
          </header>
        )}
        <ScrollArea className="min-h-0 flex-1">
          <div className="mx-auto flex w-full max-w-[var(--content-max-width)] flex-col gap-12 px-5 pb-8 pt-4">
            <React.Activity
              mode={activeSection === "general" ? "visible" : "hidden"}
            >
              <SettingsSection
                title="Preferences"
                description="Applied to every Environment and every agent session."
              >
                {settings ? (
                  <SettingsList>
                    <SettingsRow>
                      <SettingsRowMain>
                        <SettingsRowTitle>
                          <span id="theme-label">Theme</span>
                        </SettingsRowTitle>
                        <SettingsRowMeta>
                          Match the system or choose a fixed appearance.
                        </SettingsRowMeta>
                      </SettingsRowMain>
                      <SettingsRowActions>
                        <ToggleGroup
                          aria-labelledby="theme-label"
                          value={[settings.theme]}
                          disabled={isPending}
                          onValueChange={(value) => {
                            const theme = value[0]
                            if (
                              theme &&
                              themeOptions.includes(theme as ThemePreference)
                            ) {
                              dispatchSetting({
                                type: "settings.theme",
                                theme: theme as ThemePreference,
                              })
                            }
                          }}
                        >
                          <ToggleGroupItem value="light">Light</ToggleGroupItem>
                          <ToggleGroupItem value="dark">Dark</ToggleGroupItem>
                          <ToggleGroupItem value="system">System</ToggleGroupItem>
                        </ToggleGroup>
                      </SettingsRowActions>
                    </SettingsRow>
                    <SettingsRow>
                      <SettingsRowMain>
                        <SettingsRowTitle>
                          <span id="notifications-label">
                            Native notifications
                          </span>
                        </SettingsRowTitle>
                        <SettingsRowMeta>
                          Notify when background agent work needs attention.
                        </SettingsRowMeta>
                      </SettingsRowMain>
                      <SettingsRowActions>
                        {isPending ? <Spinner /> : null}
                        <Switch
                          aria-labelledby="notifications-label"
                          checked={settings.nativeNotifications}
                          disabled={isPending}
                          onCheckedChange={(enabled) =>
                            dispatchSetting({
                              type: "settings.notifications",
                              enabled,
                            })
                          }
                        />
                      </SettingsRowActions>
                    </SettingsRow>
                  </SettingsList>
                ) : (
                  <SettingsList>
                    <SettingsEmpty
                      icon={<Settings2Icon />}
                      title="Settings unavailable"
                      description="Connect the Rust runtime to load persisted preferences."
                    />
                  </SettingsList>
                )}
              </SettingsSection>
            </React.Activity>
            <React.Activity
              mode={activeSection === "agents" ? "visible" : "hidden"}
            >
              <SettingsSection
                title="Harnesses"
                description="Agents approved for use with Agent Factory."
              >
                <SettingsList>
                  {!projection.herdr.connected ? (
                    <SettingsRow>
                      <SettingsRowMain>
                        <SettingsRowTitle>Herdr unavailable</SettingsRowTitle>
                        <SettingsRowMeta>
                          Run <code>herdr</code> in a terminal, then reopen
                          Harnesses.
                        </SettingsRowMeta>
                      </SettingsRowMain>
                      <SettingsRowActions>
                        <Badge variant="outline">Unavailable</Badge>
                        <Button
                          variant="outline"
                          size="sm"
                          aria-label="Copy Herdr start command"
                          onClick={() =>
                            void navigator.clipboard?.writeText("herdr")
                          }
                        >
                          <CopyIcon data-icon="inline-start" />
                          Copy command
                        </Button>
                      </SettingsRowActions>
                    </SettingsRow>
                  ) : null}
                  {projection.herdr.connected
                    ? projection.harnesses.map((harness) => (
                        <SettingsRow key={harness.id}>
                          <SettingsRowMain>
                            <SettingsRowTitle>{harness.name}</SettingsRowTitle>
                            <SettingsRowMeta>
                              {harness.guidance}
                              {harness.action ? (
                                <>
                                  {" "}
                                  <code>{harness.action.command}</code>
                                </>
                              ) : null}
                            </SettingsRowMeta>
                          </SettingsRowMain>
                          <SettingsRowActions>
                            <Badge
                              variant={
                                harness.readiness === "ready"
                                  ? "secondary"
                                  : "outline"
                              }
                            >
                              {harnessReadinessLabel(harness.readiness)}
                            </Badge>
                            {harness.action ? (
                              <Button
                                variant="outline"
                                size="sm"
                                aria-label={`${harness.action.label} for ${harness.name}`}
                                onClick={() =>
                                  void navigator.clipboard?.writeText(
                                    harness.action?.command ?? "",
                                  )
                                }
                              >
                                <CopyIcon data-icon="inline-start" />
                                Copy command
                              </Button>
                            ) : null}
                          </SettingsRowActions>
                        </SettingsRow>
                      ))
                    : null}
                  {projection.herdr.connected &&
                  projection.harnesses.length === 0 ? (
                    <SettingsEmpty
                      icon={<TerminalIcon />}
                      title="No approved agents"
                      description="This Herdr installation reported none of the agents approved by Agent Factory."
                    />
                  ) : null}
                </SettingsList>
              </SettingsSection>
            </React.Activity>
            <React.Activity
              mode={activeSection === "llmProviders" ? "visible" : "hidden"}
            >
              <LlmProviderSettings
                projection={projection}
                emitIntent={emitIntent}
                createRequested={createProvider}
                onCreateRequestHandled={() => setCreateProvider(false)}
                onDirtyChange={setProvidersDirty}
                selection={providerSelection}
                onSelectionChange={setProviderSelection}
              />
            </React.Activity>
            <React.Activity
              mode={activeSection === "environments" ? "visible" : "hidden"}
            >
              <EnvironmentSettings
                projection={projection}
                emitIntent={emitIntent}
                createRequested={createEnvironment}
                onCreateRequestHandled={() => setCreateEnvironment(false)}
                onDirtyChange={setEnvironmentsDirty}
                selection={environmentSelection}
                onSelectionChange={setEnvironmentSelection}
                onOpenProviders={() =>
                  guard.request(() => {
                    setActiveSection("llmProviders")
                    setEnvironmentSelection(undefined)
                    setProviderSelection(undefined)
                    setEnvironmentsDirty(false)
                  })
                }
              />
            </React.Activity>
            <React.Activity
              mode={activeSection === "secrets" ? "visible" : "hidden"}
            >
              <SecretSettings
                projection={projection}
                emitIntent={emitIntent}
                createRequested={createSecret}
                onCreateRequestHandled={() => setCreateSecret(false)}
                onDirtyChange={setSecretsDirty}
                selection={secretSelection}
                onSelectionChange={setSecretSelection}
              />
            </React.Activity>
            <React.Activity
              mode={activeSection === "plugins" ? "visible" : "hidden"}
            >
                <PluginSettings
                  projection={projection}
                  emitIntent={emitIntent}
                  selection={pluginSelection}
                  onSelectionChange={setPluginSelection}
                />
            </React.Activity>
          </div>
        </ScrollArea>
      </div>
      <UnsavedChangesDialog
        open={guard.isOpen}
        onConfirm={guard.confirm}
        onCancel={guard.cancel}
      />
    </div>
  )
}

function harnessReadinessLabel(
  readiness: WorkspaceProjection["harnesses"][number]["readiness"],
) {
  if (readiness === "ready") return "Ready"
  if (readiness === "setup_required") return "Setup required"
  return "Installation required"
}
