import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { RuleBuilder } from "@/components/smart-lists/rule-builder";
import { ruleFieldMetas } from "@/components/smart-lists/rule-field-meta";
import { seasonsOfYear } from "@/components/smart-lists/season-values";
import { i18n } from "@/lib/i18n";
import { render } from "@/test/render";
import type { SmartFilterGroup } from "@/types/api";

// The builder is schema-driven end to end: nothing here means anything because
// of what it is *called*. `status` is an enum only because the schema says
// `fieldType: "enum"`, and the assertions below check that the operator menu and
// value editor follow the declared type rather than the field name.
const typeConfig = {
  fields: [
    {
      field: "status",
      fieldType: "enum",
      displayName: "Status",
      enumOptions: ["Watching", "Completed"],
    },
    { field: "score", fieldType: "rating", displayName: "Score" },
    { field: "note", fieldType: "text", displayName: "Note" },
    { field: "aired", fieldType: "date", displayName: "Aired" },
  ],
};

function fieldMetas() {
  return ruleFieldMetas(typeConfig, "tags", ["anime", "manga"], (descriptor) => i18n._(descriptor));
}

/**
 * The builder is a controlled component, so a test that only passes a static
 * `value` can never observe a second edit. This mirrors what every real caller
 * does — hold the group in state and feed it back — which is what makes
 * multi-step assertions (add a rule, then change its operator) possible.
 */
function ControlledRuleBuilder({
  initial,
  onChange,
}: {
  initial: SmartFilterGroup;
  onChange?: (next: SmartFilterGroup) => void;
}) {
  const [value, setValue] = useState(initial);
  return (
    <RuleBuilder
      fieldMetas={fieldMetas()}
      value={value}
      onChange={(next) => {
        setValue(next);
        onChange?.(next);
      }}
    />
  );
}

const emptyGroup: SmartFilterGroup = { conjunction: "all", rules: [], groups: [] };

describe("RuleBuilder", () => {
  it("adds a rule seeded from the first schema field", async () => {
    const onChange = vi.fn();
    const screen = await render(<ControlledRuleBuilder initial={emptyGroup} onChange={onChange} />);

    await screen.getByRole("button", { name: "Add rule" }).click();

    // The first schema field seeds the row, and an enum starts complete: its
    // first option is preselected rather than leaving an empty rule behind.
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        rules: [
          {
            kind: "compare",
            field: "status",
            negated: false,
            values: [],
            op: "eq",
            value: "Watching",
          },
        ],
      }),
    );
    await expect.element(screen.getByRole("combobox", { name: "Field" })).toHaveValue("status");
  });

  it("offers only the operators its declared field type supports", async () => {
    const screen = await render(
      <ControlledRuleBuilder
        initial={{
          conjunction: "all",
          rules: [
            {
              kind: "compare",
              field: "status",
              negated: false,
              values: [],
              op: "eq",
              value: "Watching",
            },
          ],
          groups: [],
        }}
      />,
    );

    const operator = screen.getByRole("combobox", { name: "Operator" });
    await expect.element(operator).toBeVisible();

    // An enum gets equality and emptiness and nothing else — in particular
    // none of the substring operators a `text` field would offer.
    expect(
      operator
        .getByRole("option")
        .elements()
        .map((option) => option.textContent),
    ).toEqual(["is", "is not", "is empty", "has a value"]);
  });

  it("rewrites the rule shape when the operator changes", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <ControlledRuleBuilder
        initial={{
          conjunction: "all",
          rules: [
            {
              kind: "compare",
              field: "status",
              negated: false,
              values: [],
              op: "eq",
              value: "Watching",
            },
          ],
          groups: [],
        }}
        onChange={onChange}
      />,
    );

    await screen.getByRole("combobox", { name: "Operator" }).selectOptions("has a value");

    // "has a value" is not a comparison at all — it is a negated emptiness
    // check, so the whole rule kind changes rather than just its operator.
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        rules: [{ kind: "isEmpty", field: "status", negated: true, values: [] }],
      }),
    );
  });

  it("switching field replaces the rule with that field's default", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <ControlledRuleBuilder
        initial={{
          conjunction: "all",
          rules: [
            {
              kind: "compare",
              field: "status",
              negated: false,
              values: [],
              op: "eq",
              value: "Watching",
            },
          ],
          groups: [],
        }}
        onChange={onChange}
      />,
    );

    await screen.getByRole("combobox", { name: "Field" }).selectOptions("Aired");

    // A date field's least-surprising default is a relative window, not the
    // equality the enum row was using.
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        rules: [
          {
            kind: "compare",
            field: "aired",
            negated: false,
            values: [],
            op: "gte",
            relative: { amount: 30, unit: "days", future: false },
          },
        ],
      }),
    );
    // ...and the value editor follows the new type: an amount + unit pair.
    await expect.element(screen.getByRole("spinbutton", { name: "Amount" })).toBeVisible();
    await expect.element(screen.getByRole("combobox", { name: "Unit" })).toHaveValue("days");
  });

  it("shows a rule outside its vocabulary read-only, and lets it be removed", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <ControlledRuleBuilder
        initial={{
          conjunction: "all",
          rules: [{ kind: "unsupported", negated: false, values: [], raw: "handwritten.expr > 3" }],
          groups: [],
        }}
        onChange={onChange}
      />,
    );

    // Hand-written syntax is preserved rather than mangled, so it is shown
    // verbatim with no field/operator editors offered for it.
    await expect.element(screen.getByText("handwritten.expr > 3")).toBeVisible();
    expect(screen.getByRole("combobox", { name: "Field" }).elements()).toHaveLength(0);

    await screen.getByRole("button", { name: "Remove rule" }).click();
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ rules: [] }));
  });

  it("drops a subgroup once its last rule is removed", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <ControlledRuleBuilder
        initial={{
          conjunction: "all",
          rules: [],
          groups: [
            {
              conjunction: "any",
              rules: [
                {
                  kind: "compare",
                  field: "status",
                  negated: false,
                  values: [],
                  op: "eq",
                  value: "Watching",
                },
              ],
            },
          ],
        }}
        onChange={onChange}
      />,
    );

    await screen.getByRole("button", { name: "Remove rule" }).click();

    // An empty group would evaluate to nothing and read as a mistake, so it
    // disappears with its last rule instead of lingering.
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ groups: [] }));
  });

  it("offers the schema's fields plus the tags and file built-ins, in that order", () => {
    // The first meta seeds a freshly added rule, so schema fields must come
    // first: an enum equality starts complete where a tags rule would sit
    // empty until values are picked.
    expect(fieldMetas().map((meta) => meta.key)).toEqual([
      "status",
      "score",
      "note",
      "aired",
      "tags",
      "file.name",
      "file.mtime",
    ]);
  });

  it("edits a season rule spanning whole years as a 'year is' rule", async () => {
    const seasonMetas = ruleFieldMetas(
      {
        fields: [
          { field: "season", fieldType: "season", displayName: "Season", seasonLanguage: "en" },
        ],
      },
      undefined,
      [],
      (descriptor) => i18n._(descriptor),
    );
    const meta = seasonMetas[0];
    const year = meta.yearOptions?.[0] ?? "2026";
    const screen = await render(
      <RuleBuilder
        fieldMetas={seasonMetas}
        value={{
          conjunction: "all",
          rules: [
            {
              kind: "contains",
              field: "season",
              mode: "any",
              negated: false,
              // All four seasons of one year — stored this way because that is
              // what the engine can evaluate, but meant as "the year".
              values: seasonsOfYear(year, "en"),
            },
          ],
          groups: [],
        }}
        onChange={() => {}}
      />,
    );

    // It must come back as the intent, not as the four values it is stored as.
    await expect.element(screen.getByRole("combobox", { name: "Operator" })).toHaveValue("yearIs");
    await expect.element(screen.getByText(year)).toBeVisible();
  });
});
