# Plan: Unified two-mode Secrets (View + bulk Edit)

## Goal
Replace the current Secrets list with two modes: a read-only **View** mode and a single **Edit** mode that lets the user edit existing secrets, add new ones, and remove unused ones in one bulk operation. Remove row overflow menus; clicking any Secret row enters Edit. Rename the top-level entry action from **Create** to **Add**.

## Implementation

### `apps/web-ui/components/settings/secret-settings.tsx`
- Replace `SecretDraftEntry` with a discriminated `EditableSecretEntry`:
  - `existing`: carries `secretRef`, current `label`, empty `value` input, and `referencedBy`.
  - `new`: carries `label` and `value`.
- View mode (`selection === undefined`):
  - List secrets as clickable rows. No overflow menus.
  - Empty state action renamed to **Add**.
- Edit mode (`selection.kind === "draft"`):
  - Load all existing secrets as disabled-key rows plus one trailing empty `new` row.
  - Typing into the last empty row auto-appends another empty row.
  - Existing secret keys are disabled; their values can be replaced.
  - New rows can be removed with the `XIcon` without confirmation.
  - Unused existing rows can be removed with the `XIcon`, which opens a delete confirmation.
  - Referenced existing rows cannot be removed (remove button disabled).
- Save dispatches, in order inside one transition:
  1. `secret.create` for each valid new row.
  2. `secret.replace` for each existing row with a non-empty value.
  3. `secret.delete` for each secret marked for removal.
- Remove the old per-secret Edit `Dialog`.
- Keep the delete `AlertDialog`, now triggered from the bulk edit form.

### `apps/web-ui/components/settings/settings-view.tsx`
- Rename the Secrets page-header action to **Add**.
- Keep the existing `createRequested`/`selection` plumbing.

### `apps/web-ui/components/settings/secret-settings.test.tsx`
- Rewrite tests for the two-mode model:
  - View mode entry into Edit.
  - Creating a new secret in bulk mode.
  - Replacing an existing secret's value.
  - Mixed create + replace in one save.
  - Auto-appending empty rows.
  - Deleting an unused existing secret.
  - Guarding referenced secrets (no remove, disabled key edit).
  - Ignoring empty new rows.
  - Removing a new draft row.
  - Dirty state reporting.
  - List rendering and mount-time `secret.list`.

## Validation
- `pnpm exec vitest run components/settings/secret-settings.test.tsx`: 15/15 passing.
- `pnpm exec tsc --noEmit -p tsconfig.json`: clean.
- `pnpm exec eslint` on changed files: clean.
