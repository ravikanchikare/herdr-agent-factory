"use client"

import * as React from "react"

import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react"
import { describe, expect, it } from "vitest"

import type {
  PluginDetailsDto,
  RuntimeIntent,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"

import { PluginSettings } from "@/components/settings/plugin-settings"

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
  pluginRegistries: [
    {
      id: "official",
      catalogUrl: "https://example.invalid/catalog.json",
      signatureUrl: "https://example.invalid/catalog.sig",
      publicKeyBase64: "AAAA",
    },
  ],
  pluginCatalogs: [
    {
      registryId: "official",
      generatedAt: "2026-08-11T00:00:00Z",
      plugins: [
        {
          id: "observability",
          name: "observability",
          version: "1.0.0",
          description: "Read traces and logs for a running environment.",
          sourceUrl:
            "https://github.com/ravikanchikare/desktop-shell-plugins/tree/main/plugins/observability",
        },
        {
          id: "quality-tools",
          name: "quality-tools",
          version: "2.1.0",
          description: "Review source changes before release.",
          sourceUrl: "https://example.invalid/quality-tools",
        },
      ],
    },
  ],
  plugins: { installed: [], localMcpServers: [] },
}

const detailsFixture: PluginDetailsDto = {
  registryId: "official",
  pluginId: "observability",
  name: "observability",
  version: "1.0.0",
  description: "Read traces and logs for a running environment.",
  authorName: "Ravi Kanchikare",
  sourceUrl:
    "https://github.com/ravikanchikare/desktop-shell-plugins/tree/main/plugins/observability",
  skills: [
    {
      name: "trace-reader",
      description: "Read and explain trace output.",
    },
  ],
  mcpServers: [{ name: "httpbin", kind: "streamableHttp" }],
  mcpDisabledReason: null,
}

function renderPluginSettings(
  projection: WorkspaceProjection = baseProjection,
) {
  const emitted: RuntimeIntent[] = []
  const emitIntent = async (intent: RuntimeIntent) => {
    emitted.push(intent)
  }
  const view = render(
    <PluginSettingsHarness
      projection={projection}
      emitIntent={emitIntent}
    />,
  )
  return { ...view, emitted, emitIntent }
}

function PluginSettingsHarness({
  projection,
  emitIntent,
}: {
  projection: WorkspaceProjection
  emitIntent: (intent: RuntimeIntent) => Promise<void>
}) {
  const [selection, setSelection] = React.useState<
    React.ComponentProps<typeof PluginSettings>["selection"]
  >()
  return (
    <PluginSettings
      projection={projection}
      emitIntent={emitIntent}
      selection={selection}
      onSelectionChange={setSelection}
    />
  )
}

describe("PluginSettings", () => {
  it("presents Marketplace and Yours without registry management", () => {
    renderPluginSettings()

    const marketplaceTab = screen.getByRole("tab", { name: "Marketplace" })
    const tabsHeader = marketplaceTab.closest(
      '[data-slot="settings-tabs-header"]',
    )

    expect(marketplaceTab).toBeVisible()
    expect(screen.getByRole("tab", { name: "Yours" })).toBeVisible()
    expect(tabsHeader).toContainElement(screen.getByLabelText("Search plugins"))
    expect(tabsHeader).toContainElement(
      screen.getByRole("button", {
        name: "Filter marketplace: All plugins",
      }),
    )
    expect(
      within(screen.getByRole("button", { name: "Open observability" }))
        .getByText("v1.0.0"),
    ).toBeVisible()
    expect(screen.queryByText("View details")).not.toBeInTheDocument()
    expect(screen.queryByText("Registries")).not.toBeInTheDocument()
    expect(
      screen.queryByRole("button", { name: /registry/i }),
    ).not.toBeInTheDocument()
    expect(screen.queryByText(/catalog url/i)).not.toBeInTheDocument()
  })

  it("searches and filters marketplace plugins", () => {
    renderPluginSettings({
      ...baseProjection,
      plugins: {
        installed: [
          {
            name: "observability",
            activeVersion: "1.0.0",
            previousVersion: null,
            skills: [],
            mcpServers: [],
            mcpDisabledReason: null,
          },
        ],
        localMcpServers: [],
      },
    })

    fireEvent.change(screen.getByLabelText("Search plugins"), {
      target: { value: "quality" },
    })
    expect(screen.getByText("quality-tools")).toBeVisible()
    expect(screen.queryByText("observability")).not.toBeInTheDocument()

    fireEvent.change(screen.getByLabelText("Search plugins"), {
      target: { value: "" },
    })
    fireEvent.click(
      screen.getByRole("button", {
        name: "Filter marketplace: All plugins",
      }),
    )
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Installed" }))

    expect(screen.getByText("observability")).toBeVisible()
    expect(screen.queryByText("quality-tools")).not.toBeInTheDocument()
  })

  it("installs a marketplace plugin", async () => {
    const { emitted } = renderPluginSettings()

    fireEvent.click(
      screen.getByRole("button", { name: "Install observability" }),
    )

    await waitFor(() => {
      expect(emitted).toContainEqual({
        type: "plugin.install",
        registryId: "official",
        pluginId: "observability",
      })
    })
  })

  it("loads a signed package inspection for plugin details", async () => {
    const { emitted, rerender, emitIntent } = renderPluginSettings()

    fireEvent.click(
      screen.getByText("Read traces and logs for a running environment."),
    )

    await waitFor(() => {
      expect(emitted).toContainEqual({
        type: "plugin.details",
        registryId: "official",
        pluginId: "observability",
      })
    })

    rerender(
      <PluginSettingsHarness
        projection={{ ...baseProjection, pluginDetails: detailsFixture }}
        emitIntent={emitIntent}
      />,
    )

    // Plugins leaves its detail the same way Providers and Environments do.
    expect(screen.getByRole("button", { name: "Back to Plugins" })).toBeVisible()
    const breadcrumb = screen.getByRole("navigation", {
      name: "Breadcrumb",
    })
    expect(
      within(breadcrumb).getByRole("button", { name: "Plugins" }),
    ).toBeVisible()
    expect(
      within(breadcrumb).queryByRole("button", { name: "observability" }),
    ).not.toBeInTheDocument()
    expect(within(breadcrumb).getByText("observability")).toHaveAttribute(
      "aria-current",
      "page",
    )
    expect(
      screen.getByRole("link", { name: "View Source" }),
    ).toHaveAttribute("href", detailsFixture.sourceUrl)
    expect(screen.getByText("httpbin")).toBeVisible()
    expect(screen.getByText("trace-reader")).toBeVisible()
    expect(screen.getByText(/Ravi Kanchikare/)).toBeVisible()

    const connectors = screen.getByRole("button", {
      name: "Connectors, 1",
    })
    expect(connectors.closest('[data-slot="card"]')).toBeNull()
    fireEvent.click(connectors)
    expect(screen.queryByText("httpbin")).not.toBeInTheDocument()
    fireEvent.click(connectors)
    expect(screen.getByText("httpbin")).toBeVisible()

    fireEvent.click(within(breadcrumb).getByRole("button", { name: "Plugins" }))
    expect(screen.getByRole("tab", { name: "Marketplace" })).toBeVisible()
  })

  it("shows installed plugins in Yours and uninstalls after confirmation", async () => {
    const { emitted } = renderPluginSettings({
      ...baseProjection,
      plugins: {
        installed: [
          {
            name: "observability",
            activeVersion: "1.0.0",
            previousVersion: null,
            skills: [
              {
                name: "trace-reader",
                description: "Read and explain trace output.",
              },
            ],
            mcpServers: [{ name: "httpbin", kind: "streamableHttp" }],
            mcpDisabledReason: null,
          },
        ],
        localMcpServers: [],
      },
    })

    fireEvent.click(screen.getByRole("tab", { name: "Yours" }))
    expect(screen.getByText("1 connector · 1 skill")).toBeVisible()

    fireEvent.click(
      screen.getByRole("button", { name: "Actions for observability" }),
    )
    fireEvent.click(screen.getByRole("menuitem", { name: "Uninstall" }))
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "Plugin data is retained",
    )
    fireEvent.click(screen.getByRole("button", { name: "Uninstall" }))

    await waitFor(() => {
      expect(emitted).toContainEqual({
        type: "plugin.uninstall",
        pluginName: "observability",
      })
    })
  })

  it("keeps local connector trust in Yours", async () => {
    const { emitted } = renderPluginSettings({
      ...baseProjection,
      plugins: {
        installed: [],
        localMcpServers: [
          {
            environmentId: "production",
            pluginName: "observability",
            serverName: "collector",
            command: "/usr/bin/collector",
            args: ["serve"],
            cwd: "/tmp",
            environmentKeys: [],
            trustClass: "pathExecutable",
            fingerprint: "a".repeat(64),
            trusted: false,
          },
        ],
      },
    })

    fireEvent.click(screen.getByRole("tab", { name: "Yours" }))
    fireEvent.click(screen.getByRole("button", { name: "Trust" }))

    await waitFor(() => {
      expect(emitted).toContainEqual({
        type: "plugin.trustLocalMcp",
        environmentId: "production",
        pluginName: "observability",
        serverName: "collector",
        fingerprint: "a".repeat(64),
      })
    })
  })
})
