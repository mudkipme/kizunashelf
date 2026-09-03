import { useState } from "react";
import { describe, expect, it } from "vitest";
import { userEvent } from "vitest/browser";

import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { render } from "@/test/render";

/**
 * The shared multi-value picker behind tag editors, enum lists, relation
 * targets and rule values. Its behavior is popover positioning, focus movement
 * and chip management — the parts a DOM emulator has to approximate — so it is
 * exercised in a real browser here.
 */
function Controlled({
  options = [],
  allowCustomValue = false,
  initial = [],
}: {
  options?: { value: string; label?: string; detail?: string }[];
  allowCustomValue?: boolean;
  initial?: string[];
}) {
  const [values, setValues] = useState(initial);
  return (
    <div>
      <MultiValueCombobox
        values={values}
        options={options}
        ariaLabel="Tags"
        placeholder="Add value"
        allowCustomValue={allowCustomValue}
        onChange={setValues}
      />
      {/* Mirrors committed state so assertions read the value, not the chips. */}
      <output data-testid="values">{values.join("|")}</output>
    </div>
  );
}

describe("MultiValueCombobox", () => {
  it("picks an option from the list and shows it as a chip", async () => {
    const screen = await render(<Controlled options={[{ value: "anime" }, { value: "manga" }]} />);

    await screen.getByRole("combobox", { name: "Tags" }).click();
    await screen.getByRole("option", { name: "manga" }).click();

    await expect.element(screen.getByTestId("values")).toHaveTextContent("manga");
  });

  it("filters the list as you type", async () => {
    const screen = await render(
      <Controlled options={[{ value: "anime" }, { value: "manga" }, { value: "movie" }]} />,
    );

    const input = screen.getByRole("combobox", { name: "Tags" });
    await input.click();
    await userEvent.keyboard("an");

    // Matching is substring, not prefix: "manga" matches on its middle.
    await expect
      .poll(() =>
        screen
          .getByRole("option")
          .elements()
          .map((option) => option.textContent),
      )
      .toEqual(["anime", "manga"]);
  });

  it("refuses a typed value when custom values are not allowed", async () => {
    const screen = await render(<Controlled options={[{ value: "anime" }]} />);

    const input = screen.getByRole("combobox", { name: "Tags" });
    await input.click();
    await userEvent.keyboard("brand-new");

    // A closed vocabulary (schema enum options) must not gain values by typing.
    expect(screen.getByRole("option").elements()).toHaveLength(0);
    await expect.element(screen.getByTestId("values")).toHaveTextContent("");
  });

  it("offers a typed value as a new entry when custom values are allowed", async () => {
    const screen = await render(<Controlled options={[{ value: "anime" }]} allowCustomValue />);

    const input = screen.getByRole("combobox", { name: "Tags" });
    await input.click();
    await userEvent.keyboard("brand-new");
    // The new entry is labelled as an addition so it can't be mistaken for an
    // existing option.
    await screen.getByRole("option", { name: 'Add "brand-new"' }).click();

    await expect.element(screen.getByTestId("values")).toHaveTextContent("brand-new");
  });

  it("commits a typed value on comma, so several can be entered in a row", async () => {
    const screen = await render(<Controlled allowCustomValue />);

    const input = screen.getByRole("combobox", { name: "Tags" });
    await input.click();
    await userEvent.keyboard("first,");
    await userEvent.keyboard("second,");

    await expect.element(screen.getByTestId("values")).toHaveTextContent("first|second");
  });

  it("never stores the same value twice", async () => {
    const screen = await render(<Controlled allowCustomValue initial={["anime"]} />);

    const input = screen.getByRole("combobox", { name: "Tags" });
    await input.click();
    await userEvent.keyboard("anime,");

    await expect.element(screen.getByTestId("values")).toHaveTextContent("anime");
  });

  it("shows an option's label while storing its underlying value", async () => {
    const screen = await render(
      <Controlled options={[{ value: "frieren", label: "Frieren", detail: "Anime" }]} />,
    );

    await screen.getByRole("combobox", { name: "Tags" }).click();
    await screen.getByRole("option", { name: /Frieren/ }).click();

    // A relation picker shows a readable title but must write the basename the
    // wikilink resolves against.
    await expect.element(screen.getByTestId("values")).toHaveTextContent("frieren");
    await expect.element(screen.getByText("Frieren").first()).toBeVisible();
  });
});
