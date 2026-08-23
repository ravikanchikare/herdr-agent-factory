import * as React from "react"

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

/// Guards a navigation that would throw away unsaved edits.
///
/// The guarded action is held rather than run: `request` stores it, and it only
/// executes once the user confirms. That ordering is what makes the dialog a
/// guard instead of an apology.
export function useUnsavedChangesGuard(isDirty: boolean) {
  const [pending, setPending] = React.useState<(() => void) | null>(null)

  const request = React.useCallback(
    (action: () => void) => {
      if (!isDirty) {
        action()
        return
      }
      // Stored in a thunk: a bare function passed to a state setter would be
      // called as an updater instead of being kept.
      setPending(() => action)
    },
    [isDirty],
  )

  const confirm = React.useCallback(() => {
    pending?.()
    setPending(null)
  }, [pending])

  const cancel = React.useCallback(() => setPending(null), [])

  return { request, confirm, cancel, isOpen: pending !== null }
}

export function UnsavedChangesDialog({
  open,
  onConfirm,
  onCancel,
  description = "Your unsaved changes to this Environment will be lost.",
}: {
  open: boolean
  onConfirm: () => void
  onCancel: () => void
  description?: string
}) {
  return (
    <AlertDialog open={open} onOpenChange={(next) => !next && onCancel()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Discard unsaved changes?</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={onCancel}>Keep editing</AlertDialogCancel>
          <AlertDialogAction variant="destructive" onClick={onConfirm}>
            Discard
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
