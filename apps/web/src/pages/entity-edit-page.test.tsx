import { useQueryClient } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { Link, Route, Routes, useLocation, useNavigate } from "react-router-dom";
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
function Navigation() {
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <>
      <Link to="/library?type=books">Library</Link>
      <Link to="/entities/other/edit">Edit another entry</Link>
      <button onClick={() => navigate(-1)}>Back</button>
      <button onClick={() => navigate(1)}>Forward</button>
      <p>Location: {location.pathname + location.search}</p>
    </>
  );
}
async function editor(history?: { initialEntries: string[]; initialIndex: number }) {
  return render(
    <>
      <Refresh />
      <Navigation />
      <Routes>
        <Route path="/entities/:id/edit" element={<EntityEditPage />} />
        <Route path="/entities/:id" element={<p>Saved</p>} />
        <Route path="/library" element={<p>Library destination</p>} />
        <Route path="/activity" element={<p>Activity destination</p>} />
      </Routes>
    </>,
    { route: "/entities/book/edit", ...history },
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

it("allows a clean editor, including reverted edits, to navigate without confirmation", async () => {
  const screen = await editor();
  const notes = screen.getByRole("textbox", { name: "Notes" });
  await expect.element(notes).toHaveValue("Original");
  await notes.fill("Draft");
  await notes.fill("Original");
  await screen.getByRole("link", { name: "Library", exact: true }).click();
  await expect.element(screen.getByText("Library destination", { exact: true })).toBeVisible();
  await expect.element(screen.getByRole("alertdialog")).not.toBeInTheDocument();
  expect(state.save).not.toHaveBeenCalled();
});

it("keeps unsaved edits when navigation is cancelled and discards only to the requested destination", async () => {
  const screen = await editor();
  const notes = screen.getByRole("textbox", { name: "Notes" });
  await expect.element(notes).toHaveValue("Original");
  await notes.fill("My draft");
  await screen.getByRole("link", { name: "Library", exact: true }).click();
  await expect.element(screen.getByRole("alertdialog", { name: "Unsaved changes" })).toBeVisible();
  await screen.getByRole("button", { name: "Keep editing", exact: true }).click();
  await expect.element(notes).toHaveValue("My draft");
  await expect
    .element(screen.getByText("Location: /entities/book/edit", { exact: true }))
    .toBeVisible();
  await screen.getByRole("link", { name: "Library", exact: true }).click();
  await screen.getByRole("button", { name: "Discard", exact: true }).click();
  await expect
    .element(screen.getByText("Location: /library?type=books", { exact: true }))
    .toBeVisible();
  expect(state.save).not.toHaveBeenCalled();
});

it("protects navigation directly to another entry's editor", async () => {
  const screen = await editor();
  const notes = screen.getByRole("textbox", { name: "Notes" });
  await expect.element(notes).toHaveValue("Original");
  await notes.fill("My draft");
  await screen.getByRole("link", { name: "Edit another entry" }).click();
  await expect.element(screen.getByRole("alertdialog", { name: "Unsaved changes" })).toBeVisible();
  await screen.getByRole("button", { name: "Keep editing", exact: true }).click();
  await expect.element(notes).toHaveValue("My draft");
  await expect
    .element(screen.getByText("Location: /entities/book/edit", { exact: true }))
    .toBeVisible();
});

it.each([
  { action: "Back", destination: "/library", label: "Library destination" },
  { action: "Forward", destination: "/activity", label: "Activity destination" },
])(
  "protects history $action and preserves the pending history destination",
  async ({ action, destination, label }) => {
    const screen = await editor({
      initialEntries: ["/library", "/entities/book/edit", "/activity"],
      initialIndex: 1,
    });
    const notes = screen.getByRole("textbox", { name: "Notes" });
    await expect.element(notes).toHaveValue("Original");
    await notes.fill("History draft");
    await screen.getByRole("button", { name: action, exact: true }).click();
    await screen.getByRole("button", { name: "Keep editing", exact: true }).click();
    await expect.element(notes).toHaveValue("History draft");
    await screen.getByRole("button", { name: action, exact: true }).click();
    await screen.getByRole("button", { name: "Discard", exact: true }).click();
    await expect.element(screen.getByText(label, { exact: true })).toBeVisible();
    await expect
      .element(screen.getByText(`Location: ${destination}`, { exact: true }))
      .toBeVisible();
  },
);

it("saves before completing the requested navigation and prevents leaving during the write", async () => {
  const screen = await editor();
  const notes = screen.getByRole("textbox", { name: "Notes" });
  await expect.element(notes).toHaveValue("Original");
  await notes.fill("Save this draft");
  let completeSave!: (result: { entity: ReturnType<typeof entity> }) => void;
  state.save.mockImplementation(
    () =>
      new Promise((resolve) => {
        completeSave = resolve;
      }),
  );
  await screen.getByRole("link", { name: "Library", exact: true }).click();
  await screen.getByRole("button", { name: "Save and leave", exact: true }).click();
  await expect.element(screen.getByRole("button", { name: "Discard", exact: true })).toBeDisabled();
  await expect
    .element(screen.getByRole("button", { name: "Keep editing", exact: true }))
    .toBeDisabled();
  await expect
    .element(screen.getByText("Location: /entities/book/edit", { exact: true }))
    .toBeVisible();
  expect(state.save).toHaveBeenCalledWith("book", {
    revision: "r1",
    schemaRevision: undefined,
    frontmatterDraft: { title: "Original" },
    body: "Save this draft",
  });
  completeSave({ entity: { ...entity(), revision: "r2", body: "Save this draft" } });
  await expect
    .element(screen.getByText("Location: /library?type=books", { exact: true }))
    .toBeVisible();
  await expect.element(screen.getByRole("alertdialog")).not.toBeInTheDocument();
});

it("keeps the pending navigation and draft on save failure, then leaves after a successful retry", async () => {
  const screen = await editor();
  const notes = screen.getByRole("textbox", { name: "Notes" });
  await expect.element(notes).toHaveValue("Original");
  await notes.fill("Do not lose this");
  state.save.mockRejectedValueOnce(new Error("Disk full"));
  await screen.getByRole("link", { name: "Library", exact: true }).click();
  await screen.getByRole("button", { name: "Save and leave", exact: true }).click();
  await expect
    .element(screen.getByRole("alertdialog").getByText("Disk full", { exact: true }))
    .toBeVisible();
  await expect
    .element(screen.getByText("Location: /entities/book/edit", { exact: true }))
    .toBeVisible();
  await screen.getByRole("button", { name: "Keep editing", exact: true }).click();
  await expect.element(notes).toHaveValue("Do not lose this");
  state.save.mockResolvedValue({ entity: entity() });
  await screen.getByRole("link", { name: "Library", exact: true }).click();
  await screen.getByRole("button", { name: "Save and leave", exact: true }).click();
  await expect.element(screen.getByText("Library destination", { exact: true })).toBeVisible();
});

it("keeps a conflicting draft and requires review instead of leaving", async () => {
  const screen = await editor();
  const notes = screen.getByRole("textbox", { name: "Notes" });
  await expect.element(notes).toHaveValue("Original");
  await notes.fill("Conflict draft");
  state.save.mockRejectedValue({ status: 409 });
  await screen.getByRole("link", { name: "Library", exact: true }).click();
  await screen.getByRole("button", { name: "Save and leave", exact: true }).click();
  await expect
    .element(screen.getByRole("button", { name: "Save and leave", exact: true }))
    .toBeDisabled();
  await screen.getByRole("button", { name: "Keep editing", exact: true }).click();
  await expect.element(notes).toHaveValue("Conflict draft");
  await expect
    .element(screen.getByRole("button", { name: "Review changes", exact: true }))
    .toBeVisible();
});

it("protects Cancel and saves to the entity detail without a second confirmation", async () => {
  const screen = await editor();
  const notes = screen.getByRole("textbox", { name: "Notes" });
  await expect.element(notes).toHaveValue("Original");
  await notes.fill("Cancel draft");
  await screen.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect.element(screen.getByRole("alertdialog", { name: "Unsaved changes" })).toBeVisible();
  state.save.mockResolvedValue({ entity: entity() });
  await screen.getByRole("button", { name: "Save and leave", exact: true }).click();
  await expect.element(screen.getByText("Saved", { exact: true })).toBeVisible();
  await expect.element(screen.getByRole("alertdialog")).not.toBeInTheDocument();
});

it("lets a normal Save leave a dirty editor without prompting", async () => {
  const screen = await editor();
  const notes = screen.getByRole("textbox", { name: "Notes" });
  await expect.element(notes).toHaveValue("Original");
  await notes.fill("Normal save");
  state.save.mockResolvedValue({ entity: entity() });
  await screen.getByRole("button", { name: "Save", exact: true }).click();
  await expect.element(screen.getByText("Saved", { exact: true })).toBeVisible();
  await expect.element(screen.getByRole("alertdialog")).not.toBeInTheDocument();
});
