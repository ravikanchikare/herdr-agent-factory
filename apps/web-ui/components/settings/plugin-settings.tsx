import * as React from "react"

import {
  CheckIcon,
  ChevronDownIcon,
  ExternalLinkIcon,
  FilterIcon,
  PackageOpenIcon,
  PuzzleIcon,
  SearchIcon,
  TerminalIcon,
} from "lucide-react"

import type {
  PluginDetailsDto,
  PluginListDto,
  RegistryCatalogDto,
  RuntimeIntent,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"
import {
  Alert,
  AlertDescription,
} from "@agent-factory/ui/components/alert"
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
import {
  Button,
  buttonVariants,
} from "@agent-factory/ui/components/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@agent-factory/ui/components/card"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@agent-factory/ui/components/collapsible"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@agent-factory/ui/components/dropdown-menu"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@agent-factory/ui/components/input-group"
import { Separator } from "@agent-factory/ui/components/separator"
import { Skeleton } from "@agent-factory/ui/components/skeleton"
import { Spinner } from "@agent-factory/ui/components/spinner"
import { cn } from "@agent-factory/ui/lib/utils"

import type { EmitIntent } from "@/components/shell/workspace-shell"
import {
  SettingsDetailsNavigation,
  SettingsEmpty,
  SettingsTab,
  SettingsTabPanel,
  SettingsTabs,
  useSettingsErrorToast,
} from "@/components/settings/settings-primitives"

type PluginIntent = Extract<
  RuntimeIntent,
  { type: `plugin.${string}` | `registry.${string}` }
>
type InstalledPlugin = PluginListDto["installed"][number]
type LocalMcpServer = PluginListDto["localMcpServers"][number]
type CatalogPlugin = RegistryCatalogDto["plugins"][number] & {
  registryId: string
}
type TabValue = "marketplace" | "yours"
type FilterValue = "all" | "available" | "installed"
type PendingAction = {
  key: string
  label: "install" | "uninstall" | "rollback" | "trust"
}
export type PluginSettingsSelection = {
  registryId?: string
  pluginId: string
}

type DisplayDetails = {
  name: string
  version: string
  description?: string | null
  authorName?: string | null
  sourceUrl?: string
  skills: PluginDetailsDto["skills"]
  mcpServers: PluginDetailsDto["mcpServers"]
  mcpDisabledReason?: string | null
}

export function PluginSettings({
  projection,
  emitIntent,
  selection,
  onSelectionChange,
}: {
  projection: WorkspaceProjection
  emitIntent: EmitIntent
  selection?: PluginSettingsSelection
  onSelectionChange: (selection?: PluginSettingsSelection) => void
}) {
  const [tab, setTab] = React.useState<TabValue>("marketplace")
  const [query, setQuery] = React.useState("")
  const [filter, setFilter] = React.useState<FilterValue>("all")
  const [pendingAction, setPendingAction] = React.useState<PendingAction>()
  const [confirmUninstall, setConfirmUninstall] = React.useState<string>()
  const [initialLoadPending, setInitialLoadPending] = React.useState(true)
  const refreshedRef = React.useRef<Set<string>>(new Set())

  const { installed, localMcpServers } = projection.plugins
  const installedByName = React.useMemo(
    () => new Map(installed.map((plugin) => [plugin.name, plugin])),
    [installed],
  )
  const catalogPlugins = React.useMemo(
    () => marketplacePlugins(projection.pluginCatalogs),
    [projection.pluginCatalogs],
  )
  const catalogByName = React.useMemo(
    () => new Map(catalogPlugins.map((plugin) => [plugin.name, plugin])),
    [catalogPlugins],
  )
  const deferredQuery = React.useDeferredValue(query)
  const normalizedQuery = deferredQuery.trim().toLocaleLowerCase()
  const visibleMarketplace = React.useMemo(
    () =>
      catalogPlugins.filter((plugin) => {
        const matchesQuery =
          normalizedQuery.length === 0 ||
          plugin.name.toLocaleLowerCase().includes(normalizedQuery) ||
          plugin.description?.toLocaleLowerCase().includes(normalizedQuery)
        const isInstalled = installedByName.has(plugin.name)
        const matchesFilter =
          filter === "all" ||
          (filter === "installed" && isInstalled) ||
          (filter === "available" && !isInstalled)
        return matchesQuery && matchesFilter
      }),
    [catalogPlugins, filter, installedByName, normalizedQuery],
  )
  const visibleInstalled = React.useMemo(
    () =>
      installed.filter(
        (plugin) =>
          normalizedQuery.length === 0 ||
          plugin.name.toLocaleLowerCase().includes(normalizedQuery),
      ),
    [installed, normalizedQuery],
  )

  // Rust owns plugin state. These effects only synchronize this mounted view
  // with that external system and request missing signed catalogs once.
  React.useEffect(() => {
    void Promise.all([
      emitIntent({ type: "registry.list" }),
      emitIntent({ type: "plugin.list" }),
    ]).finally(() => setInitialLoadPending(false))
  }, [emitIntent])

  React.useEffect(() => {
    if (initialLoadPending) return
    const loadedRegistryIds = new Set(
      projection.pluginCatalogs.map((catalog) => catalog.registryId),
    )
    const registriesToRefresh = projection.pluginRegistries.filter(
      (registry) =>
        !loadedRegistryIds.has(registry.id) &&
        !refreshedRef.current.has(registry.id),
    )
    if (registriesToRefresh.length === 0) return
    for (const registry of registriesToRefresh) {
      refreshedRef.current.add(registry.id)
      void emitIntent({
        type: "registry.refresh",
        registryId: registry.id,
      })
    }
  }, [
    emitIntent,
    initialLoadPending,
    projection.pluginCatalogs,
    projection.pluginRegistries,
  ])

  // Suppress noisy background registry fetch failures; show inline instead.
  // User-initiated actions (install/uninstall) still toast via pendingAction.
  const pluginErrorForToast = React.useMemo(() => {
    if (!projection.pluginError) return undefined
    if (
      projection.pluginError.toLowerCase().includes("registry download failed") &&
      !pendingAction
    ) {
      return undefined
    }
    return projection.pluginError
  }, [projection.pluginError, pendingAction])
  useSettingsErrorToast("Plugin operation failed", pluginErrorForToast)

  const runAction = React.useCallback(
    async (
      action: PendingAction,
      intent: PluginIntent,
      after?: () => void,
    ) => {
      setPendingAction(action)
      try {
        await emitIntent(intent)
        after?.()
      } finally {
        setPendingAction(undefined)
      }
    },
    [emitIntent],
  )

  const openCatalogDetails = React.useCallback(
    (plugin: CatalogPlugin) => {
      onSelectionChange({
        registryId: plugin.registryId,
        pluginId: plugin.id,
      })
      void emitIntent({
        type: "plugin.details",
        registryId: plugin.registryId,
        pluginId: plugin.id,
      })
    },
    [emitIntent, onSelectionChange],
  )

  const openInstalledDetails = React.useCallback(
    (plugin: InstalledPlugin) => {
      const catalogPlugin = catalogByName.get(plugin.name)
      if (catalogPlugin) {
        openCatalogDetails(catalogPlugin)
        return
      }
      onSelectionChange({
        pluginId: plugin.name,
      })
    },
    [catalogByName, onSelectionChange, openCatalogDetails],
  )

  const selectedCatalog = selection
    ? catalogPlugins.find(
        (plugin) =>
          plugin.registryId === selection.registryId &&
          plugin.id === selection.pluginId,
      )
    : undefined
  const selectedInstalled = selection
    ? installedByName.get(selectedCatalog?.name ?? selection.pluginId)
    : undefined
  const remoteDetails = detailsForSelection(
    projection.pluginDetails,
    selection,
  )
  const displayDetails = selection
    ? combineDetails(selectedCatalog, selectedInstalled, remoteDetails)
    : undefined

  if (selection && displayDetails) {
    return (
      <PluginDetailsView
        details={displayDetails}
        installed={selectedInstalled !== undefined}
        loading={
          selection.registryId !== undefined &&
          remoteDetails === undefined &&
          selectedInstalled === undefined &&
          projection.pluginError === undefined
        }
        pendingAction={pendingAction}
        onBack={() => onSelectionChange(undefined)}
        onInstall={
          selectedCatalog
            ? () =>
                void runAction(
                  {
                    key: selectedCatalog.name,
                    label: "install",
                  },
                  {
                    type: "plugin.install",
                    registryId: selectedCatalog.registryId,
                    pluginId: selectedCatalog.id,
                  },
                )
            : undefined
        }
        onRequestUninstall={() =>
          setConfirmUninstall(displayDetails.name)
        }
      >
        <UninstallDialog
          pluginName={confirmUninstall}
          onOpenChange={(open) => !open && setConfirmUninstall(undefined)}
          onConfirm={(pluginName) =>
            void runAction(
              { key: pluginName, label: "uninstall" },
              { type: "plugin.uninstall", pluginName },
              () => setConfirmUninstall(undefined),
            )
          }
        />
      </PluginDetailsView>
    )
  }

  const isCatalogLoading =
    initialLoadPending ||
    (
      projection.pluginError === undefined &&
      projection.pluginRegistries.some(
        (registry) =>
          !projection.pluginCatalogs.some(
            (catalog) => catalog.registryId === registry.id,
          ),
      )
    )

  return (
    <div className="flex flex-col gap-6">
      <SettingsTabs
        value={tab}
        onValueChange={(value) => setTab(value as TabValue)}
        className="flex flex-col gap-5"
        tabs={
          <>
            <SettingsTab value="marketplace">Marketplace</SettingsTab>
            <SettingsTab value="yours">Yours</SettingsTab>
          </>
        }
        action={
          <div className="flex min-w-0 flex-1 items-center justify-end gap-2">
            {tab === "marketplace" ? (
              <MarketplaceFilter value={filter} onValueChange={setFilter} />
            ) : null}
            <InputGroup className="w-full max-w-xs">
              <InputGroupAddon>
                <SearchIcon />
              </InputGroupAddon>
              <InputGroupInput
                value={query}
                aria-label="Search plugins"
                placeholder="Search plugins"
                onChange={(event) => setQuery(event.currentTarget.value)}
              />
            </InputGroup>
          </div>
        }
      >
        <SettingsTabPanel
          value="marketplace"
          className="flex flex-col gap-3"
        >
          <p className="text-xs text-muted-foreground">
            Discover signed plugins that add connectors and skills to your
            Environments.
          </p>
          {projection.pluginError &&
          catalogPlugins.length === 0 &&
          !isCatalogLoading ? (
            <Alert variant="destructive">
              <AlertDescription className="text-xs">
                Registry unavailable — {projection.pluginError}. Verify the
                registry URL and signature in your plugin configuration.
              </AlertDescription>
            </Alert>
          ) : null}
          {isCatalogLoading && catalogPlugins.length === 0 ? (
            <MarketplaceSkeleton />
          ) : visibleMarketplace.length === 0 ? (
            <SettingsEmpty
              icon={<PackageOpenIcon />}
              title={
                catalogPlugins.length === 0
                  ? "No plugins available"
                  : "No matching plugins"
              }
              description={
                catalogPlugins.length === 0
                  ? "The marketplace catalog is currently empty."
                  : "Try another search or filter."
              }
            />
          ) : (
            <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
              {visibleMarketplace.map((plugin) => (
                <MarketplaceCard
                  key={`${plugin.registryId}:${plugin.id}`}
                  plugin={plugin}
                  installed={installedByName.has(plugin.name)}
                  pending={
                    pendingAction?.key === plugin.name &&
                    pendingAction.label === "install"
                  }
                  onOpen={() => openCatalogDetails(plugin)}
                  onInstall={() =>
                    void runAction(
                      { key: plugin.name, label: "install" },
                      {
                        type: "plugin.install",
                        registryId: plugin.registryId,
                        pluginId: plugin.id,
                      },
                    )
                  }
                />
              ))}
            </div>
          )}
        </SettingsTabPanel>

        <SettingsTabPanel value="yours" className="flex flex-col gap-8">
          <div className="flex flex-col gap-3">
            <p className="text-xs text-muted-foreground">
              Installed plugins are available to every Environment and can be
              selected during Environment setup.
            </p>
            {visibleInstalled.length === 0 ? (
              <SettingsEmpty
                icon={<PuzzleIcon />}
                title={
                  installed.length === 0
                    ? "No plugins installed"
                    : "No matching plugins"
                }
                description={
                  installed.length === 0
                    ? "Install a plugin from Marketplace to see it here."
                    : "Try another search."
                }
              />
            ) : (
              <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
                {visibleInstalled.map((plugin) => (
                  <InstalledPluginCard
                    key={plugin.name}
                    plugin={plugin}
                    pendingAction={pendingAction}
                    onOpen={() => openInstalledDetails(plugin)}
                    onRollback={() =>
                      void runAction(
                        { key: plugin.name, label: "rollback" },
                        {
                          type: "plugin.rollback",
                          pluginName: plugin.name,
                        },
                      )
                    }
                    onRequestUninstall={() =>
                      setConfirmUninstall(plugin.name)
                    }
                  />
                ))}
              </div>
            )}
          </div>

          {localMcpServers.length > 0 ? (
            <LocalConnectorTrust
              servers={localMcpServers}
              pendingAction={pendingAction}
              onTrustChange={(server) =>
                void runAction(
                  {
                    key: localConnectorKey(server),
                    label: "trust",
                  },
                  {
                    type: server.trusted
                      ? "plugin.revokeLocalMcp"
                      : "plugin.trustLocalMcp",
                    environmentId: server.environmentId,
                    pluginName: server.pluginName,
                    serverName: server.serverName,
                    fingerprint: server.fingerprint,
                  },
                )
              }
            />
          ) : null}
        </SettingsTabPanel>
      </SettingsTabs>

      <UninstallDialog
        pluginName={confirmUninstall}
        onOpenChange={(open) => !open && setConfirmUninstall(undefined)}
        onConfirm={(pluginName) =>
          void runAction(
            { key: pluginName, label: "uninstall" },
            { type: "plugin.uninstall", pluginName },
            () => setConfirmUninstall(undefined),
          )
        }
      />
    </div>
  )
}

function MarketplaceFilter({
  value,
  onValueChange,
}: {
  value: FilterValue
  onValueChange: (value: FilterValue) => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            aria-label={`Filter marketplace: ${filterLabel(value)}`}
          />
        }
      >
        <FilterIcon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        <DropdownMenuGroup>
          <DropdownMenuRadioGroup
            value={value}
            onValueChange={(next) =>
              onValueChange(next as FilterValue)
            }
          >
            <DropdownMenuRadioItem value="all">
              All plugins
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="available">
              Available
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="installed">
              Installed
            </DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function MarketplaceCard({
  plugin,
  installed,
  pending,
  onOpen,
  onInstall,
}: {
  plugin: CatalogPlugin
  installed: boolean
  pending: boolean
  onOpen: () => void
  onInstall: () => void
}) {
  return (
    <Card
      size="sm"
      className="transition-colors hover:bg-accent/20"
      onClick={onOpen}
    >
      <CardHeader>
        <button
          type="button"
          className="flex min-w-0 items-start gap-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          aria-label={`Open ${plugin.name}`}
        >
          <PluginIcon installed={installed} />
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="flex min-w-0 items-baseline gap-2">
              <CardTitle className="truncate">{plugin.name}</CardTitle>
              <span className="shrink-0 text-xs text-muted-foreground">
                v{plugin.version}
              </span>
            </span>
            <CardDescription className="line-clamp-2">
              {plugin.description ?? "Agent Plugin package"}
            </CardDescription>
          </span>
        </button>
        <CardAction data-plugin-action>
          {installed ? (
            <Badge variant="secondary">
              <CheckIcon />
              Installed
            </Badge>
          ) : (
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={pending}
              aria-label={`Install ${plugin.name}`}
              onClick={(event) => {
                event.stopPropagation()
                onInstall()
              }}
            >
              {pending ? <Spinner data-icon="inline-start" /> : null}
              Install
            </Button>
          )}
        </CardAction>
      </CardHeader>
    </Card>
  )
}

function InstalledPluginCard({
  plugin,
  pendingAction,
  onOpen,
  onRollback,
  onRequestUninstall,
}: {
  plugin: InstalledPlugin
  pendingAction?: PendingAction
  onOpen: () => void
  onRollback: () => void
  onRequestUninstall: () => void
}) {
  const pending = pendingAction?.key === plugin.name
  const contribution = contributionLabel(
    plugin.mcpServers.length,
    plugin.skills.length,
  )
  return (
    <Card
      size="sm"
      className="transition-colors hover:bg-accent/20"
      onClick={onOpen}
    >
      <CardHeader>
        <button
          type="button"
          className="flex min-w-0 items-start gap-3 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          aria-label={`Open ${plugin.name}`}
        >
          <PluginIcon installed />
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="flex min-w-0 items-baseline gap-2">
              <CardTitle className="truncate">{plugin.name}</CardTitle>
              <span className="shrink-0 text-xs text-muted-foreground">
                v{plugin.activeVersion}
              </span>
            </span>
            <CardDescription>{contribution}</CardDescription>
          </span>
        </button>
        <CardAction
          data-plugin-action
          onClick={(event) => event.stopPropagation()}
        >
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  disabled={pending}
                  aria-label={`Actions for ${plugin.name}`}
                />
              }
            >
              {pending ? <Spinner /> : <ChevronDownIcon />}
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuGroup>
                {plugin.previousVersion ? (
                  <DropdownMenuItem onClick={onRollback}>
                    Roll back to {plugin.previousVersion}
                  </DropdownMenuItem>
                ) : null}
                <DropdownMenuItem
                  variant="destructive"
                  onClick={onRequestUninstall}
                >
                  Uninstall
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </CardAction>
      </CardHeader>
    </Card>
  )
}

function PluginDetailsView({
  details,
  installed,
  loading,
  pendingAction,
  onBack,
  onInstall,
  onRequestUninstall,
  children,
}: {
  details: DisplayDetails
  installed: boolean
  loading: boolean
  pendingAction?: PendingAction
  onBack: () => void
  onInstall?: () => void
  onRequestUninstall: () => void
  children: React.ReactNode
}) {
  const pending = pendingAction?.key === details.name
  return (
    <div className="flex flex-col gap-6">
      <SettingsDetailsNavigation
        parent="Plugins"
        current={details.name}
        onBack={onBack}
      />

      <div className="flex items-start justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-2">
          {/* Named the way Provider and Environment details are named. */}
          <h2 className="truncate text-lg font-semibold tracking-tight">
            {details.name}
          </h2>
          <p className="text-sm text-muted-foreground">
            {details.description ?? "Agent Plugin package"}
          </p>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs text-muted-foreground">
              Version {details.version}
            </span>
            {details.authorName ? (
              <span className="text-xs text-muted-foreground">
                by {details.authorName}
              </span>
            ) : null}
            {details.sourceUrl ? (
              <a
                className={cn(
                  buttonVariants({ variant: "link", size: "sm" }),
                  "h-auto px-0",
                )}
                href={details.sourceUrl}
                target="_blank"
                rel="noreferrer"
              >
                View Source
                <ExternalLinkIcon data-icon="inline-end" />
              </a>
            ) : null}
          </div>
        </div>
        {installed ? (
          <Button
            type="button"
            size="sm"
            variant="destructive"
            disabled={pending}
            onClick={onRequestUninstall}
          >
            {pending ? <Spinner data-icon="inline-start" /> : null}
            Uninstall
          </Button>
        ) : (
          <Button
            type="button"
            size="sm"
            disabled={pending || !onInstall}
            onClick={onInstall}
          >
            {pending ? <Spinner data-icon="inline-start" /> : null}
            Install
          </Button>
        )}
      </div>

      {loading ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Spinner />
          Inspecting the signed plugin package…
        </div>
      ) : (
        <div className="flex flex-col">
          <Separator />
          <PluginComponentSection
            title="Connectors"
            count={details.mcpServers.length}
            empty="This plugin does not include connectors."
          >
            {details.mcpDisabledReason ? (
              <Alert variant="destructive">
                <AlertDescription>
                  {details.mcpDisabledReason}
                </AlertDescription>
              </Alert>
            ) : (
              details.mcpServers.map((server) => (
                <ComponentRow
                  key={server.name}
                  name={server.name}
                  description={connectorKindLabel(server.kind)}
                />
              ))
            )}
          </PluginComponentSection>

          <Separator />
          <PluginComponentSection
            title="Skills"
            count={details.skills.length}
            empty="This plugin does not include skills."
          >
            {details.skills.map((skill) => (
              <ComponentRow
                key={skill.name}
                name={skill.name}
                description={skill.description}
              />
            ))}
          </PluginComponentSection>
          <Separator />
        </div>
      )}
      {children}
    </div>
  )
}

function PluginComponentSection({
  title,
  count,
  empty,
  children,
}: {
  title: string
  count: number
  empty: string
  children: React.ReactNode
}) {
  return (
    <Collapsible defaultOpen>
      <section>
        <CollapsibleTrigger
          render={
            <button
              type="button"
              aria-label={`${title}, ${count}`}
              className="flex w-full items-center gap-2 py-3 text-left outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/30"
            />
          }
        >
          {/* The count belongs to the label it counts, so it sits next to it
              rather than adrift at the far edge. */}
          <span className="text-sm font-medium">{title}</span>
          <span className="text-xs text-muted-foreground">{count}</span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="flex flex-col gap-3 pb-4">
            {count === 0 ? (
              <p className="text-xs text-muted-foreground">{empty}</p>
            ) : (
              children
            )}
          </div>
        </CollapsibleContent>
      </section>
    </Collapsible>
  )
}

function ComponentRow({
  name,
  description,
}: {
  name: string
  description?: string | null
}) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 px-2">
      <p className="text-xs font-medium">{name}</p>
      {description ? (
        <p className="line-clamp-2 text-xs text-muted-foreground">
          {description}
        </p>
      ) : null}
    </div>
  )
}

function LocalConnectorTrust({
  servers,
  pendingAction,
  onTrustChange,
}: {
  servers: LocalMcpServer[]
  pendingAction?: PendingAction
  onTrustChange: (server: LocalMcpServer) => void
}) {
  return (
    <section>
      <Card>
        <CardHeader>
          <CardTitle>Local connector trust</CardTitle>
          <CardDescription>
          Executable connectors remain blocked until their exact launch
          command is trusted for an Environment.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-0 py-1">
          {servers.map((server) => {
            const key = localConnectorKey(server)
            const pending = pendingAction?.key === key
            return (
              <div
                key={key}
                className="flex items-center justify-between gap-4 border-b py-3 last:border-b-0"
              >
                <div className="flex min-w-0 items-center gap-3">
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
                    <TerminalIcon />
                  </span>
                  <div className="flex min-w-0 flex-col gap-0.5">
                    <p className="truncate text-xs font-medium">
                      {server.pluginName} / {server.serverName}
                    </p>
                    <p className="truncate text-xs text-muted-foreground">
                      {server.command} {server.args.join(" ")}
                    </p>
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <Badge
                    variant={server.trusted ? "secondary" : "destructive"}
                  >
                    {server.trusted ? "Trusted" : "Blocked"}
                  </Badge>
                  <Button
                    type="button"
                    size="sm"
                    variant={server.trusted ? "outline" : "destructive"}
                    disabled={pending}
                    onClick={() => onTrustChange(server)}
                  >
                    {pending ? <Spinner data-icon="inline-start" /> : null}
                    {server.trusted ? "Revoke trust" : "Trust"}
                  </Button>
                </div>
              </div>
            )
          })}
        </CardContent>
      </Card>
    </section>
  )
}

function UninstallDialog({
  pluginName,
  onOpenChange,
  onConfirm,
}: {
  pluginName?: string
  onOpenChange: (open: boolean) => void
  onConfirm: (pluginName: string) => void
}) {
  return (
    <AlertDialog
      open={pluginName !== undefined}
      onOpenChange={onOpenChange}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Uninstall this plugin?</AlertDialogTitle>
          <AlertDialogDescription>
            {pluginName} will be removed from Agent Factory and every
            Environment that uses it. Plugin data is retained.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Keep plugin</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={() => pluginName && onConfirm(pluginName)}
          >
            Uninstall
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function MarketplaceSkeleton() {
  return (
    <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
      {[0, 1, 2, 3].map((index) => (
        <Card key={index} size="sm">
          <CardHeader>
            <div className="flex items-center gap-3">
              <Skeleton className="size-9" />
              <div className="flex flex-1 flex-col gap-2">
                <Skeleton className="h-3 w-1/3" />
                <Skeleton className="h-3 w-4/5" />
              </div>
            </div>
          </CardHeader>
          <CardFooter>
            <Skeleton className="h-3 w-1/4" />
          </CardFooter>
        </Card>
      ))}
    </div>
  )
}

function PluginIcon({ installed }: { installed: boolean }) {
  return (
    <span
      className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"
    >
      {installed ? <CheckIcon /> : <PuzzleIcon />}
    </span>
  )
}

function marketplacePlugins(
  catalogs: readonly RegistryCatalogDto[],
): CatalogPlugin[] {
  const seen = new Set<string>()
  const plugins: CatalogPlugin[] = []
  for (const catalog of catalogs) {
    for (const plugin of catalog.plugins) {
      if (seen.has(plugin.name)) continue
      seen.add(plugin.name)
      plugins.push({ ...plugin, registryId: catalog.registryId })
    }
  }
  return plugins.sort((left, right) => left.name.localeCompare(right.name))
}

function detailsForSelection(
  details: PluginDetailsDto | undefined,
  selected: PluginSettingsSelection | undefined,
): PluginDetailsDto | undefined {
  if (!details || !selected?.registryId) return undefined
  return details.registryId === selected.registryId &&
    details.pluginId === selected.pluginId
    ? details
    : undefined
}

function combineDetails(
  catalog: CatalogPlugin | undefined,
  installed: InstalledPlugin | undefined,
  remote: PluginDetailsDto | undefined,
): DisplayDetails | undefined {
  if (remote) {
    return {
      name: remote.name,
      version: installed?.activeVersion ?? remote.version,
      description: remote.description,
      authorName: remote.authorName,
      sourceUrl: remote.sourceUrl,
      skills: installed?.skills ?? remote.skills,
      mcpServers: installed?.mcpServers ?? remote.mcpServers,
      mcpDisabledReason:
        installed?.mcpDisabledReason ?? remote.mcpDisabledReason,
    }
  }
  if (installed) {
    return {
      name: installed.name,
      version: installed.activeVersion,
      description: catalog?.description,
      sourceUrl: catalog?.sourceUrl,
      skills: installed.skills,
      mcpServers: installed.mcpServers,
      mcpDisabledReason: installed.mcpDisabledReason,
    }
  }
  if (catalog) {
    return {
      name: catalog.name,
      version: catalog.version,
      description: catalog.description,
      sourceUrl: catalog.sourceUrl,
      skills: [],
      mcpServers: [],
    }
  }
  return undefined
}

function contributionLabel(connectors: number, skills: number): string {
  if (connectors === 0 && skills === 0) return "No connectors or skills"
  return `${countLabel(connectors, "connector")} · ${countLabel(
    skills,
    "skill",
  )}`
}

function countLabel(count: number, singular: string): string {
  return `${count} ${singular}${count === 1 ? "" : "s"}`
}

function connectorKindLabel(
  kind: PluginDetailsDto["mcpServers"][number]["kind"],
): string {
  if (kind === "streamableHttp") return "Streamable HTTP"
  if (kind === "sse") return "Server-sent events"
  return "Local executable"
}

function filterLabel(value: FilterValue): string {
  if (value === "available") return "Available"
  if (value === "installed") return "Installed"
  return "All plugins"
}

function localConnectorKey(server: LocalMcpServer): string {
  return `${server.environmentId}:${server.pluginName}:${server.serverName}`
}
