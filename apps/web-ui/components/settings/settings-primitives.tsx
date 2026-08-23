import * as React from "react"

import {
  ArrowLeftIcon,
  MoreHorizontalIcon,
  PencilIcon,
  PlusIcon,
} from "lucide-react"

import { Button } from "@agent-factory/ui/components/button"
import { Card, CardContent } from "@agent-factory/ui/components/card"
import { Input } from "@agent-factory/ui/components/input"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@agent-factory/ui/components/collapsible"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@agent-factory/ui/components/dropdown-menu"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@agent-factory/ui/components/empty"
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@agent-factory/ui/components/tabs"
import { toast } from "@agent-factory/ui/components/toast"
import { cn } from "@agent-factory/ui/lib/utils"

/// Settings speaks at three levels and each one has exactly one shape.
///
/// - Page: `SettingsPageHeader` — `text-lg` title and the page's single primary
///   action. Use Create for Factory-owned objects and Add for objects brought in
///   from another source. The title stands alone: no strapline.
/// - Section: `SettingsSection` — `text-sm font-medium` title, `text-xs`
///   description, and actions that belong to that section's own card.
/// - Group: `SettingsGroup` — a labelled cluster inside a card.
///
/// Lists of things are always `SettingsList` of `SettingsRow`: identity on the
/// left, state and actions on the right, destructive actions behind the row's
/// overflow menu and an alert dialog. A row that has a detail opens it from
/// anywhere on the row, and that detail is left through
/// `SettingsDetailsNavigation` — Back, then the breadcrumb back to the same
/// list. Failures are transient, so they are
/// toasts (`useSettingsErrorToast`) rather than banners stacked above the
/// content. Following these instead of hand-rolled markup is what keeps the
/// sections looking like one product.

export function SettingsPageHeader({
  title,
  navigation,
  action,
}: {
  title: string
  navigation?: React.ReactNode
  action?: React.ReactNode
}) {
  return (
    <div className="flex min-h-8 items-center justify-between gap-4">
      <div className="flex min-w-0 items-center gap-2">
        {navigation}
        <h2 className="truncate text-lg font-semibold tracking-tight">
          {title}
        </h2>
      </div>
      {action ? (
        <div className="flex shrink-0 items-center gap-2">{action}</div>
      ) : null}
    </div>
  )
}

/// Operation failures are reported where failures belong — over the content,
/// briefly — not as a banner that pushes the page down and stays there.
export function useSettingsErrorToast(title: string, message?: string) {
  const reportedRef = React.useRef<string | undefined>(undefined)
  React.useEffect(() => {
    if (!message) {
      reportedRef.current = undefined
      return
    }
    if (message === reportedRef.current) return
    reportedRef.current = message
    toast.add({ title, description: message, type: "error" })
  }, [title, message])
}

export function SettingsSection({
  title,
  description,
  action,
  className,
  children,
}: {
  title?: string
  description?: string
  action?: React.ReactNode
  className?: string
  children: React.ReactNode
}) {
  return (
    <section className={cn("flex flex-col gap-2", className)}>
      {title || description || action ? (
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 flex-col gap-1">
            {title ? <p className="text-sm font-medium">{title}</p> : null}
            {description ? (
              <p className="text-xs text-muted-foreground">{description}</p>
            ) : null}
          </div>
          {action ? (
            <div className="flex shrink-0 items-center gap-1">{action}</div>
          ) : null}
        </div>
      ) : null}
      {children}
    </section>
  )
}

/// Switching between peer views uses one control everywhere: the current peer
/// is a filled pill, the rest are plain text. Tabs rather than buttons, so the
/// group keeps its roving focus and arrow-key navigation.
export function SettingsTabs({
  value,
  onValueChange,
  tabs,
  action,
  className,
  children,
}: {
  value?: string
  onValueChange?: (value: string) => void
  /** The `SettingsTab` list. */
  tabs: React.ReactNode
  /** Search, filters, or other controls that share the tab row. */
  action?: React.ReactNode
  className?: string
  /** `SettingsTabPanel`s, when the tabs switch content in place. */
  children?: React.ReactNode
}) {
  return (
    <Tabs
      value={value}
      onValueChange={(next) => onValueChange?.(String(next))}
      className={className}
    >
      <div
        data-slot="settings-tabs-header"
        className="flex min-w-0 items-center justify-between gap-3"
      >
        <TabsList className="h-auto w-auto shrink-0 justify-start gap-1 bg-transparent p-0">
          {tabs}
        </TabsList>
        {action}
      </div>
      {children}
    </Tabs>
  )
}

export function SettingsTabPanel({
  value,
  className,
  children,
}: {
  value: string
  className?: string
  children: React.ReactNode
}) {
  return (
    <TabsContent value={value} className={className}>
      {children}
    </TabsContent>
  )
}

export function SettingsTab({
  value,
  ariaLabel,
  children,
}: {
  value: string
  ariaLabel?: string
  children: React.ReactNode
}) {
  return (
    <TabsTrigger
      value={value}
      aria-label={ariaLabel}
      className="h-7 flex-none rounded-full border-transparent px-3 text-muted-foreground hover:text-foreground data-active:bg-accent data-active:font-medium data-active:text-foreground dark:data-active:border-transparent dark:data-active:bg-accent"
    >
      {children}
    </TabsTrigger>
  )
}

/// A card of rows. Row separation is the card's job, so rows never draw their
/// own borders.
export function SettingsList({
  className,
  children,
}: {
  className?: string
  children: React.ReactNode
}) {
  return (
    <Card>
      <CardContent className={cn("flex flex-col gap-0.5 py-2", className)}>
        {children}
      </CardContent>
    </Card>
  )
}

/// The icon tile that anchors a row representing a thing — a plugin, a registry,
/// a secret. Rows that configure behaviour rather than name a thing have none.
export function SettingsRowIcon({ children }: { children: React.ReactNode }) {
  return (
    <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground transition-colors group-hover/row:text-foreground group-data-[state=open]/collapsible:text-foreground [&_svg:not([class*='size-'])]:size-4">
      {children}
    </span>
  )
}

/// A row only reacts to hover when hovering it leads somewhere: `onOpen` makes
/// the whole row the way into its detail, and `hoverable` covers rows that
/// reveal their actions instead. A row that is neither stays flat, so nothing
/// implies an interaction the row does not have.
export function SettingsRow({
  icon,
  className,
  onOpen,
  hoverable,
  children,
}: {
  icon?: React.ReactNode
  className?: string
  /** Opens this row's detail. Pair it with `openLabel` on `SettingsRowMain`. */
  onOpen?: () => void
  /** Hover feedback without navigation, for rows with `SettingsHoverAction`. */
  hoverable?: boolean
  children: React.ReactNode
}) {
  return (
    <div
      // Pointer clicks land here; keyboard activation of the row's own button
      // arrives as a click that bubbles to the same handler.
      onClick={onOpen}
      className={cn(
        "group/row -mx-2 flex items-center gap-3 rounded-md px-2 py-2 transition-colors",
        (onOpen || hoverable) && "hover:bg-accent/30",
        onOpen && "cursor-pointer",
        className,
      )}
    >
      {icon ? <SettingsRowIcon>{icon}</SettingsRowIcon> : null}
      {children}
    </div>
  )
}

/// Secondary row actions stay out of the way until the row is hovered, or the
/// action itself is focused by keyboard. Deliberately not `focus-within`: a
/// click on anything else in the row would leave them stuck on screen.
export function SettingsHoverAction({ children }: { children: React.ReactNode }) {
  return (
    <span className="opacity-0 transition-opacity group-hover/row:opacity-100 has-[:focus-visible]:opacity-100">
      {children}
    </span>
  )
}

/// The identity of a row: what it is, then one line about it. Given an
/// `openLabel` it becomes the row's keyboard-reachable control — the click it
/// raises is handled by `SettingsRow`, so there is one navigation path, not two.
export function SettingsRowMain({
  className,
  openLabel,
  children,
}: {
  className?: string
  /** Accessible name for opening this row's detail, e.g. `Open Team LiteLLM`. */
  openLabel?: string
  children: React.ReactNode
}) {
  if (openLabel) {
    return (
      <button
        type="button"
        aria-label={openLabel}
        className={cn(
          "flex min-w-0 flex-1 cursor-pointer flex-col items-start gap-0.5 rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring",
          className,
        )}
      >
        {children}
      </button>
    )
  }
  return (
    <div className={cn("flex min-w-0 flex-1 flex-col gap-0.5", className)}>
      {children}
    </div>
  )
}

/// Spans, not paragraphs: a row that opens its detail wraps these in a button,
/// which may only contain phrasing content.
export function SettingsRowTitle({
  className,
  children,
}: {
  className?: string
  children: React.ReactNode
}) {
  return (
    <span className={cn("block truncate text-sm font-medium", className)}>
      {children}
    </span>
  )
}

export function SettingsRowMeta({
  className,
  children,
}: {
  className?: string
  children: React.ReactNode
}) {
  return (
    <span className={cn("block truncate text-xs text-muted-foreground", className)}>
      {children}
    </span>
  )
}

export function SettingsRowActions({
  className,
  children,
}: {
  className?: string
  children: React.ReactNode
}) {
  return (
    <div
      // An action is about this row, not a way into it, so it never triggers
      // the row's own navigation.
      onClick={(event) => event.stopPropagation()}
      className={cn("flex shrink-0 items-center gap-1", className)}
    >
      {children}
    </div>
  )
}

/// A labelled cluster inside a card — a row's expanded detail, or a group of
/// related controls.
export function SettingsGroup({
  label,
  className,
  children,
}: {
  label: string
  className?: string
  children: React.ReactNode
}) {
  return (
    <fieldset className={cn("flex flex-col gap-2", className)}>
      <legend className="text-xs font-medium text-muted-foreground">
        {label}
      </legend>
      {children}
    </fieldset>
  )
}

/// Every primary creation or import action in Settings has the same shape.
export function SettingsPrimaryAction({
  label,
  disabled,
  onClick,
}: {
  /** Use Create for local objects and Add for objects from an external source. */
  label: string
  disabled?: boolean
  onClick: () => void
}) {
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      disabled={disabled}
      onClick={onClick}
    >
      <PlusIcon className="size-3.5" />
      {label}
    </Button>
  )
}

/// Every Settings detail page is left the same way: a Back button on its own
/// muted surface, then the breadcrumb whose parent goes to the same list. Two
/// controls for one destination because Back is the one users look for, and the
/// breadcrumb is what tells them where they are.
export function SettingsDetailsNavigation({
  parent,
  current,
  onBack,
}: {
  parent: string
  current: string
  onBack: () => void
}) {
  return (
    <div className="flex min-w-0 items-center gap-2">
      {/* Settings already has a Back in its section rail, so this one says
          where it goes for anyone reading the label rather than the page. */}
      <Button
        type="button"
        size="sm"
        variant="secondary"
        aria-label={`Back to ${parent}`}
        onClick={onBack}
      >
        <ArrowLeftIcon data-icon="inline-start" />
        Back
      </Button>
      <nav aria-label="Breadcrumb">
        <ol className="flex min-w-0 items-center gap-1 text-xs">
          <li>
            <Button type="button" size="sm" variant="ghost" onClick={onBack}>
              {parent}
            </Button>
          </li>
          <li aria-hidden="true" className="text-muted-foreground">
            /
          </li>
          <li
            aria-current="page"
            className="truncate px-2 font-medium text-foreground"
          >
            {current}
          </li>
        </ol>
      </nav>
    </div>
  )
}

/// The title of a detail page, renamed in place. Renaming is an edit of the
/// name alone, so it commits on blur rather than waiting for the form's Save.
export function SettingsDetailsTitle({
  name,
  placeholder,
  editing,
  disabled,
  onEdit,
  onChange,
  onCommit,
  onCancel,
}: {
  name: string
  /** Shown while a new object has no name yet. */
  placeholder: string
  editing: boolean
  disabled?: boolean
  onEdit: () => void
  onChange: (name: string) => void
  /** Blur or Enter: keep the edit. */
  onCommit: () => void
  /** Escape: put the previous name back. */
  onCancel: () => void
}) {
  // The field exists only while editing, so mounting it is exactly when the
  // name should be selected and ready to be typed over.
  const focusInput = React.useCallback((input: HTMLInputElement | null) => {
    if (!input) return
    input.focus()
    input.select()
  }, [])

  if (editing) {
    return (
      <Input
        ref={focusInput}
        value={name}
        disabled={disabled}
        className="h-8 w-full text-base font-semibold"
        aria-label="Name"
        placeholder={placeholder}
        onChange={(event) => onChange(event.currentTarget.value)}
        onBlur={onCommit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault()
            onCommit()
          } else if (event.key === "Escape") {
            onCancel()
          }
        }}
      />
    )
  }

  return (
    <div className="group/title flex min-w-0 items-center gap-1">
      <h2 className="truncate text-lg font-semibold tracking-tight">
        {name || placeholder}
      </h2>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        disabled={disabled}
        aria-label={`Rename ${name || placeholder}`}
        onClick={onEdit}
        className="opacity-0 transition-opacity group-hover/title:opacity-100 focus-visible:opacity-100"
      >
        <PencilIcon />
      </Button>
    </div>
  )
}

export function SettingsDetailsActionMenu({
  name,
  disabled,
  onDelete,
}: {
  name: string
  disabled?: boolean
  onDelete: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={(
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            disabled={disabled}
            aria-label={`Actions for ${name}`}
          />
        )}
      >
        <MoreHorizontalIcon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem variant="destructive" onClick={onDelete}>
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

/// Unsaved edits pin their actions to the bottom of the viewport, where the
/// separator earns its place by dividing the bar from the content scrolling
/// under it. Actions that simply end a form — Create, Cancel — sit in the
/// normal flow with nothing drawn above them.
export function SettingsDetailsActionBar({
  sticky,
  children,
}: {
  sticky: boolean
  children: React.ReactNode
}) {
  return (
    <div
      className={cn(
        "flex items-center justify-end gap-2",
        sticky
          ? "sticky bottom-0 z-10 -mx-5 border-t bg-background/95 px-5 py-3 backdrop-blur"
          : "py-3",
      )}
    >
      {children}
    </div>
  )
}

/// The one empty state: dashed, inside the section's own card, with the action
/// that fills it.
export function SettingsEmpty({
  icon,
  title,
  description,
  action,
}: {
  icon: React.ReactNode
  title: string
  description: string
  action?: React.ReactNode
}) {
  return (
    <Empty className="border border-dashed">
      <EmptyHeader>
        <EmptyMedia variant="icon">{icon}</EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{description}</EmptyDescription>
      </EmptyHeader>
      {action ? <EmptyContent>{action}</EmptyContent> : null}
    </Empty>
  )
}

/// A row that expands onto its own detail. The row's own identity is the
/// control: there is no separate disclosure affordance to hunt for. Only the
/// label area is the button, so the row's switches, buttons, and menus stay
/// operable — and reachable — in their own right.
export function SettingsDisclosureRow({
  icon,
  title,
  meta,
  actions,
  open,
  onOpenChange,
  defaultOpen,
  children,
}: {
  icon?: React.ReactNode
  title: React.ReactNode
  meta?: React.ReactNode
  actions?: React.ReactNode
  open?: boolean
  onOpenChange?: (open: boolean) => void
  defaultOpen?: boolean
  children: React.ReactNode
}) {
  return (
    <Collapsible
      open={open}
      onOpenChange={onOpenChange}
      defaultOpen={defaultOpen}
      className="group/row group/collapsible -mx-2 flex flex-col rounded-md px-2 py-2 transition-colors hover:bg-accent/30"
    >
      <div className="flex items-center gap-3">
        {icon ? <SettingsRowIcon>{icon}</SettingsRowIcon> : null}
        <CollapsibleTrigger
          render={(
            <button
              type="button"
              className="flex min-w-0 flex-1 cursor-pointer flex-col items-start gap-0.5 rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
          )}
        >
          <span className="w-full truncate text-sm font-medium">{title}</span>
          {meta ? (
            <>
              {/* Keeps the row's accessible name from running the two lines
                  together into one word. */}
              <span className="sr-only">, </span>
              <span className="w-full truncate text-xs text-muted-foreground">
                {meta}
              </span>
            </>
          ) : null}
        </CollapsibleTrigger>
        {actions ? (
          <div className="flex shrink-0 items-center gap-1">{actions}</div>
        ) : null}
      </div>
      {/* Detail lines up with the row's title, past the icon tile. */}
      <CollapsibleContent
        className={cn("mt-2 flex flex-col gap-3", icon && "pl-11")}
      >
        {children}
      </CollapsibleContent>
    </Collapsible>
  )
}
