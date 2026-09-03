import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { PlanReview } from "@/components/import/plan-review";
import { render } from "@/test/render";
import type { ImportPlan, ImportPlanBucket, ImportPlanItem } from "@/types/api";

const providerLabels = new Map([
  ["anilist", "AniList"],
  ["bangumi", "Bangumi"],
]);
const typeLabels = new Map([
  ["anime", "Anime"],
  ["tv", "TV series"],
  ["book", "Book"],
]);

function item(overrides: Partial<ImportPlanItem> & Pick<ImportPlanItem, "index">): ImportPlanItem {
  return {
    title: `Item ${overrides.index}`,
    provider: "anilist",
    bucket: "anime",
    state: "willCreate",
    userData: { hasNotes: false },
    ...overrides,
  };
}

function bucket(
  overrides: Partial<ImportPlanBucket> & Pick<ImportPlanBucket, "bucket">,
): ImportPlanBucket {
  return { provider: "anilist", candidateTypes: [], ...overrides };
}

/**
 * The wizard owns the skip set (the commit reads it), so the review is a
 * controlled component. Holding the set here lets a test observe the checkbox
 * *after* a toggle rather than only the callback that requested it.
 */
function Controlled({
  plan,
  onToggleSkip,
  onBucketType = () => {},
}: {
  plan: ImportPlan;
  onToggleSkip?: (index: number) => void;
  onBucketType?: (bucket: string, type: string) => void;
}) {
  const [skip, setSkip] = useState<ReadonlySet<number>>(new Set());
  const [types, setTypes] = useState<Record<string, string>>({});
  return (
    <PlanReview
      plan={plan}
      onBucketType={(bucketKey, type) => {
        setTypes((prev) => ({ ...prev, [bucketKey]: type }));
        onBucketType(bucketKey, type);
      }}
      effectiveType={(planBucket) =>
        types[planBucket.bucket] ?? planBucket.selectedType ?? planBucket.candidateTypes[0]
      }
      skip={skip}
      onToggleSkip={(index) => {
        setSkip((prev) => {
          const next = new Set(prev);
          if (next.has(index)) next.delete(index);
          else next.add(index);
          return next;
        });
        onToggleSkip?.(index);
      }}
      typeLabels={typeLabels}
      providerLabels={providerLabels}
    />
  );
}

describe("PlanReview", () => {
  it("summarizes the plan by what each item will do", async () => {
    const screen = await render(
      <Controlled
        plan={{
          buckets: [],
          items: [
            item({ index: 0 }),
            item({ index: 1 }),
            item({
              index: 2,
              state: "exists",
              existing: { id: "anime/Frieren", title: "Frieren" },
            }),
            item({ index: 3, state: "needsReview", reviewReason: "noTypeMatch" }),
          ],
        }}
      />,
    );

    await expect.element(screen.getByText("4 found")).toBeVisible();
    await expect.element(screen.getByText("2 to create")).toBeVisible();
    // The two non-creating states are called out separately: "already there" is
    // reassuring, "needs review" is a thing the user has to act on.
    await expect.element(screen.getByText(/1 already in library/)).toBeVisible();
    await expect.element(screen.getByText(/1 needs review/)).toBeVisible();
  });

  it("asks for a type only where the mapping is genuinely ambiguous", async () => {
    const onBucketType = vi.fn();
    const screen = await render(
      <Controlled
        onBucketType={onBucketType}
        plan={{
          buckets: [
            bucket({ bucket: "anime", candidateTypes: ["anime", "tv"] }),
            // One candidate needs no question asked, and none is a different
            // problem entirely — handled by the warning below, not a picker.
            bucket({ bucket: "manga", candidateTypes: ["book"] }),
            bucket({ provider: "bangumi", bucket: "2", candidateTypes: [] }),
          ],
          items: [],
        }}
      />,
    );

    const picker = screen.getByRole("combobox", { name: "AniList · anime" });
    expect(screen.getByRole("combobox").elements()).toHaveLength(1);
    expect(
      picker
        .getByRole("option")
        .elements()
        .map((option) => option.textContent),
    ).toEqual(["Anime", "TV series"]);

    await picker.selectOptions("TV series");
    // The bucket key travels, not the provider-qualified label the user read.
    expect(onBucketType).toHaveBeenLastCalledWith("anime", "tv");
    await expect.element(picker).toHaveValue("tv");
  });

  it("names an unmappable bucket in the provider's own words", async () => {
    const screen = await render(
      <Controlled
        plan={{
          // Bangumi's buckets are numeric `subject_type` codes; "2" must reach
          // the user as "Anime" or the warning is unactionable.
          buckets: [bucket({ provider: "bangumi", bucket: "2", candidateTypes: [] })],
          items: [],
        }}
      />,
    );

    await expect.element(screen.getByText(/No entity type maps Bangumi Anime/)).toBeVisible();
  });

  it("offers a skip checkbox only for items it would actually create", async () => {
    const onToggleSkip = vi.fn();
    const screen = await render(
      <Controlled
        onToggleSkip={onToggleSkip}
        plan={{
          buckets: [],
          items: [
            item({ index: 7, title: "Frieren" }),
            item({ index: 3, title: "Vinland Saga", state: "exists" }),
            item({
              index: 5,
              title: "Mushishi",
              state: "needsReview",
              reviewReason: "noSupportedId",
            }),
          ],
        }}
      />,
    );

    // Nothing to opt out of on a row that will not be written: an existing
    // entity is already skipped, and one needing review cannot be created.
    expect(screen.getByRole("checkbox").elements()).toHaveLength(1);

    const include = screen.getByRole("checkbox", { name: "Include Frieren" });
    await expect.element(include).toBeChecked();
    await include.click();

    // The plan's own `index` is what commit skips on, not the row's position —
    // this row is first but carries index 7.
    expect(onToggleSkip).toHaveBeenLastCalledWith(7);
    await expect.element(include).not.toBeChecked();
    // Skipped rows stay listed (dimmed), so the decision remains reversible.
    await expect.element(screen.getByText("Frieren")).toBeVisible();
  });

  it("links an already-imported item to the entity it matched", async () => {
    const screen = await render(
      <Controlled
        plan={{
          buckets: [],
          items: [
            item({
              index: 0,
              title: "Frieren",
              state: "exists",
              existing: { id: "anime/Frieren", title: "Frieren" },
            }),
            // Matched by dedup but with no resolved entity to point at: the
            // badge still shows, just without a dead link.
            item({ index: 1, title: "Mushishi", state: "exists" }),
          ],
        }}
      />,
    );

    const link = screen.getByRole("link");
    expect(link.elements()).toHaveLength(1);
    await expect.element(link).toHaveAttribute("href", "/entities/anime%2FFrieren");
    expect(screen.getByText("In library ✓").elements()).toHaveLength(2);
  });

  it("summarizes an item's user data in one line", async () => {
    const screen = await render(
      <Controlled
        plan={{
          buckets: [],
          items: [
            item({
              index: 0,
              userData: {
                status: "completed",
                score10: 9,
                watchedCount: 12,
                completed: "2026-01-02",
                hasNotes: true,
              },
            }),
          ],
        }}
      />,
    );

    await expect
      .element(screen.getByText(/watched/))
      .toHaveTextContent("Completed · ★ 9 · 12 watched · 2026-01-02 · notes");
  });

  it("mounts a large plan in windows, and all of it on request", async () => {
    const screen = await render(<Scrolled count={500} />);
    const rows = () => screen.getByRole("listitem").elements().length;

    // A MAL/Goodreads export routinely runs to thousands of rows; mounting them
    // all at once is what makes the review step feel broken.
    await expect.poll(rows).toBe(200);
    await expect.element(screen.getByText("Showing 200 of 500")).toBeVisible();

    // Clicked directly rather than through the driver: a driver click scrolls
    // the button into view first, which is exactly what brings the sentinel
    // near the viewport — the list would then grow on its own and the assertion
    // below could not tell the button from the scroll.
    (screen.getByRole("button", { name: "Show all" }).element() as HTMLElement).click();

    // "Show all" exists so the whole plan can be searched with Ctrl-F.
    await expect.poll(rows).toBe(500);
    expect(screen.getByText(/Showing/).elements()).toHaveLength(0);
  });

  it("grows the window as the plan is scrolled, without a click", async () => {
    const screen = await render(<Scrolled count={500} />);
    const rows = () => screen.getByRole("listitem").elements().length;
    await expect.poll(rows).toBe(200);

    const viewport = screen.getByTestId("viewport").element();
    viewport.scrollTop = viewport.scrollHeight;

    // The sentinel coming near the viewport is the normal way the list grows;
    // "Show all" is the escape hatch, not the mechanism.
    await expect.poll(rows).toBeGreaterThan(200);
  });
});

/**
 * The window only stays a window while the sentinel is off screen. A test page
 * has no natural viewport bound — the runner's iframe is as tall as its content,
 * so every sentinel is "in view" and the list would grow to full on its own.
 * Clipping to a short scroll container restores the constraint a real page has.
 */
function Scrolled({ count }: { count: number }) {
  const items = Array.from({ length: count }, (_, index) =>
    item({ index, title: `Item ${index}` }),
  );
  return (
    <div data-testid="viewport" style={{ height: "300px", overflowY: "auto" }}>
      <Controlled plan={{ buckets: [], items }} />
    </div>
  );
}
