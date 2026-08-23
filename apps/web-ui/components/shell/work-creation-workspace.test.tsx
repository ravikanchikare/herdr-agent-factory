import { fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import {
  SuccessCriteriaFields,
  withTrailingEmptyCriterion,
  WorkCreationWorkspace,
} from "@/components/shell/work-creation-workspace"

describe("withTrailingEmptyCriterion", () => {
  it("always keeps exactly one trailing empty row", () => {
    expect(withTrailingEmptyCriterion([])).toEqual([""])
    expect(withTrailingEmptyCriterion([""])).toEqual([""])
    expect(withTrailingEmptyCriterion(["a"])).toEqual(["a", ""])
    expect(withTrailingEmptyCriterion(["a", ""])).toEqual(["a", ""])
    expect(withTrailingEmptyCriterion(["a", "", ""])).toEqual(["a", ""])
    expect(withTrailingEmptyCriterion(["a", "b"])).toEqual(["a", "b", ""])
  })
})

describe("SuccessCriteriaFields", () => {
  it("exposes the next empty criterion as the current one is filled", () => {
    const onChange = vi.fn()
    const { rerender } = render(
      <SuccessCriteriaFields criteria={[""]} onChange={onChange} />,
    )

    expect(
      screen.getByRole("textbox", { name: "Success criterion 1" }),
    ).toBeTruthy()
    expect(
      screen.queryByRole("textbox", { name: "Success criterion 2" }),
    ).toBeNull()
    expect(
      screen.queryByRole("button", { name: "Add criterion" }),
    ).toBeNull()

    fireEvent.change(
      screen.getByRole("textbox", { name: "Success criterion 1" }),
      { target: { value: "Refunds are classified" } },
    )
    expect(onChange).toHaveBeenCalledWith(["Refunds are classified", ""])

    rerender(
      <SuccessCriteriaFields
        criteria={["Refunds are classified", ""]}
        onChange={onChange}
      />,
    )
    expect(
      screen.getByRole("textbox", { name: "Success criterion 2" }),
    ).toBeTruthy()
  })

  it("removes a filled criterion and keeps a trailing empty row", () => {
    const onChange = vi.fn()
    render(
      <SuccessCriteriaFields
        criteria={["First", "Second", ""]}
        onChange={onChange}
      />,
    )

    fireEvent.click(
      screen.getByRole("button", { name: "Remove success criterion 1" }),
    )
    expect(onChange).toHaveBeenCalledWith(["Second", ""])
  })
})

describe("WorkCreationWorkspace", () => {
  it("keeps the heading in content, close in the title bar, and 30/70 rows", () => {
    render(
      <WorkCreationWorkspace
        createTargetAgent={async () => true}
        sidebarOpen
        onClose={() => {}}
      />,
    )

    const region = screen.getByRole("region", { name: "Define your agent" })
    const heading = within(region).getByRole("heading", {
      name: "Define your agent",
    })
    expect(heading).toBeTruthy()
    expect(heading.closest("header")).toBeNull()
    expect(
      within(region).getByText(
        "Save the initial draft, then configure an Environment before starting a Run.",
      ),
    ).toBeTruthy()
    const close = within(region).getByRole("button", { name: "Close" })
    expect(close.closest("header")).not.toBeNull()
    expect(
      within(region).queryByRole("button", { name: "Add criterion" }),
    ).toBeNull()
    expect(within(region).getByRole("button", { name: "Cancel" })).toBeTruthy()
    expect(within(region).queryByRole("button", { name: "Discard" })).toBeNull()

    const nameLabel = within(region).getByText("Name", {
      selector: "label",
    })
    const nameRow = nameLabel.closest(".group\\/row")
    expect(nameRow).not.toBeNull()
    const main = nameRow!.querySelector(".w-\\[30\\%\\]")
    const actions = nameRow!.querySelector(".w-\\[70\\%\\]")
    expect(main).not.toBeNull()
    expect(actions).not.toBeNull()
    expect(actions?.className).toContain("pr-4")
  })

  it("closes from the title-bar action", () => {
    const onClose = vi.fn()
    render(
      <WorkCreationWorkspace
        createTargetAgent={async () => true}
        sidebarOpen
        onClose={onClose}
      />,
    )

    fireEvent.click(screen.getByRole("button", { name: "Close" }))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it("checks Trust workspace by default", () => {
    render(
      <WorkCreationWorkspace
        createTargetAgent={async () => true}
        sidebarOpen
        onClose={() => {}}
      />,
    )

    expect(screen.getByRole("checkbox", { name: "Trust workspace" })).toBeChecked()
  })
})
