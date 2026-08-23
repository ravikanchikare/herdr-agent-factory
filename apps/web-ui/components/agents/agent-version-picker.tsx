import * as React from "react"
import { TagIcon } from "lucide-react"

import type { TargetAgentVersionProjection } from "@agent-factory/runtime-client"
import { Button } from "@agent-factory/ui/components/button"
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
  ComboboxValue,
} from "@agent-factory/ui/components/combobox"

export function sortAgentVersions(
  versions: readonly TargetAgentVersionProjection[],
) {
  return versions.toSorted((left, right) =>
    right.version.localeCompare(left.version, undefined, { numeric: true }),
  )
}

function versionLabel(version: TargetAgentVersionProjection): string {
  return `v${version.version}`
}

/**
 * Title-bar Version navigation: opens an immutable Version in the Draft
 * Versions surface. Always labels "Version" — never shows a selected
 * version, because the primary pane is the Draft (or empty Draft state).
 */
export function AgentVersionButtonGroup({
  versions,
  onOpenVersion,
}: {
  versions: readonly TargetAgentVersionProjection[]
  onOpenVersion: (version: TargetAgentVersionProjection) => void
}) {
  // Navigation only: never retain a selected Version on the Draft surface.
  const [value, setValue] = React.useState<TargetAgentVersionProjection | null>(
    null,
  )
  const sorted = sortAgentVersions(versions)
  const empty = sorted.length === 0

  return (
    <Combobox
      items={sorted}
      value={value}
      onValueChange={(next) => {
        if (next) {
          onOpenVersion(next)
        }
        setValue(null)
      }}
      itemToStringLabel={versionLabel}
      disabled={empty}
    >
      <ComboboxTrigger
        render={
          <Button
            type="button"
            size="sm"
            variant="secondary"
            data-native-no-drag
            aria-label="Open version selector"
            disabled={empty}
          />
        }
      >
        <ComboboxValue placeholder="Version" />
      </ComboboxTrigger>
      <ComboboxContent
        className="min-w-56"
        align="end"
        aria-label="Select version"
      >
        <ComboboxInput
          placeholder="Search versions…"
          showTrigger={false}
          aria-label="Search versions"
        />
        <ComboboxEmpty>No versions found.</ComboboxEmpty>
        <ComboboxList>
          {(version: TargetAgentVersionProjection) => (
            <ComboboxItem key={version.id} value={version}>
              <TagIcon aria-hidden="true" />
              <span>{versionLabel(version)}</span>
            </ComboboxItem>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}
