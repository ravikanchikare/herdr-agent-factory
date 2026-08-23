import * as React from "react"
import {
  AppWindowIcon,
  FolderIcon,
  FolderOpenIcon,
  PlusIcon,
  SettingsIcon,
  XIcon,
} from "lucide-react"

import type {
  TargetAgentProjection,
  TargetAgentVersionProjection,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"
import { Button } from "@agent-factory/ui/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@agent-factory/ui/components/collapsible"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@agent-factory/ui/components/context-menu"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@agent-factory/ui/components/sidebar"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-factory/ui/components/tooltip"

import { openDraftInNewWindow } from "@/lib/draft-window"

export function AgentFactorySidebar({
  projection,
  onAddTarget,
  onOpenSettings,
  onOpenDraft,
  onCreateDraft,
  onRemoveAgent,
}: {
  projection: WorkspaceProjection
  onAddTarget: () => void
  onOpenSettings: () => void
  onOpenDraft: (
    targetAgentId: string,
    workspaceBindingId: string,
    draftId?: string,
  ) => void
  onCreateDraft: (
    agent: TargetAgentProjection,
    version?: TargetAgentVersionProjection,
  ) => void
  onRemoveAgent: (targetAgentId: string) => void
}) {
  const focusedContext = focusedWorkContext(projection)

  return (
    <Sidebar collapsible="offcanvas">
        <nav aria-label="Agents" className="contents">
        <SidebarHeader
          data-native-drag-region
          className="h-11 min-h-11 shrink-0"
          aria-hidden="true"
        />
        <SidebarContent>
          <SidebarGroup className="pt-0">
            <SidebarGroupLabel className="gap-1">
              <span className="font-medium">Agents</span>
              <Tooltip>
                <TooltipTrigger
                  render={
                    <Button
                      className="ml-auto"
                      variant="ghost"
                      size="icon-sm"
                      aria-label="Create Agent"
                      disabled={projection.connection !== "ready"}
                      onClick={onAddTarget}
                    />
                  }
                >
                  <PlusIcon />
                </TooltipTrigger>
                <TooltipContent side="bottom">Create Agent</TooltipContent>
              </Tooltip>
            </SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {projection.targetWorkspace.targetGroups.map((group) => {
                  const activeDrafts = group.drafts.filter(
                    (draft) => draft.lifecycle !== "archived",
                  )
                  const latestVersion = group.versions.toSorted((left, right) =>
                    right.version.localeCompare(left.version, undefined, {
                      numeric: true,
                    }),
                  )[0]
                  return (
                    <SidebarMenuItem key={group.targetAgent.id}>
                      <Collapsible defaultOpen>
                        <ContextMenu>
                          <ContextMenuTrigger
                            render={
                              <CollapsibleTrigger
                                aria-label={group.targetAgent.name}
                                render={
                                  <SidebarMenuButton
                                    className="font-medium"
                                  />
                                }
                              />
                            }
                          >
                            <FolderIcon
                              data-folder-state="closed"
                              className="in-data-panel-open:hidden"
                            />
                            <FolderOpenIcon
                              data-folder-state="open"
                              className="hidden in-data-panel-open:block"
                            />
                            <span>{group.targetAgent.name}</span>
                          </ContextMenuTrigger>
                          <ContextMenuContent>
                            <ContextMenuGroup>
                              <ContextMenuItem
                                onClick={() =>
                                  onCreateDraft(
                                    group.targetAgent,
                                    latestVersion,
                                  )}
                              >
                                <PlusIcon />
                                Create Draft
                              </ContextMenuItem>
                            </ContextMenuGroup>
                          </ContextMenuContent>
                        </ContextMenu>
                        <CollapsibleContent>
                          <SidebarMenu>
                            {activeDrafts.map((draft) => {
                              const draftBinding = group.workspaceBindings.find(
                                (candidate) =>
                                  candidate.id === draft.workspaceBindingId,
                              )
                              if (!draftBinding) return null
                              return (
                                <SidebarMenuItem
                                  key={draft.id}
                                  data-sidebar-entry="draft"
                                >
                                  <ContextMenu>
                                    <ContextMenuTrigger
                                      render={
                                        <SidebarMenuButton
                                          size="sm"
                                          className="pl-8 text-muted-foreground"
                                          isActive={
                                            focusedContext?.agentDraftId ===
                                              draft.id
                                          }
                                          tooltip={draft.branchRef}
                                          onClick={() =>
                                            onOpenDraft(
                                              group.targetAgent.id,
                                              draftBinding.id,
                                              draft.id,
                                            )
                                          }
                                        />
                                      }
                                    >
                                      <span>{draftBinding.name}</span>
                                    </ContextMenuTrigger>
                                    <ContextMenuContent>
                                      <ContextMenuGroup>
                                        <ContextMenuItem
                                          onClick={() =>
                                            void openDraftInNewWindow({
                                              draftId: draft.id,
                                              targetAgentId:
                                                group.targetAgent.id,
                                              workspaceBindingId:
                                                draftBinding.id,
                                              title:
                                                `${group.targetAgent.name} — ${draftBinding.name}`,
                                            })
                                          }
                                        >
                                          <AppWindowIcon />
                                          Open in new window
                                        </ContextMenuItem>
                                        <ContextMenuItem
                                          onClick={() =>
                                            onRemoveAgent(group.targetAgent.id)
                                          }
                                        >
                                          <XIcon />
                                          Remove
                                        </ContextMenuItem>
                                      </ContextMenuGroup>
                                    </ContextMenuContent>
                                  </ContextMenu>
                                </SidebarMenuItem>
                              )
                            })}
                          </SidebarMenu>
                        </CollapsibleContent>
                      </Collapsible>
                    </SidebarMenuItem>
                  )
                })}
                {projection.targetWorkspace.targetGroups.length === 0 ? (
                  <li className="px-2 py-6 text-center text-sm text-muted-foreground">
                    Create an Agent to begin.
                  </li>
                ) : null}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter className="border-t border-sidebar-border">
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton tooltip="Settings" onClick={onOpenSettings}>
                <SettingsIcon aria-hidden="true" />
                <span>Settings</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
        </nav>
    </Sidebar>
  )
}

function focusedWorkContext(projection: WorkspaceProjection) {
  const pane = projection.targetWorkspace.panes.find(
    (candidate) => candidate.id === projection.targetWorkspace.focusedPaneId,
  )
  return projection.targetWorkspace.workContexts.find(
    (context) => context.id === pane?.workContextId,
  )
}
