import { beforeEach, expect, it, vi } from "vitest";

import { EntityRatings } from "@/components/entities/entity-ratings";
import { render } from "@/test/render";
import type { EntitySummary } from "@/types/api";

const state = vi.hoisted(() => ({
  writable: true,
  save: vi.fn(),
  success: vi.fn(),
  revision: "r1",
}));
const entity: EntitySummary = {
  id: "books:Example",
  type: "books",
  typeLabel: "Books",
  title: "Example",
  basename: "Example",
  path: "Books/Example.md",
  titles: {},
  dates: [],
  externalRefs: {},
  relationCount: 0,
  ratings: [{ field: "感想", label: "My score", value: 8.25, max: 10 }],
};
vi.mock("@/api/entities", () => ({ saveRating: state.save }));
vi.mock("sonner", () => ({ toast: { success: state.success, error: vi.fn() } }));
vi.mock("@/api/client", () => ({
  errorMessage: (error: Error) => error.message,
  isConflictError: (error?: { status: number }) => error?.status === 409,
}));
vi.mock("@/api/invalidate-entity-data", () => ({ useInvalidateEntityData: () => vi.fn() }));
vi.mock("@/lib/capabilities", () => ({
  useCapabilities: () => ({ contentWritable: state.writable }),
}));
vi.mock("@/api/queries", () => ({
  entityQuery: (id: string) => ({
    queryKey: ["entity", id],
    queryFn: async () => ({
      entity: {
        ...entity,
        revision: state.revision,
        frontmatter: { 感想: "8.25", unknown: { nested: true } },
        body: "Notes",
      },
    }),
  }),
}));
beforeEach(() => {
  state.writable = true;
  state.revision = "r1";
  state.save.mockReset();
  state.success.mockReset();
  state.save.mockResolvedValue({
    entity: { ...entity, revision: "r2" },
    previous: { present: true, value: "8.25" },
  });
});

it("saves zero directly and Undo uses the save revision and exact previous value", async () => {
  const screen = await render(<EntityRatings entity={entity} />);
  await screen.getByRole("button", { name: "Rate My score" }).click();
  await screen.getByRole("button", { name: "0", exact: true }).click();
  expect(state.save).toHaveBeenCalledWith(entity.id, {
    revision: "r1",
    field: "感想",
    max: 10,
    value: 0,
  });
  await expect.poll(() => state.success.mock.calls.length).toBe(1);
  state.revision = "r3";
  state.success.mock.calls[0][1].action.onClick();
  expect(state.save).toHaveBeenLastCalledWith(entity.id, {
    revision: "r2",
    field: "感想",
    max: 10,
    restore: { present: true, value: "8.25" },
  });
});

it("preserves a decimal draft on conflict and requires explicit reload before retry", async () => {
  state.save.mockRejectedValueOnce(Object.assign(new Error("Changed elsewhere"), { status: 409 }));
  const screen = await render(<EntityRatings entity={entity} />);
  await screen.getByRole("button", { name: "Rate My score" }).click();
  await screen.getByText("Exact score", { exact: true }).click();
  await screen.getByRole("spinbutton", { name: "Exact score" }).fill("8.75");
  await screen.getByRole("button", { name: "Save", exact: true }).click();
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Changed elsewhere");
  await expect.element(screen.getByRole("spinbutton", { name: "Exact score" })).toHaveValue(8.75);
  await expect.element(screen.getByRole("button", { name: "Save", exact: true })).toBeDisabled();
  state.revision = "r3";
  await screen.getByRole("button", { name: "Reload" }).click();
  await screen.getByRole("button", { name: "Save", exact: true }).click();
  expect(state.save).toHaveBeenLastCalledWith(entity.id, {
    revision: "r3",
    field: "感想",
    max: 10,
    value: 8.75,
  });
});

it("clears only the selected field and keeps scores visible in read-only mode", async () => {
  const screen = await render(<EntityRatings entity={entity} />);
  await screen.getByRole("button", { name: "Rate My score" }).click();
  await screen.getByRole("button", { name: "Clear rating" }).click();
  expect(state.save).toHaveBeenCalledWith(entity.id, {
    revision: "r1",
    field: "感想",
    max: 10,
    value: null,
  });
  state.writable = false;
  await screen.rerender(
    <EntityRatings
      entity={{ ...entity, ratings: [{ field: "感想", label: "My score", value: 87.25 }] }}
    />,
  );
  await expect.element(screen.getByText("87.25", { exact: true })).toBeVisible();
  await expect
    .element(screen.getByRole("button", { name: "Rate My score" }))
    .not.toBeInTheDocument();
});
