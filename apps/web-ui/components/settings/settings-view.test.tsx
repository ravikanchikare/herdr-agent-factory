import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type * as React from "react"

import type {
  EnvironmentDto,
  LlmProviderDto,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"

import { SettingsView } from "@/components/settings/settings-view"

const baseProjection: WorkspaceProjection = {
  revision: 1,
  connection: "ready",
  projects: [],
  herdr: { connected: true, freshness: "live", issues: [] },
  harnesses: [],
  sessions: [],
  liveAgents: [],
  targetWorkspace: {
    targetGroups: [],
    workContexts: [],
    panes: [],
    terminals: [],
  },
  factoryRuns: [],
  terminals: [],
  files: { state: "idle", entries: [] },
  environments: [],
  llmProviders: [],
  secrets: [],
  pluginRegistries: [],
  pluginCatalogs: [],
  plugins: { installed: [], localMcpServers: [] },
  settings: {
    theme: "system",
    nativeNotifications: true,
    layout: { inspectorPercent: 28, terminalPercent: 24 },
  },
}

const provider: LlmProviderDto = {
  id: "00000000-0000-4000-8000-000000000010",
  name: "Shared Ollama",
  type: "ollama",
  endpoint: "http://127.0.0.1:11434",
  credentialRef: null,
  allowedModels: ["qwen3-coder"],
  readiness: { state: "ready", issues: [] },
}

const environment: EnvironmentDto = {
  id: "local",
  name: "Local",
  codingHarnessId: "claude",
  evaluationHarnessId: "claude",
  plugins: [],
  permissions: {
    trustedRead: "allow",
    trustedWrite: "ask",
    terminal: "ask",
  },
  registryIds: [],
  environmentVariables: [],
  llm: {
    providerId: provider.id,
    allowedModels: [...provider.allowedModels],
    defaultModel: "qwen3-coder",
  },
  resolvedLlm: {
    providerId: provider.id,
    providerName: provider.name,
    type: provider.type,
    endpoint: provider.endpoint,
    credentialRef: null,
    allowedModels: [...provider.allowedModels],
    defaultModel: "qwen3-coder",
  },
  llmNeedsSetup: false,
  readiness: { state: "ready", issues: [] },
}

function renderSettingsView(
  projection: WorkspaceProjection,
  overrides: Partial<React.ComponentProps<typeof SettingsView>> = {},
) {
  return render(
    <SettingsView
      projection={projection}
      onClose={() => {}}
      emitIntent={async () => {}}
      {...overrides}
    />,
  )
}

describe("SettingsView", () => {
  it("shows Settings sections in the required order", () => {
    renderSettingsView(baseProjection)

    expect(
      within(screen.getByRole("navigation", { name: "Settings sections" }))
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual([
      "Back",
      "General",
      "Providers",
      "Environments",
      "Secrets",
      "Harnesses",
      "Plugins",
    ])
    expect(screen.queryByRole("button", { name: "Updates" })).toBeNull()
  })

  it("uses contextual Add actions for Providers, Environments, and Secrets", () => {
    renderSettingsView(baseProjection)

    fireEvent.click(screen.getByRole("button", { name: "Providers" }))
    expect(screen.getByText("No Providers")).toBeVisible()
    expect(screen.getAllByRole("button", { name: "Add" }).length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole("button", { name: "Environments" }))
    expect(screen.getAllByRole("button", { name: "Add" }).length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole("button", { name: "Secrets" }))
    expect(screen.getAllByRole("button", { name: "Add" }).length).toBeGreaterThan(0)
    expect(screen.queryByRole("button", { name: "Create" })).toBeNull()
  })

  it("does not activate hidden Settings section effects", async () => {
    const emitIntent = vi.fn(async () => {})
    renderSettingsView(baseProjection, { emitIntent })

    expect(emitIntent).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole("button", { name: "Plugins" }))

    await waitFor(() => {
      expect(emitIntent).toHaveBeenCalledWith({ type: "registry.list" })
      expect(emitIntent).toHaveBeenCalledWith({ type: "plugin.list" })
    })
  })

  it("shows the one next action when Herdr is not running", () => {
    renderSettingsView({
      ...baseProjection,
      herdr: {
        connected: false,
        freshness: "last_observed",
        issues: ["Herdr is not running. Start it, then reconnect Agent Factory."],
      },
      harnesses: [],
    })

    fireEvent.click(screen.getByRole("button", { name: "Harnesses" }))

    expect(screen.getByText("Herdr unavailable")).toBeVisible()
    expect(screen.getByText("herdr")).toBeVisible()
    expect(
      screen.getByRole("button", { name: "Copy Herdr start command" }),
    ).toBeVisible()
  })

  it("renders the compact approved Harness list with only required actions", () => {
    renderSettingsView({
      ...baseProjection,
      herdr: {
        connected: true,
        freshness: "live",
        version: "0.8.0",
        protocol: 19,
        issues: [],
      },
      harnesses: [
        {
          id: "claude",
          name: "Claude Code",
          readiness: "ready",
          guidance: "Ready to launch with Herdr.",
          action: null,
        },
        {
          id: "codex",
          name: "Codex",
          readiness: "installation_required",
          guidance: "Install Codex, then restart Herdr.",
          action: {
            label: "Copy install command",
            command: "npm install -g @openai/codex",
          },
        },
      ],
    })

    fireEvent.click(screen.getByRole("button", { name: "Harnesses" }))

    expect(screen.getByText("Claude Code")).toBeInTheDocument()
    expect(screen.getByText("Ready")).toBeVisible()
    expect(screen.getByText("Installation required")).toBeVisible()
    expect(screen.getByText("npm install -g @openai/codex")).toBeVisible()
    expect(
      screen.getByRole("button", {
        name: "Copy install command for Codex",
      }),
    ).toBeVisible()
    expect(screen.queryByText("remote")).toBeNull()
    expect(screen.queryByText("2026.08.04.1")).toBeNull()
    expect(screen.queryByText("ignored remote manifest")).toBeNull()
    expect(screen.queryByRole("button", { name: /Claude Code/ })).toBeNull()
  })

  it("shows a short setup action without opening agent details", () => {
    renderSettingsView({
      ...baseProjection,
      harnesses: [
        {
          id: "claude",
          name: "Claude Code",
          readiness: "setup_required",
          guidance: "Run Claude Code once to finish setup, then restart Herdr.",
          action: {
            label: "Copy setup command",
            command: "claude",
          },
        },
      ],
    })

    fireEvent.click(screen.getByRole("button", { name: "Harnesses" }))

    expect(screen.getByText("Setup required")).toBeVisible()
    expect(screen.getByText("claude")).toBeVisible()
    expect(
      screen.getByRole("button", {
        name: "Copy setup command for Claude Code",
      }),
    ).toBeVisible()
  })


  it("opens straight into an Environment draft when asked to", () => {
    renderSettingsView(baseProjection, {
      initialSection: "environments",
      createEnvironmentRequested: true,
    })

    expect(screen.getByLabelText("Name")).toBeInTheDocument()
  })

  it("uses the same list-to-details navigation for Environments and Providers", () => {
    renderSettingsView(
      {
        ...baseProjection,
        environments: [environment],
        llmProviders: [provider],
      },
      { initialSection: "environments" },
    )

    expect(screen.getByText("Environments", { selector: "h2" })).toBeVisible()
    const manageEnvironment = screen.getByRole("button", {
      name: "Open Local",
    })
    const environmentCard = manageEnvironment.closest('[data-slot="card"]')
    expect(environmentCard).not.toBeNull()
    expect(within(environmentCard as HTMLElement).queryByRole("tab")).toBeNull()
    fireEvent.click(manageEnvironment)
    // Back sits before the breadcrumb on every detail page, named for where it
    // goes so it is not confused with the section rail's own Back.
    expect(
      screen.getByRole("button", { name: "Back to Environments" }),
    ).toBeVisible()
    const environmentBreadcrumb = screen.getByRole("navigation", {
      name: "Breadcrumb",
    })
    expect(
      within(environmentBreadcrumb).getByRole("button", {
        name: "Environments",
      }),
    ).toBeVisible()
    expect(within(environmentBreadcrumb).getByText("Local")).toHaveAttribute(
      "aria-current",
      "page",
    )
    fireEvent.click(
      within(environmentBreadcrumb).getByRole("button", {
        name: "Environments",
      }),
    )
    expect(screen.getByRole("button", { name: "Open Local" })).toBeVisible()

    fireEvent.click(screen.getByRole("button", { name: "Providers" }))
    const manageProvider = screen.getByRole("button", {
      name: "Open Shared Ollama",
    })
    const providerCard = manageProvider.closest('[data-slot="card"]')
    expect(providerCard).not.toBeNull()
    expect(within(providerCard as HTMLElement).queryByRole("tab")).toBeNull()
    fireEvent.click(manageProvider)
    expect(
      screen.getByRole("button", { name: "Back to Providers" }),
    ).toBeVisible()
    const providerBreadcrumb = screen.getByRole("navigation", {
      name: "Breadcrumb",
    })
    expect(
      within(providerBreadcrumb).getByRole("button", { name: "Providers" }),
    ).toBeVisible()
    expect(
      within(providerBreadcrumb).getByText("Shared Ollama"),
    ).toHaveAttribute("aria-current", "page")
    fireEvent.click(
      within(providerBreadcrumb).getByRole("button", { name: "Providers" }),
    )
    expect(
      screen.getByRole("button", { name: "Open Shared Ollama" }),
    ).toBeVisible()
  })

  it("guards unsaved Environment edits when leaving the section", async () => {
    renderSettingsView(baseProjection, {
      initialSection: "environments",
      createEnvironmentRequested: true,
    })

    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Local Ollama" },
    })
    fireEvent.keyDown(screen.getByLabelText("Name"), { key: "Enter" })
    fireEvent.click(screen.getByRole("button", { name: "Secrets" }))

    // The section change is held until the user decides.
    expect(screen.getByText("Discard unsaved changes?")).toBeInTheDocument()

    fireEvent.click(screen.getByRole("button", { name: "Discard" }))
    await waitFor(() =>
      expect(screen.getByText("Secrets", { selector: "h2" })).toBeInTheDocument(),
    )
    await waitFor(() =>
      expect(screen.queryByText("Discard unsaved changes?")).toBeNull(),
    )

    fireEvent.click(screen.getByRole("button", { name: "Environments" }))
    await waitFor(() => {
      expect(screen.queryByText("Discard unsaved changes?")).toBeNull()
      expect(screen.getByText("No Environments yet")).toBeVisible()
    })
  })
})
