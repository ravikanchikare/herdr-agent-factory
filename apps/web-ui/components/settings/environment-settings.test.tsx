import * as React from "react"

import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type {
  EnvironmentDto,
  LlmProviderDto,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"
import { TestRuntimeClient } from "@agent-factory/runtime-client/testing"

import {
  EnvironmentSettings,
  type EnvironmentSettingsSelection,
} from "@/components/settings/environment-settings"

const provider: LlmProviderDto = {
  id: "00000000-0000-4000-8000-000000000010",
  name: "Shared Ollama",
  type: "ollama",
  endpoint: "http://127.0.0.1:11434",
  credentialRef: null,
  allowedModels: ["qwen3-coder", "glm-5.2:cloud"],
  readiness: { state: "ready", issues: [] },
}

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
  llmProviders: [provider],
  environments: [],
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

function environment(overrides: Partial<EnvironmentDto> = {}): EnvironmentDto {
  return {
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
    ...overrides,
  }
}

function renderEnvironments(
  projection: WorkspaceProjection,
  options: { createRequested?: boolean; onOpenProviders?: () => void } = {},
) {
  const client = new TestRuntimeClient(projection)
  return {
    client,
    ...render(
      <EnvironmentSettingsHarness
        projection={projection}
        emitIntent={(intent) => client.dispatch(intent)}
        createRequested={options.createRequested ?? false}
        onCreateRequestHandled={() => {}}
        onOpenProviders={options.onOpenProviders ?? (() => {})}
      />,
    ),
  }
}

function EnvironmentSettingsHarness(
  props: Omit<
    React.ComponentProps<typeof EnvironmentSettings>,
    "selection" | "onSelectionChange"
  >,
) {
  const [selection, setSelection] =
    React.useState<EnvironmentSettingsSelection>()
  return (
    <EnvironmentSettings
      {...props}
      selection={selection}
      onSelectionChange={setSelection}
    />
  )
}

describe("Environment Settings", () => {
  it("links the no-provider state to Providers", () => {
    const onOpenProviders = vi.fn()
    renderEnvironments(
      { ...baseProjection, llmProviders: [], environments: [environment({ llm: null })] },
      { onOpenProviders },
    )

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    expect(screen.getByText("No Intelligence Providers")).toBeVisible()
    fireEvent.click(screen.getByRole("button", { name: "Open Providers" }))
    expect(onOpenProviders).toHaveBeenCalledOnce()
  })

  it("lets several Environments share a provider with independent filters", async () => {
    renderEnvironments({
      ...baseProjection,
      environments: [
        environment({ id: "coding", name: "Coding" }),
        environment({
          id: "evaluation",
          name: "Evaluation",
          llm: {
            providerId: provider.id,
            allowedModels: ["glm-5.2:cloud"],
            defaultModel: "glm-5.2:cloud",
          },
        }),
      ],
    })

    expect(screen.getByRole("button", { name: "Open Coding" })).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Open Evaluation" }))
    // Available models are always shown; the Evaluation Environment exposed only
    // glm-5.2:cloud, so qwen3-coder is unchecked while glm-5.2:cloud is checked.
    await waitFor(() => {
      expect(screen.getByRole("checkbox", { name: "glm-5.2:cloud" })).toBeChecked()
    })
    expect(screen.getByRole("checkbox", { name: "qwen3-coder" })).not.toBeChecked()
  })

  it("shows a dismissible provider warning only with an actionable issue", async () => {
    const affected = environment({
      llmNeedsSetup: true,
      readiness: {
        state: "needs_setup",
        issues: [
          "Provider changed—review this Environment",
          "The selected model is no longer allowed",
        ],
      },
    })
    const { client } = renderEnvironments({
      ...baseProjection,
      environments: [affected],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    expect(
      screen.getAllByText("Provider changed—review this Environment").length,
    ).toBeGreaterThan(0)
    fireEvent.click(
      screen.getByRole("button", { name: "Dismiss provider warning" }),
    )
    expect(
      screen.queryByRole("button", { name: "Dismiss provider warning" }),
    ).toBeNull()
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull()

    fireEvent.click(
      screen.getByRole("checkbox", { name: "glm-5.2:cloud" }),
    )
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    await waitFor(() =>
      expect(
        client.intents.find((intent) => intent.type === "environment.configuration.set"),
      ).toEqual({
        type: "environment.configuration.set",
        environmentId: "local",
        configuration: {
          name: "Local",
          environmentVariables: [],
          llm: {
            providerId: provider.id,
            allowedModels: ["qwen3-coder"],
            defaultModel: "qwen3-coder",
          },
          plugins: [],
          registries: [],
        },
      }),
    )
  })

  it("shows the provider name instead of its internal ID", async () => {
    renderEnvironments({
      ...baseProjection,
      environments: [environment()],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    await waitFor(() => {
      expect(screen.getByLabelText("Intelligence Provider")).toHaveTextContent(
        "Shared Ollama",
      )
    })
    expect(screen.getByLabelText("Intelligence Provider")).not.toHaveTextContent(
      provider.id,
    )
    expect(
      screen.getByLabelText("Environment default model"),
    ).toHaveTextContent("qwen3-coder")
  })

  it("leaves details after a successful save without an unsaved warning", async () => {
    const { client } = renderEnvironments({
      ...baseProjection,
      environments: [environment()],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    fireEvent.click(screen.getByRole("checkbox", { name: "glm-5.2:cloud" }))
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    await waitFor(() =>
      expect(
        client.intents.find(
          (intent) => intent.type === "environment.configuration.set",
        ),
      ).toBeTruthy(),
    )

    fireEvent.click(screen.getByRole("button", { name: "Back to Environments" }))
    expect(screen.queryByText("Discard unsaved changes?")).toBeNull()
    expect(screen.getByRole("button", { name: "Open Local" })).toBeVisible()
  })

  it("uses the same header actions as Provider details", () => {
    renderEnvironments({
      ...baseProjection,
      environments: [environment()],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    expect(screen.getByRole("button", { name: "Rename Local" })).toBeVisible()
    fireEvent.click(screen.getByRole("button", { name: "Actions for Local" }))

    expect(screen.queryByRole("menuitem", { name: "Rename" })).toBeNull()
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument()
  })

  it("saves an inline rename when the name field loses focus", async () => {
    const { client } = renderEnvironments({
      ...baseProjection,
      environments: [environment()],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    fireEvent.click(screen.getByRole("button", { name: "Rename Local" }))
    await waitFor(() => expect(screen.getByLabelText("Name")).toHaveValue("Local"))
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Local Ollama" },
    })
    fireEvent.blur(screen.getByLabelText("Name"))

    await waitFor(() =>
      expect(
        client.intents.find((intent) => intent.type === "environment.configuration.set"),
      ).toMatchObject({
        type: "environment.configuration.set",
        environmentId: "local",
        configuration: { name: "Local Ollama" },
      }),
    )
  })

  it("keeps an untouched draft free of unsaved-change actions", async () => {
    renderEnvironments(baseProjection, { createRequested: true })

    const add = await screen.findByRole("button", { name: "Add" })
    expect(add).toBeDisabled()
    expect(add.parentElement).not.toHaveClass("border-t")
  })

  it("returns to clean when the saved model set is restored", async () => {
    renderEnvironments({
      ...baseProjection,
      environments: [environment()],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    const secondaryModel = screen.getByRole("checkbox", {
      name: "glm-5.2:cloud",
    })
    fireEvent.click(secondaryModel)
    expect(screen.getByRole("button", { name: "Save" })).toBeVisible()

    fireEvent.click(secondaryModel)
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull()
    fireEvent.click(
      screen.getByRole("button", { name: "Back to Environments" }),
    )
    expect(screen.queryByText("Discard unsaved changes?")).toBeNull()
  })

  it("shows Secret names without exposing internal references", async () => {
    const secretRef = "secret_internal_123"
    renderEnvironments({
      ...baseProjection,
      secrets: [
        {
          secretRef,
          label: "Team API key",
          kind: "api_token",
          referencedBy: [],
          createdAtUnixMs: 1,
          updatedAtUnixMs: 1,
        },
      ],
      environments: [
        environment({
          environmentVariables: [
            { name: "API_KEY", source: "secret", value: secretRef },
          ],
        }),
      ],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    expect(screen.getByText("Team API key")).toBeInTheDocument()
    expect(screen.queryByText(secretRef)).toBeNull()
    fireEvent.click(
      screen.getByRole("button", { name: "Actions for API_KEY" }),
    )
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit" }))
    fireEvent.click(screen.getByRole("button", { name: "Secret value" }))

    expect(screen.getByRole("dialog", { name: "Choose secret" })).toBeVisible()
    expect(screen.getAllByText("Team API key").length).toBeGreaterThan(0)
    expect(screen.queryByText(secretRef)).toBeNull()
  })

  it("keeps removed provider models visible so the user can reconcile them", async () => {
    const affected = environment({
      llm: {
        providerId: provider.id,
        allowedModels: ["retired-model"],
        defaultModel: "retired-model",
      },
      resolvedLlm: null,
      llmNeedsSetup: true,
      readiness: {
        state: "needs_setup",
        issues: [
          "Environment model retired-model is not allowed by the Intelligence Provider",
        ],
      },
    })
    renderEnvironments({ ...baseProjection, environments: [affected] })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    const retired = screen.getByRole("checkbox", { name: /retired-model/ })
    expect(retired).toBeChecked()
    expect(screen.getByText("No longer allowed by provider")).toBeTruthy()
    fireEvent.click(screen.getByRole("checkbox", { name: "qwen3-coder" }))
    fireEvent.click(retired)

    await waitFor(() => {
      expect(screen.queryByText("No longer allowed by provider")).toBeNull()
      expect(screen.getByRole("button", { name: "Save" })).toBeEnabled()
    })
  })

  it("composes Intelligence Provider, Environment Variables, and Skills & Tools", () => {
    renderEnvironments({ ...baseProjection, environments: [environment()] })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    expect(screen.getByText("Intelligence Provider")).toBeTruthy()
    expect(screen.getByText("Environment Variables")).toBeTruthy()
    expect(screen.getByText("Skills & Tools")).toBeTruthy()
  })

  it("creates a complete authored draft through one intent", async () => {
    const { client } = renderEnvironments(baseProjection, { createRequested: true })
    await waitFor(() => expect(screen.getByLabelText("Name")).toBeTruthy())
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Coding" },
    })

    // A provider is selected before the authored boundary can be created.
    fireEvent.click(screen.getByLabelText("Intelligence Provider"))
    fireEvent.click(
      await screen.findByRole("option", { name: "Shared Ollama" }),
    )
    fireEvent.click(screen.getByRole("button", { name: "Add" }))

    await waitFor(() =>
      expect(
        client.intents.find((intent) => intent.type === "environment.create"),
      ).toMatchObject({
        type: "environment.create",
        configuration: {
          name: "Coding",
          llm: { providerId: provider.id },
        },
      }),
    )
  })

  it("constrains Available models to a scrollable area", async () => {
    const models = Array.from({ length: 12 }, (_, index) => `model-${index}`)
    renderEnvironments({
      ...baseProjection,
      llmProviders: [{ ...provider, allowedModels: models }],
      environments: [
        environment({
          llm: {
            providerId: provider.id,
            allowedModels: models,
            defaultModel: models[0] ?? "",
          },
        }),
      ],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    const modelList = await screen.findByTestId("environment-model-list")
    expect(modelList).toHaveClass("max-h-56")
    expect(
      modelList.querySelector('[data-slot="scroll-area-viewport"]'),
    ).toHaveClass("max-h-56")
    expect(modelList.querySelector(".py-4")).not.toBeNull()
    expect(screen.getByRole("checkbox", { name: "model-11" })).toBeVisible()
  })

  it("loads installed plugin skills and tools for selection", async () => {
    const { client } = renderEnvironments({
      ...baseProjection,
      environments: [environment()],
      plugins: {
        installed: [
          {
            name: "factory-tools",
            activeVersion: "1.0.0",
            skills: [{ name: "review", description: "Review code" }],
            mcpServers: [{ name: "repo", kind: "stdio" }],
          },
        ],
        localMcpServers: [],
      },
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    await waitFor(() =>
      expect(client.intents).toContainEqual({ type: "plugin.list" }),
    )
    expect(screen.getByText("factory-tools")).toBeVisible()
    fireEvent.click(screen.getByRole("button", { name: /factory-tools/ }))
    expect(screen.getByRole("switch", { name: "review" })).toBeInTheDocument()
    expect(screen.getByRole("switch", { name: "repo" })).toBeInTheDocument()
    expect(screen.queryByText("No plugins installed")).toBeNull()
  })

  it("shows the Skills & Tools empty state only after plugins have been listed", async () => {
    const { client } = renderEnvironments({
      ...baseProjection,
      environments: [environment()],
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Local" }))
    expect(screen.queryByText("No plugins installed")).toBeNull()
    await waitFor(() =>
      expect(client.intents).toContainEqual({ type: "plugin.list" }),
    )
    await waitFor(() =>
      expect(screen.getByText("No plugins installed")).toBeVisible(),
    )
  })
})
