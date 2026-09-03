import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { FieldValueInput } from "@/components/entities/metadata-inputs";
import type { EditableFieldSpec, FrontmatterValue } from "@/components/entities/metadata-types";
import { render } from "@/test/render";

/**
 * Every spec here is deliberately named to fight the assertion it is used in:
 * a `select` field called `notes`, a `text` field called `status`. If the editor
 * ever started guessing meaning from a field's name instead of its declared
 * `kind`, these are the tests that would catch it.
 */
function spec(
  overrides: Partial<EditableFieldSpec> & Pick<EditableFieldSpec, "kind">,
): EditableFieldSpec {
  return {
    key: "field",
    label: "Field",
    configured: true,
    options: [],
    relationOptions: [],
    seasonLanguage: "en",
    ...overrides,
  };
}

function Controlled({
  field,
  initial = null,
  onChange,
}: {
  field: EditableFieldSpec;
  initial?: FrontmatterValue;
  onChange?: (value: FrontmatterValue) => void;
}) {
  const [value, setValue] = useState<FrontmatterValue>(initial);
  return (
    <FieldValueInput
      field={field}
      value={value}
      disabled={false}
      onChange={(next) => {
        setValue(next);
        onChange?.(next);
      }}
    />
  );
}

describe("FieldValueInput dispatch", () => {
  it("renders a select for a `select` field, whatever the field is called", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled
        field={spec({
          kind: "select",
          key: "notes",
          label: "Notes",
          options: ["Watching", "Done"],
        })}
        onChange={onChange}
      />,
    );

    const select = screen.getByRole("combobox", { name: "Notes" });
    // The empty choice comes first: a configured field may legitimately have no
    // value, and clearing it must not require deleting text.
    expect(
      select
        .getByRole("option")
        .elements()
        .map((option) => option.textContent),
    ).toEqual(["Empty", "Watching", "Done"]);

    await select.selectOptions("Done");
    expect(onChange).toHaveBeenLastCalledWith("Done");
  });

  it("keeps a value the schema no longer offers, rather than silently dropping it", async () => {
    const screen = await render(
      <Controlled
        field={spec({ kind: "select", key: "status", label: "Status", options: ["Watching"] })}
        // Hand-written frontmatter, or a value removed from the schema since.
        initial="Rewatching"
      />,
    );

    const select = screen.getByRole("combobox", { name: "Status" });
    await expect.element(select).toHaveValue("Rewatching");
    expect(
      select
        .getByRole("option")
        .elements()
        .map((option) => option.textContent),
    ).toContain("Rewatching");
  });

  it("renders a text input for a `text` field, even one named like a status", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled
        field={spec({ kind: "text", key: "status", label: "Status" })}
        onChange={onChange}
      />,
    );

    const input = screen.getByRole("textbox", { name: "Status" });
    await expect.element(input).toBeVisible();
    // No option list anywhere: this is free text because the schema says so.
    expect(screen.getByRole("option").elements()).toHaveLength(0);

    await input.fill("Rewatching");
    expect(onChange).toHaveBeenLastCalledWith("Rewatching");
  });

  it("steps a `number` field with its buttons and keeps it non-negative", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled
        field={spec({ kind: "number", key: "episodes", label: "Episodes" })}
        initial={1}
        onChange={onChange}
      />,
    );

    await screen.getByRole("button", { name: "Increase Episodes" }).click();
    expect(onChange).toHaveBeenLastCalledWith(2);

    await screen.getByRole("button", { name: "Decrease Episodes" }).click();
    await screen.getByRole("button", { name: "Decrease Episodes" }).click();
    // A count of watched episodes has no meaningful value below zero, so the
    // stepper floors rather than going negative.
    await screen.getByRole("button", { name: "Decrease Episodes" }).click();
    expect(onChange).toHaveBeenLastCalledWith(0);
  });

  it("renders a date field as a calendar popover, storing the ISO value", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled
        field={spec({ kind: "date", key: "aired", label: "Aired" })}
        onChange={onChange}
      />,
    );

    // Empty until picked — the trigger says so rather than showing a stale date.
    const trigger = screen.getByRole("button", { name: "Aired" });
    await expect.element(trigger).toHaveTextContent("Pick a date");

    await trigger.click();
    await screen.getByRole("gridcell", { name: "15" }).first().click();

    // Whatever the calendar renders locally, the vault stores plain ISO.
    expect(onChange).toHaveBeenLastCalledWith(expect.stringMatching(/^\d{4}-\d{2}-15$/));
  });

  it("renders a boolean field as a three-state select, not a checkbox", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled
        field={spec({ kind: "boolean", key: "favorite", label: "Favorite" })}
        onChange={onChange}
      />,
    );

    // A checkbox could not express "not set", which is distinct from "no" in
    // frontmatter — an absent key versus an explicit `false`.
    const select = screen.getByRole("combobox", { name: "Favorite" });
    expect(
      select
        .getByRole("option")
        .elements()
        .map((option) => option.textContent),
    ).toEqual(["Empty", "Yes", "No"]);

    await select.selectOptions("Yes");
    expect(onChange).toHaveBeenLastCalledWith(true);
    await select.selectOptions("No");
    expect(onChange).toHaveBeenLastCalledWith(false);
    await select.selectOptions("Empty");
    expect(onChange).toHaveBeenLastCalledWith(null);
  });
});
