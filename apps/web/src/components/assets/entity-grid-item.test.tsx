import { expect, it, vi } from "vitest";
import { page } from "vitest/browser";

import { EntityGridItem } from "@/components/assets/entity-grid-item";
import { testEntity } from "@/test/api-stub";
import { render } from "@/test/render";

vi.mock("@/api/queries", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/api/queries")>()),
  configQuery: () => ({
    queryKey: ["config"],
    queryFn: async () => ({ types: [{ id: "anime", icon: "📺" }] }),
  }),
}));

it("aligns title baselines for portrait, missing and broken covers", async () => {
  await page.viewport(1000, 800);
  const portrait = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="200" height="400"><rect width="200" height="400" fill="navy"/></svg>')}`;
  const screen = await render(
    <div className="grid grid-cols-3 gap-3" style={{ width: 720 }}>
      <EntityGridItem
        showType={false}
        entity={testEntity({
          id: "poster",
          title: "Poster",
          image: portrait,
          status: { field: "state", value: "Watching", canonical: "ongoing" },
        })}
      />
      <EntityGridItem showType={false} entity={testEntity({ id: "missing", title: "Missing" })} />
      <EntityGridItem
        entity={testEntity({
          id: "broken",
          title: "Broken",
          image: "data:image/png;base64,invalid",
        })}
      />
    </div>,
  );
  await expect.poll(() => document.querySelectorAll("img").length).toBe(1);
  const cover = screen.getByRole("link", { name: /Poster/ }).element().firstElementChild!;
  const slot = cover.getBoundingClientRect();
  expect(slot.height / slot.width).toBeCloseTo(1.5, 2);
  const tops = ["Poster", "Missing", "Broken"].map(
    (title) => screen.getByText(title, { exact: true }).element().getBoundingClientRect().top,
  );
  expect(tops[1]).toBeCloseTo(tops[0], 1);
  expect(tops[2]).toBeCloseTo(tops[0], 1);
});
