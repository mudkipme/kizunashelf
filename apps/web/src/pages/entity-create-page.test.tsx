import type { ReactNode } from "react";
import { Link, Route, Routes } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";

import { EntityCreatePage } from "@/pages/entity-create-page";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({ writable: true, add: vi.fn(), upload: vi.fn(), save: vi.fn() }));
vi.mock("@/api/entities", () => ({
  addEntity: state.add,
  uploadAsset: state.upload,
  saveEntity: state.save,
}));
vi.mock("@/api/invalidate-entity-data", () => ({ useInvalidateEntityData: () => vi.fn() }));
vi.mock("@/api/use-relation-search", () => ({ useRelationSearch: () => vi.fn() }));
vi.mock("@/lib/capabilities", () => ({
  CONTENT_WRITES_DISABLED: "Content writes are disabled.",
  useCapabilities: () => ({ contentWritable: state.writable }),
}));
vi.mock("@/components/layout/app-frame", () => ({
  AppFrame: ({ children }: { children: ReactNode }) => (
    <>
      <Link to="/library">Library</Link>
      {children}
    </>
  ),
}));
vi.mock("@/api/queries", () => ({
  allTagsQuery: () => ({ queryKey: ["tags"], queryFn: async () => ({ tags: [] }) }),
  configQuery: () => ({
    queryKey: ["config"],
    queryFn: async () => ({
      types: [
        {
          id: "books",
          label: "Books",
          path: "Books",
          fields: [
            { field: "reference", fieldType: "title", displayName: "Title" },
            { field: "id", fieldType: "text", displayName: "Personal note" },
            { field: "internal", fieldType: "id", displayName: "ID" },
            { field: "name", fieldType: "externalRef", displayName: "Source" },
            { field: "art", fieldType: "image", displayName: "Cover" },
            { field: "gallery", fieldType: "imageList", displayName: "Gallery" },
          ],
        },
        { id: "notes", label: "Notes", path: "Notes", fields: [] },
      ],
    }),
  }),
}));

function entity() {
  return { id: "books/New", revision: "r1", frontmatter: { internal: "generated-id" } };
}
beforeEach(() => {
  state.writable = true;
  state.add.mockReset().mockResolvedValue({ entity: entity() });
  state.save.mockReset().mockResolvedValue({ entity: { ...entity(), revision: "r2" } });
  state.upload
    .mockReset()
    .mockImplementation(async (_id, request) => ({ path: `Assets/${request.filename}` }));
});
function createPage(type = "books") {
  return render(
    <Routes>
      <Route path="/new" element={<EntityCreatePage />} />
      <Route path="/entities/:id" element={<p>Created entry</p>} />
      <Route path="/library" element={<p>Library destination</p>} />
    </Routes>,
    { route: `/new?type=${type}` },
  );
}
function imageFile(name: string) {
  return new File(
    [
      Uint8Array.from(
        atob(
          "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aE1cAAAAASUVORK5CYII=",
        ),
        (c) => c.charCodeAt(0),
      ),
    ],
    name,
    { type: "image/png" },
  );
}

it("discloses technical fields by schema and keeps their edits when collapsed", async () => {
  const screen = await createPage();
  await expect
    .element(screen.getByRole("textbox", { name: "Personal note", exact: true }))
    .toBeVisible();
  await expect.element(screen.getByLabelText("ID", { exact: true })).not.toBeVisible();
  await expect.element(screen.getByLabelText("File name", { exact: true })).not.toBeVisible();
  await screen.getByRole("textbox", { name: "Title", exact: true }).fill("New");
  const disclosure = screen.getByRole("button", {
    name: "IDs, sources & custom fields",
    exact: true,
  });
  await disclosure.click();
  await screen.getByRole("textbox", { name: "Source", exact: true }).fill("external-42");
  await screen.getByRole("textbox", { name: "Custom field", exact: true }).fill("legacy");
  await screen.getByRole("button", { name: "Add Field", exact: true }).click();
  await screen.getByRole("textbox", { name: "legacy", exact: true }).fill("keep me");
  await disclosure.click();
  await screen.getByRole("button", { name: "Create", exact: true }).click();
  await expect.element(screen.getByText("Created entry", { exact: true })).toBeVisible();
  expect(state.add).toHaveBeenCalledWith(
    expect.objectContaining({
      basename: "New",
      frontmatterDraft: { reference: "New", name: "external-42", legacy: "keep me" },
    }),
  );
  expect(state.save).not.toHaveBeenCalled();
});

it.each(["upload", "save"])(
  "keeps selected images after %s failure and retries without creating another entry",
  async (failure) => {
    const screen = await createPage();
    await screen.getByRole("textbox", { name: "Title", exact: true }).fill("New");
    await screen.getByLabelText("Upload Cover", { exact: true }).upload(imageFile("cover.png"));
    await screen
      .getByLabelText("Upload Gallery", { exact: true })
      .upload([imageFile("first.png"), imageFile("second.png")]);
    expect(state.add).not.toHaveBeenCalled();
    expect(state.upload).not.toHaveBeenCalled();
    await expect.poll(() => document.querySelectorAll('img[src^="blob:"]').length).toBe(3);

    if (failure === "upload") {
      state.upload
        .mockResolvedValueOnce({ path: "Assets/cover.png" })
        .mockRejectedValueOnce(new Error("Upload interrupted"));
    } else {
      state.save.mockRejectedValueOnce(new Error("Save interrupted"));
    }
    await screen.getByRole("button", { name: "Create", exact: true }).click();
    await expect.element(screen.getByRole("alert")).toHaveTextContent("interrupted");
    await expect.poll(() => document.querySelectorAll('img[src^="blob:"]').length).toBe(3);
    expect(JSON.stringify(state.add.mock.calls)).not.toContain("blob:");
    await screen.getByRole("link", { name: "Library", exact: true }).click();
    await screen.getByRole("button", { name: "Keep editing", exact: true }).click();
    // Once created, clearing the title must not prevent finishing the image save.
    await screen.getByRole("textbox", { name: "Title", exact: true }).fill("");
    await screen.getByRole("button", { name: "Finish saving", exact: true }).click();
    await expect.element(screen.getByText("Created entry", { exact: true })).toBeVisible();
    expect(state.add).toHaveBeenCalledOnce();
    expect(state.upload).toHaveBeenCalledTimes(failure === "upload" ? 4 : 3);
    expect(state.save).toHaveBeenLastCalledWith("books/New", {
      revision: "r1",
      body: "",
      frontmatterDraft: {
        internal: "generated-id",
        reference: null,
        art: "Assets/cover.png",
        gallery: ["Assets/first.png", "Assets/second.png"],
      },
    });
  },
);

it("allows image removal before creating and reveals the required filename for types without a title", async () => {
  const screen = await createPage();
  await screen.getByLabelText("Upload Cover", { exact: true }).upload(imageFile("cover.png"));
  await screen.getByRole("button", { name: "Remove image", exact: true }).click();
  await screen.getByRole("combobox", { name: "Type", exact: true }).selectOptions("notes");
  await screen.getByRole("textbox", { name: "File name", exact: true }).fill("Memo");
  await screen.getByRole("button", { name: "Create", exact: true }).click();
  await expect.element(screen.getByText("Created entry", { exact: true })).toBeVisible();
  expect(state.upload).not.toHaveBeenCalled();
  expect(state.add).toHaveBeenCalledWith(
    expect.objectContaining({ type: "notes", basename: "Memo" }),
  );
});

it("keeps creation and image selection unavailable in read-only mode", async () => {
  state.writable = false;
  const screen = await createPage();
  await expect.element(screen.getByRole("textbox", { name: "Title", exact: true })).toBeDisabled();
  await expect.element(screen.getByRole("button", { name: "Create", exact: true })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Upload", exact: true }).elements()).toHaveLength(0);
  expect(state.add).not.toHaveBeenCalled();
});
