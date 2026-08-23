/** Ephemeral Version tab model for one Draft work context. */

export type VersionTabsState = {
  openIds: readonly string[]
  activeId: string | null
}

export const emptyVersionTabs: VersionTabsState = {
  openIds: [],
  activeId: null,
}

export function isVersionSurfaceVisible(state: VersionTabsState) {
  return state.openIds.length > 0
}

/** Open a Version, or activate it when it is already a tab. */
export function openVersionTab(
  state: VersionTabsState,
  versionId: string,
): VersionTabsState {
  if (state.openIds.includes(versionId)) {
    return { openIds: state.openIds, activeId: versionId }
  }
  return {
    openIds: [...state.openIds, versionId],
    activeId: versionId,
  }
}

export function activateVersionTab(
  state: VersionTabsState,
  versionId: string,
): VersionTabsState {
  if (!state.openIds.includes(versionId)) return state
  return { openIds: state.openIds, activeId: versionId }
}

/**
 * Close one Version tab. Closing the last tab hides the Version surface
 * without affecting Terminal.
 */
export function closeVersionTab(
  state: VersionTabsState,
  versionId: string,
): VersionTabsState {
  const index = state.openIds.indexOf(versionId)
  if (index < 0) return state
  const openIds = state.openIds.filter((id) => id !== versionId)
  if (openIds.length === 0) return emptyVersionTabs
  if (state.activeId !== versionId) {
    return { openIds, activeId: state.activeId }
  }
  const nextActive = openIds[index] ?? openIds[index - 1] ?? null
  return { openIds, activeId: nextActive }
}

export function closeVersionSurface(): VersionTabsState {
  return emptyVersionTabs
}

/** Drop tabs whose Version is no longer in the current projection. */
export function reconcileVersionTabs(
  state: VersionTabsState,
  availableIds: ReadonlySet<string>,
): VersionTabsState {
  const openIds = state.openIds.filter((id) => availableIds.has(id))
  if (openIds.length === state.openIds.length) {
    if (state.activeId && !openIds.includes(state.activeId)) {
      return {
        openIds,
        activeId: openIds[openIds.length - 1] ?? null,
      }
    }
    return state
  }
  if (openIds.length === 0) return emptyVersionTabs
  const activeId = state.activeId && openIds.includes(state.activeId)
    ? state.activeId
    : openIds[openIds.length - 1] ?? null
  return { openIds, activeId }
}
