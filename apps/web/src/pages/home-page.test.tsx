import type { ReactNode } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { HomePage } from "@/pages/home-page";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({ total: 0, contentWritable: true }));

vi.mock("@/api/queries", () => ({
  homeQuery: () => ({
    queryKey: ["home"],
    queryFn: async () => ({
      lists: [{ id: "watching", name: "Watching", total: 0, items: [] }],
    }),
  }),
  configQuery: () => ({ queryKey: ["config"], queryFn: async () => ({ types: [] }) }),
  statsQuery: () => ({ queryKey: ["stats"], queryFn: async () => ({ total: state.total }) }),
}));
vi.mock("@/lib/capabilities", () => ({
  contentWritesDisabled: () => "Read only",
  useCapabilities: () => ({ contentWritable: state.contentWritable, isPending: false }),
}));
vi.mock("@/components/layout/app-frame", () => ({
  AppFrame: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/components/home/coming-up-section", () => ({ ComingUpSection: () => null }));
vi.mock("@/components/smart-lists/suggested-lists-dialog", () => ({
  SuggestedListsButton: () => null,
}));

beforeEach(() => {
  state.total = 0;
  state.contentWritable = true;
});

it("welcomes an empty library with the ways to add a first entry", async () => {
  const screen = await render(<HomePage />);
  await expect.element(screen.getByRole("heading", { name: "Start your library" })).toBeVisible();
  await expect
    .element(screen.getByRole("link", { name: "Search and add" }))
    .toHaveAttribute("href", "/entities/new");
  await expect
    .element(screen.getByRole("link", { name: "Import from another service" }))
    .toHaveAttribute("href", "/entities/import");
  await expect
    .element(screen.getByRole("link", { name: "Add manually" }))
    .toHaveAttribute("href", "/entities/new/manual");
  // The empty shelves it replaces stay hidden.
  expect(screen.getByText("Nothing here yet").elements()).toHaveLength(0);
});

it("shows the shelves once the library has entries", async () => {
  state.total = 3;
  const screen = await render(<HomePage />);
  await expect.element(screen.getByText("Nothing here yet")).toBeVisible();
  expect(screen.getByRole("heading", { name: "Start your library" }).elements()).toHaveLength(0);
});

it("offers no add actions when content writes are disabled", async () => {
  state.contentWritable = false;
  const screen = await render(<HomePage />);
  await expect.element(screen.getByText("Read only")).toBeVisible();
  expect(screen.getByRole("link", { name: "Search and add" }).elements()).toHaveLength(0);
});
