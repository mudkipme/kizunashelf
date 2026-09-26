import type { ReactNode } from "react";
import { useLocation } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";

import { QuickCapturePage } from "@/pages/quick-capture-page";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({ writable: true, add: vi.fn() }));
vi.mock("@/api/entities", () => ({ quickAddEntity: state.add }));
vi.mock("@/api/invalidate-entity-data", () => ({ useInvalidateEntityData: () => vi.fn() }));
vi.mock("@/lib/capabilities", () => ({
  CONTENT_WRITES_DISABLED: "Content writes are disabled.",
  useCapabilities: () => ({ contentWritable: state.writable }),
}));
vi.mock("@/components/layout/app-frame", () => ({
  AppFrame: ({ children }: { children: ReactNode }) => children,
}));
vi.mock("@/api/queries", () => {
  const providers = [{ id: "bangumi", label: "Bangumi", enabled: true }];
  return {
    configQuery: () => ({
      queryKey: ["config"],
      queryFn: async () => ({
        types: [
          {
            id: "anime",
            label: "Anime",
            fields: [{ field: "source", fieldType: "externalRef", externalRef: "bangumi" }],
          },
        ],
      }),
    }),
    providerCatalogQuery: () => ({ queryKey: ["providers"], queryFn: async () => ({ providers }) }),
    externalProvidersQuery: () => ({ queryKey: ["probe"], queryFn: async () => ({ providers }) }),
    externalSearchQuery: ({ q }: { q: string }) => ({
      queryKey: ["search", q],
      queryFn: async () => ({
        providers,
        items: [
          {
            entityType: "anime",
            candidate: { provider: "bangumi", sourceId: "1", title: "New title" },
          },
          {
            entityType: "anime",
            candidate: { provider: "bangumi", sourceId: "2", title: "Saved title" },
            existing: { id: "anime/saved" },
          },
        ],
      }),
    }),
  };
});

function Location() {
  return <output aria-label="Location">{useLocation().pathname}</output>;
}

beforeEach(() => {
  state.writable = true;
  state.add.mockReset();
  state.add.mockRejectedValue(new Error("Test write rejected"));
});

it("requires the explicit Add action before writing a search result", async () => {
  const screen = await render(<QuickCapturePage />, { route: "/entities/new?type=anime" });
  await screen.getByRole("textbox", { name: "Search", exact: true }).fill("title");
  await screen.getByText("New title", { exact: true }).click();
  expect(state.add).not.toHaveBeenCalled();
  await screen.getByRole("button", { name: "Add New title", exact: true }).click();
  expect(state.add).toHaveBeenCalledOnce();
});

it("opens existing items in read-only mode while disabling Add", async () => {
  state.writable = false;
  const screen = await render(
    <>
      <QuickCapturePage />
      <Location />
    </>,
    { route: "/entities/new?type=anime" },
  );
  await screen.getByRole("textbox", { name: "Search", exact: true }).fill("title");
  await expect
    .element(screen.getByRole("button", { name: "Add New title", exact: true }))
    .toBeDisabled();
  await screen.getByRole("button", { name: "Open Saved title", exact: true }).click();
  await expect
    .element(screen.getByRole("status", { name: "Location" }))
    .toHaveTextContent("/entities/anime%2Fsaved");
  expect(state.add).not.toHaveBeenCalled();
});
