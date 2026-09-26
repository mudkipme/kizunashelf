import type { EntityEditReviewResponse } from "@kizunashelf/api-contract";
import { expect, it, vi } from "vitest";

import { acceptEditReview, EditConflictReview } from "@/components/entities/edit-conflict-review";
import { render } from "@/test/render";

const review = {
  entity: {
    id: "book",
    type: "books",
    typeLabel: "Books",
    title: "Book",
    titles: {},
    dates: [],
    path: "Books/Book.md",
    externalRefs: {},
    relationCount: 0,
    revision: "r2",
    raw: "",
    frontmatter: { title: "External", nullable: 1, removed: "external" },
    body: "External notes",
    basename: "Book",
  },
  local: { frontmatter: { title: "Mine", nullable: null }, body: "My notes", basename: "Book" },
  merged: {
    frontmatter: {
      title: "External",
      nullable: 1,
      removed: "external",
      nested: { keep: [1, null, true] },
    },
    body: "External notes",
    basename: "Book",
  },
  conflictFields: ["title", "nullable", "removed"],
  bodyConflict: true,
  nameConflict: false,
  schemaRevision: "schema-2",
} satisfies EntityEditReviewResponse;

it("applies explicit choices without dropping unknown values or confusing null with deletion", () => {
  const draft = acceptEditReview(review, {
    fields: { title: "mine", nullable: "mine", removed: "mine" },
    body: "mine",
  });
  expect(draft.frontmatter).toEqual({
    title: "Mine",
    nullable: null,
    nested: { keep: [1, null, true] },
  });
  expect(draft.body).toBe("My notes");
  expect(review.merged.frontmatter.removed).toBe("external");
});

it("requires every conflict to be chosen before accepting, and cancellation does not accept", async () => {
  const accept = vi.fn();
  const cancel = vi.fn();
  const screen = await render(
    <EditConflictReview
      review={{ ...review, conflictFields: ["title"], bodyConflict: false }}
      label={(field) => field}
      onAccept={accept}
      onCancel={cancel}
    />,
  );
  await expect.element(screen.getByRole("button", { name: "Use reviewed draft" })).toBeDisabled();
  await screen.getByRole("radio", { name: "My edits" }).click();
  await screen.getByRole("button", { name: "Use reviewed draft" }).click();
  expect(accept.mock.calls[0][0].frontmatter.title).toBe("Mine");
  await screen.getByRole("button", { name: "Keep editing" }).click();
  expect(cancel).toHaveBeenCalledOnce();
  expect(accept).toHaveBeenCalledOnce();
});
