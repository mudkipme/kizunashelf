import { useQueryClient } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { Route, Routes } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";

import { EntityEditPage } from "@/pages/entity-edit-page";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({
  save: vi.fn(),
  review: vi.fn(),
  revision: "r1",
  body: "Original",
  fields: { title: "Original" } as Record<string, unknown>,
}));
function entity() {
  return {
    id: "book",
    type: "books",
    basename: "Book",
    revision: state.revision,
    body: state.body,
    frontmatter: state.fields,
    path: "Books/Book.md",
    title: "Book",
    titles: {},
  };
}
vi.mock("@/api/entities", () => ({ saveEntity: state.save, reviewEntityDraft: state.review }));
vi.mock("@/api/client", () => ({
  errorMessage: (error: Error) => error.message,
  isConflictError: (error?: { status: number }) => error?.status === 409,
}));
vi.mock("@/api/queries", () => ({
  entityQuery: () => ({ queryKey: ["entity"], queryFn: async () => ({ entity: entity() }) }),
  configQuery: () => ({ queryKey: ["config"], queryFn: async () => ({ types: [] }) }),
}));
vi.mock("@/lib/capabilities", () => ({
  CONTENT_WRITES_DISABLED: "Read only",
  useCapabilities: () => ({ contentWritable: true }),
}));
vi.mock("@/api/use-relation-search", () => ({ useRelationSearch: () => vi.fn() }));
vi.mock("@/api/invalidate-entity-data", () => ({ useInvalidateEntityData: () => vi.fn() }));
vi.mock("@/components/layout/app-frame", () => ({
  AppFrame: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/components/entities/metadata-editor", () => ({
  normalizeFrontmatter: (value: unknown) => structuredClone(value),
  MetadataEditor: ({
    bodyText,
    onBodyChange,
    disabled,
  }: {
    bodyText: string;
    onBodyChange: (body: string) => void;
    disabled: boolean;
  }) => (
    <textarea
      aria-label="Notes"
      disabled={disabled}
      value={bodyText}
      onChange={(event) => onBodyChange(event.target.value)}
    />
  ),
}));
beforeEach(() => {
  state.save.mockReset();
  state.review.mockReset();
  state.revision = "r1";
  state.body = "Original";
  state.fields = { title: "Original" };
});
function Refresh() {
  const client = useQueryClient();
  return <button onClick={() => void client.invalidateQueries()}>Refresh fixture</button>;
}
async function editor() {
  return render(
    <>
      <Refresh />
      <Routes>
        <Route path="/entities/:id/edit" element={<EntityEditPage />} />
        <Route path="/entities/:id" element={<p>Saved</p>} />
      </Routes>
    </>,
    { route: "/entities/book/edit" },
  );
}

it("keeps a dirty draft through background refresh, cancels review safely, and saves the reviewed merge only on Save", async () => {
  const screen = await editor();
  await expect.element(screen.getByRole("textbox", { name: "Notes" })).toHaveValue("Original");
  await screen.getByRole("textbox", { name: "Notes" }).fill("My notes");
  state.revision = "r2";
  state.fields = { title: "External", raw: { nested: [null, true] } };
  await screen.getByRole("button", { name: "Refresh fixture" }).click();
  await expect.element(screen.getByRole("button", { name: "Review changes" })).toBeVisible();
  await expect.element(screen.getByRole("textbox", { name: "Notes" })).toHaveValue("My notes");
  state.review.mockResolvedValue({
    entity: entity(),
    local: { basename: "Book", body: "My notes", frontmatter: { title: "Original" } },
    merged: { basename: "Book", body: "My notes", frontmatter: state.fields },
    conflictFields: [],
    bodyConflict: false,
    nameConflict: false,
    schemaRevision: "schema-2",
  });
  await screen.getByRole("button", { name: "Review changes" }).click();
  await screen.getByRole("button", { name: "Keep editing" }).click();
  await expect.element(screen.getByRole("button", { name: "Review changes" })).toBeVisible();
  await screen.getByRole("button", { name: "Review changes" }).click();
  await screen.getByRole("button", { name: "Use reviewed draft" }).click();
  expect(state.save).not.toHaveBeenCalled();
  expect(state.review.mock.calls[0][1].baseline.body).toBe("Original");
  expect(state.review.mock.calls[0][1].draft.body).toBe("My notes");
  state.save.mockResolvedValue({ entity: entity() });
  await screen.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => state.save.mock.calls.length).toBe(1);
  expect(state.save).toHaveBeenCalledWith("book", {
    revision: "r2",
    schemaRevision: "schema-2",
    frontmatterDraft: state.fields,
    body: "My notes",
  });
});
