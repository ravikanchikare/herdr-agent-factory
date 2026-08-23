import * as React from "react"

import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { WorkspaceProjection } from "@agent-factory/runtime-client"
import { TestRuntimeClient } from "@agent-factory/runtime-client/testing"

import {
  SecretSettings,
  type SecretSettingsSelection,
} from "@/components/settings/secret-settings"

const baseSecretsProjection: WorkspaceProjection = {
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

const sampleSecret = {
  secretRef: "secret_ollama_1",
  label: "OLLAMA_SECRET",
  kind: "api_token" as const,
  referencedBy: [
    {
      environmentId: "environment-1",
      environmentName: "Ollama",
      kind: "llm_provider" as const,
      label: "Intelligence Provider",
    },
  ],
  createdAtUnixMs: 1,
  updatedAtUnixMs: 1_000_000,
}

const unusedSecret = {
  secretRef: "secret_unused_2",
  label: "UNUSED_SECRET",
  kind: "api_token" as const,
  referencedBy: [],
  createdAtUnixMs: 2,
  updatedAtUnixMs: 2_000_000,
}

function SecretSettingsHarness(
  props: Omit<
    React.ComponentProps<typeof SecretSettings>,
    "selection" | "onSelectionChange"
  >,
) {
  const [selection, setSelection] = React.useState<SecretSettingsSelection>()
  return (
    <SecretSettings
      {...props}
      selection={selection}
      onSelectionChange={setSelection}
    />
  )
}

function renderSecrets(
  projection: WorkspaceProjection,
  options: {
    createRequested?: boolean
    onCreateRequestHandled?: () => void
    selection?: SecretSettingsSelection
    onSelectionChange?: (selection?: SecretSettingsSelection) => void
    onDirtyChange?: (dirty: boolean) => void
  } = {},
) {
  const client = new TestRuntimeClient(projection)
  const emitIntent = async (intent: Parameters<
    typeof client.dispatch
  >[0]) => {
    await client.dispatch(intent)
  }
  return {
    client,
    ...render(
      options.selection || options.onSelectionChange ? (
        <SecretSettings
          projection={projection}
          emitIntent={emitIntent}
          createRequested={options.createRequested ?? false}
          onCreateRequestHandled={options.onCreateRequestHandled ?? (() => {})}
          onDirtyChange={options.onDirtyChange}
          selection={options.selection}
          onSelectionChange={options.onSelectionChange ?? (() => {})}
        />
      ) : (
        <SecretSettingsHarness
          projection={projection}
          emitIntent={emitIntent}
          createRequested={options.createRequested ?? false}
          onCreateRequestHandled={options.onCreateRequestHandled ?? (() => {})}
          onDirtyChange={options.onDirtyChange}
        />
      ),
    ),
  }
}

describe("Secrets settings", () => {
  it("renders an empty state that opens the bulk edit form", () => {
    const onSelectionChange = vi.fn()
    renderSecrets(baseSecretsProjection, { onSelectionChange })

    expect(screen.getByText("No secrets saved")).toBeVisible()
    fireEvent.click(screen.getByRole("button", { name: "Add" }))
    expect(onSelectionChange).toHaveBeenCalledWith({ kind: "draft" })
  })

  it("enters the bulk edit form when create is requested", () => {
    renderSecrets(baseSecretsProjection, { createRequested: true })

    expect(screen.getByRole("heading", { name: "Edit Secrets" })).toBeVisible()
    expect(screen.getAllByLabelText(/Key [0-9]+$/i)).toHaveLength(1)
  })

  it("opens the bulk edit form by clicking a secret row", () => {
    const onSelectionChange = vi.fn()
    renderSecrets(
      { ...baseSecretsProjection, secrets: [unusedSecret] },
      { onSelectionChange },
    )

    fireEvent.click(screen.getByRole("button", { name: "Edit UNUSED_SECRET" }))
    expect(onSelectionChange).toHaveBeenCalledWith({ kind: "draft" })
  })

  it("creates a new secret in bulk edit mode", async () => {
    const { client } = renderSecrets(baseSecretsProjection, {
      selection: { kind: "draft" },
    })

    fireEvent.change(screen.getByLabelText("Key 1"), {
      target: { value: "NEW_SECRET" },
    })
    fireEvent.change(screen.getByLabelText("Value 1"), {
      target: { value: "hunter2" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() =>
      expect(client.intents).toContainEqual({
        type: "secret.create",
        label: "NEW_SECRET",
        value: "hunter2",
      }),
    )
  })

  it("replaces an existing secret's value in bulk edit mode", async () => {
    const { client } = renderSecrets(
      { ...baseSecretsProjection, secrets: [unusedSecret] },
      { selection: { kind: "draft" } },
    )

    const valueInput = screen.getByLabelText("Value 1")
    fireEvent.change(valueInput, { target: { value: "new-value" } })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() =>
      expect(client.intents).toContainEqual({
        type: "secret.replace",
        secretRef: unusedSecret.secretRef,
        value: "new-value",
      }),
    )
  })

  it("creates a new secret and replaces an existing one in one save", async () => {
    const { client } = renderSecrets(
      { ...baseSecretsProjection, secrets: [unusedSecret] },
      { selection: { kind: "draft" } },
    )

    fireEvent.change(screen.getByLabelText("Value 1"), {
      target: { value: "replaced-value" },
    })

    fireEvent.change(screen.getByLabelText("Key 2"), {
      target: { value: "ADDED_SECRET" },
    })
    fireEvent.change(screen.getByLabelText("Value 2"), {
      target: { value: "added-value" },
    })

    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() =>
      expect(client.intents).toEqual(
        expect.arrayContaining([
          {
            type: "secret.replace",
            secretRef: unusedSecret.secretRef,
            value: "replaced-value",
          },
          {
            type: "secret.create",
            label: "ADDED_SECRET",
            value: "added-value",
          },
        ]),
      ),
    )
    expect(
      client.intents.filter(
        (savedIntent) =>
          savedIntent.type === "secret.create" ||
          savedIntent.type === "secret.replace",
      ),
    ).toHaveLength(2)
  })

  it("keeps one empty row available while entering secrets", async () => {
    renderSecrets(baseSecretsProjection, { selection: { kind: "draft" } })

    fireEvent.change(screen.getByLabelText("Key 1"), {
      target: { value: "SECRET_ONE" },
    })
    await waitFor(() =>
      expect(screen.getAllByLabelText(/Key [0-9]+$/i)).toHaveLength(2),
    )

    fireEvent.change(screen.getAllByLabelText(/Key [0-9]+$/i)[1] as HTMLElement, {
      target: { value: "SECRET_TWO" },
    })
    fireEvent.change(screen.getAllByLabelText(/Value [0-9]+$/i)[1] as HTMLElement, {
      target: { value: "second-value" },
    })
    await waitFor(() =>
      expect(screen.getAllByLabelText(/Key [0-9]+$/i)).toHaveLength(3),
    )
  })

  it("deletes an unused existing secret in bulk edit mode", async () => {
    const { client } = renderSecrets(
      { ...baseSecretsProjection, secrets: [unusedSecret] },
      { selection: { kind: "draft" } },
    )

    fireEvent.click(screen.getByRole("button", { name: "Remove row 1" }))
    expect(
      screen.getByRole("alertdialog", { name: "Delete this secret?" }),
    ).toBeVisible()
    fireEvent.click(screen.getByRole("button", { name: "Delete" }))

    expect(screen.getAllByLabelText(/Key [0-9]+$/i)).toHaveLength(1)

    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() =>
      expect(client.intents).toContainEqual({
        type: "secret.delete",
        secretRef: unusedSecret.secretRef,
      }),
    )
  })

  it("disables removing a referenced existing secret", () => {
    renderSecrets(
      { ...baseSecretsProjection, secrets: [sampleSecret] },
      { selection: { kind: "draft" } },
    )

    expect(
      screen.getByRole("button", { name: "Remove row 1" }),
    ).toBeDisabled()
  })

  it("disables editing the key of an existing secret", () => {
    renderSecrets(
      { ...baseSecretsProjection, secrets: [unusedSecret] },
      { selection: { kind: "draft" } },
    )

    expect(screen.getByLabelText("Key 1")).toBeDisabled()
  })

  it("ignores empty new rows when saving", async () => {
    const { client } = renderSecrets(baseSecretsProjection, {
      selection: { kind: "draft" },
    })

    fireEvent.change(screen.getByLabelText("Key 1"), {
      target: { value: "FILLED_SECRET" },
    })
    fireEvent.change(screen.getByLabelText("Value 1"), {
      target: { value: "filled-value" },
    })

    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() =>
      expect(client.intents).toContainEqual({
        type: "secret.create",
        label: "FILLED_SECRET",
        value: "filled-value",
      }),
    )
    expect(
      client.intents.filter((intent) => intent.type === "secret.create"),
    ).toHaveLength(1)
  })

  it("removes a draft row and keeps existing rows", () => {
    renderSecrets(
      { ...baseSecretsProjection, secrets: [unusedSecret] },
      { selection: { kind: "draft" } },
    )

    fireEvent.change(screen.getByLabelText("Key 2"), {
      target: { value: "REMOVE_ME" },
    })
    expect(screen.getAllByLabelText(/Key [0-9]+$/i)).toHaveLength(3)

    fireEvent.click(screen.getByRole("button", { name: "Remove row 2" }))
    expect(screen.getAllByLabelText(/Key [0-9]+$/i)).toHaveLength(2)
    expect(screen.getByDisplayValue("UNUSED_SECRET")).toBeInTheDocument()
  })

  it("reports dirty state when any row has input", () => {
    const onDirtyChange = vi.fn()
    renderSecrets(baseSecretsProjection, {
      selection: { kind: "draft" },
      onDirtyChange,
    })

    expect(onDirtyChange).toHaveBeenLastCalledWith(false)
    fireEvent.change(screen.getByLabelText("Key 1"), {
      target: { value: "DIRTY" },
    })
    expect(onDirtyChange).toHaveBeenLastCalledWith(true)
  })

  it("lists secrets with labels, kinds, and usage", () => {
    renderSecrets({
      ...baseSecretsProjection,
      secrets: [sampleSecret, unusedSecret],
    })

    expect(screen.getByText("OLLAMA_SECRET")).toBeVisible()
    expect(screen.getByText("UNUSED_SECRET")).toBeVisible()
    expect(screen.getAllByText("API token").length).toBe(2)
    expect(screen.getByText(/Used by Ollama/)).toBeVisible()
    expect(screen.getByText("Not used by any Environment")).toBeVisible()
  })
})
