import * as React from "react"

import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import type { WorkspaceProjection } from "@agent-factory/runtime-client"
import { TestRuntimeClient } from "@agent-factory/runtime-client/testing"

import {
  LlmProviderSettings,
  type LlmProviderSettingsSelection,
} from "@/components/settings/llm-provider-settings"

const providerId = "00000000-0000-4000-8000-000000000010"
const provider = {
  id: providerId,
  name: "Team LiteLLM",
  type: "litellm" as const,
  endpoint: "https://gateway.example.com/v1",
  credentialRef: "secret_team",
  allowedModels: ["coding", "evaluation"],
  readiness: { state: "ready" as const, issues: [] },
}

const projection: WorkspaceProjection = {
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
  environments: [
    environment("coding", "Coding"),
    environment("evaluation", "Evaluation"),
  ],
  secrets: [
    {
      secretRef: "secret_team",
      label: "Team key",
      kind: "api_token",
      referencedBy: [],
      createdAtUnixMs: 1,
      updatedAtUnixMs: 1,
    },
  ],
  pluginRegistries: [],
  pluginCatalogs: [],
  plugins: { installed: [], localMcpServers: [] },
}

function environment(id: string, name: string) {
  return {
    id,
    name,
    codingHarnessId: "claude",
    evaluationHarnessId: "claude",
    plugins: [],
    permissions: {
      trustedRead: "allow" as const,
      trustedWrite: "ask" as const,
      terminal: "ask" as const,
    },
    registryIds: [],
    environmentVariables: [],
    llm: { providerId, allowedModels: ["coding"], defaultModel: "coding" },
    resolvedLlm: null,
    llmNeedsSetup: false,
    readiness: { state: "ready" as const, issues: [] },
  }
}

function renderProviders(overrides: Partial<WorkspaceProjection> = {}) {
  const current = { ...projection, ...overrides }
  const client = new TestRuntimeClient(current)
  return {
    client,
    ...render(
      <LlmProviderSettingsHarness
        projection={current}
        emitIntent={(intent) => client.dispatch(intent)}
        createRequested={false}
        onCreateRequestHandled={() => {}}
      />,
    ),
  }
}

function LlmProviderSettingsHarness(
  props: Omit<
    React.ComponentProps<typeof LlmProviderSettings>,
    "selection" | "onSelectionChange"
  >,
) {
  const [selection, setSelection] =
    React.useState<LlmProviderSettingsSelection>()
  return (
    <LlmProviderSettings
      {...props}
      selection={selection}
      onSelectionChange={setSelection}
    />
  )
}

describe("Provider Settings", () => {
  it("saves a compatible endpoint change without a downstream warning", async () => {
    const { client } = renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )
    await waitFor(() => expect(screen.getByLabelText("Endpoint")).toHaveValue(provider.endpoint))
    fireEvent.change(screen.getByLabelText("Endpoint"), {
      target: { value: "https://new-gateway.example.com/v1" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    expect(screen.queryByRole("alertdialog")).toBeNull()
    await waitFor(() =>
      expect(
        client.intents.find((i) => i.type === "llmProvider.configuration.set"),
      ).toMatchObject({
        type: "llmProvider.configuration.set",
        providerId,
        configuration: { endpoint: "https://new-gateway.example.com/v1" },
      }),
    )
  })

  it("saves an added provider model without warning compatible Environments", async () => {
    const { client } = renderProviders({
      llmProviderModelDiscovery: {
        providerKey: "litellm|https://gateway.example.com/v1|secret_team",
        models: ["coding", "evaluation", "new-model"],
      },
    })
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )
    const newModel = await screen.findByRole("checkbox", { name: "new-model" })
    fireEvent.click(newModel)
    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    expect(screen.queryByRole("alertdialog")).toBeNull()
    await waitFor(() =>
      expect(
        client.intents.some((i) => i.type === "llmProvider.configuration.set"),
      ).toBe(true),
    )
  })

  it("warns only for Environments that conflict with a removed model", async () => {
    renderProviders({
      environments: [
        environment("coding", "Coding"),
        {
          ...environment("evaluation", "Evaluation"),
          llm: {
            providerId,
            allowedModels: ["evaluation"],
            defaultModel: "evaluation",
          },
        },
      ],
    })
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )
    const evaluation = await screen.findByRole("checkbox", {
      name: "evaluation",
    })
    fireEvent.click(evaluation)
    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    const dialog = screen.getByRole("alertdialog")
    expect(dialog).toHaveTextContent("Evaluation")
    expect(dialog).not.toHaveTextContent("Coding")
  })

  it("shows the Secret name and puts model checkboxes before their labels", async () => {
    renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )

    await waitFor(() => {
      expect(screen.getByLabelText("Provider secret")).toHaveTextContent(
        "Team key",
      )
    })
    expect(screen.getByLabelText("Provider secret")).not.toHaveTextContent(
      "secret_team",
    )
    const coding = screen.getByRole("checkbox", { name: "coding" })
    expect(coding.parentElement?.firstElementChild).toBe(coding)
  })

  it("shows Select secret when the provider has no Secret", async () => {
    renderProviders({
      llmProviders: [{ ...provider, credentialRef: null }],
      secrets: [],
    })
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )

    await waitFor(() => {
      expect(screen.getByLabelText("Provider secret")).toHaveTextContent(
        "Select secret",
      )
    })
    expect(screen.getByLabelText("Provider secret")).not.toHaveTextContent(
      "__none__",
    )
  })

  it("constrains long model lists to a scrollable area", async () => {
    const models = Array.from({ length: 12 }, (_, index) => `model-${index}`)
    renderProviders({
      llmProviders: [
        {
          ...provider,
          allowedModels: models,
        },
      ],
    })
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )

    const modelList = await screen.findByTestId("provider-model-list")
    expect(modelList).toHaveClass("max-h-56")
    expect(modelList.querySelector('[data-slot="scroll-area-viewport"]')).toHaveClass(
      "max-h-56",
    )
    expect(modelList.querySelector(".py-4")).not.toBeNull()
    expect(screen.getByRole("checkbox", { name: "model-11" })).toBeVisible()
  })

  it("shows no action bar until the configuration actually changes", async () => {
    renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )

    expect(screen.queryByRole("button", { name: "Save" })).toBeNull()

    fireEvent.change(screen.getByLabelText("Endpoint"), {
      target: { value: "https://new-gateway.example.com/v1" },
    })
    const save = await screen.findByRole("button", { name: "Save" })
    expect(save.parentElement).toHaveClass("border-t")
  })

  it("returns to clean when the saved model set is restored", async () => {
    renderProviders({
      llmProviderModelDiscovery: {
        providerKey: "litellm|https://gateway.example.com/v1|secret_team",
        models: ["coding", "evaluation"],
      },
    })
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )

    const evaluation = await screen.findByRole("checkbox", {
      name: "evaluation",
    })
    fireEvent.click(evaluation)
    expect(screen.getByRole("button", { name: "Save" })).toBeVisible()

    fireEvent.click(screen.getByRole("checkbox", { name: "evaluation" }))
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Save" })).toBeNull(),
    )
    fireEvent.click(screen.getByRole("button", { name: "Back to Providers" }))
    expect(screen.queryByText("Discard unsaved changes?")).toBeNull()
  })

  it("offers creation actions without a separator before anything is typed", async () => {
    renderProviders({ llmProviders: [] })
    fireEvent.click(screen.getByRole("button", { name: "Add" }))

    const add = await screen.findByRole("button", { name: "Add" })
    expect(add.parentElement).not.toHaveClass("border-t")
    expect(add).toBeDisabled()
  })

  it("saves an inline rename when the name field loses focus", async () => {
    const { client } = renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )
    fireEvent.click(
      screen.getByRole("button", { name: "Rename Team LiteLLM" }),
    )
    await waitFor(() => expect(screen.getByLabelText("Name")).toHaveValue(provider.name))
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Renamed LiteLLM" },
    })
    fireEvent.blur(screen.getByLabelText("Name"))

    expect(screen.queryByRole("alertdialog")).toBeNull()
    await waitFor(() =>
      expect(
        client.intents.find((i) => i.type === "llmProvider.configuration.set"),
      ).toMatchObject({
        type: "llmProvider.configuration.set",
        configuration: { name: "Renamed LiteLLM" },
      }),
    )
  })

  it("leaves a rename to Save when another field is also edited", async () => {
    const { client } = renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )
    fireEvent.change(screen.getByLabelText("Endpoint"), {
      target: { value: "https://new-gateway.example.com/v1" },
    })
    fireEvent.click(
      screen.getByRole("button", { name: "Rename Team LiteLLM" }),
    )
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Renamed LiteLLM" },
    })
    fireEvent.blur(screen.getByLabelText("Name"))

    expect(
      client.intents.filter((i) => i.type === "llmProvider.configuration.set"),
    ).toHaveLength(0)
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    await waitFor(() =>
      expect(
        client.intents.find((i) => i.type === "llmProvider.configuration.set"),
      ).toMatchObject({
        type: "llmProvider.configuration.set",
        configuration: {
          name: "Renamed LiteLLM",
          endpoint: "https://new-gateway.example.com/v1",
        },
      }),
    )
  })

  it("keeps Rename out of the overflow menu", () => {
    renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )
    fireEvent.click(
      screen.getByRole("button", { name: "Actions for Team LiteLLM" }),
    )

    expect(screen.queryByRole("menuitem", { name: "Rename" })).toBeNull()
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument()
  })

  it("lists every unlink and retains the Secret in deletion copy", async () => {
    const { client } = renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )
    fireEvent.click(
      screen.getByRole("button", { name: "Actions for Team LiteLLM" }),
    )
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }))

    const dialog = screen.getByRole("alertdialog")
    expect(dialog.textContent).toContain("Coding")
    expect(dialog.textContent).toContain("Evaluation")
    expect(dialog.textContent).toContain("Secret is retained")
    fireEvent.click(screen.getByRole("button", { name: "Delete Provider" }))

    await waitFor(() => expect(client.intents).toContainEqual({
      type: "llmProvider.delete",
      providerId,
    }))
  })

  it("auto-discovers models when opening a fully-configured provider", async () => {
    const { client } = renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )

    await waitFor(() =>
      expect(
        client.intents.find((i) => i.type === "llmProvider.models.list"),
      ).toEqual({
        type: "llmProvider.models.list",
        providerId,
        provider: {
          type: "litellm",
          endpoint: provider.endpoint,
          credentialRef: "secret_team",
        },
      }),
    )
  })

  it("does not auto-discover until Type, Endpoint, and Secret are complete", async () => {
    const { client } = renderProviders({
      llmProviders: [{ ...provider, credentialRef: null }],
      secrets: [],
    })
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )

    // LiteLLM with no Secret is an incomplete connection: no discovery fires
    // and the manual Refresh fallback stays disabled.
    expect(
      client.intents.some((i) => i.type === "llmProvider.models.list"),
    ).toBe(false)
    expect(screen.getByRole("button", { name: "Refresh" })).toBeDisabled()
  })

  it("offers Refresh as a manual fallback that reloads the model list", async () => {
    const { client } = renderProviders()
    fireEvent.click(
      screen.getByRole("button", { name: "Open Team LiteLLM" }),
    )
    await waitFor(() =>
      expect(
        client.intents.some((i) => i.type === "llmProvider.models.list"),
      ).toBe(true),
    )
    const initial = client.intents.filter(
      (i) => i.type === "llmProvider.models.list",
    ).length

    const refresh = await screen.findByRole("button", { name: "Refresh" })
    await waitFor(() => expect(refresh).toBeEnabled())
    fireEvent.click(refresh)

    await waitFor(() =>
      expect(
        client.intents.filter((i) => i.type === "llmProvider.models.list")
          .length,
      ).toBe(initial + 1),
    )
  })
})
