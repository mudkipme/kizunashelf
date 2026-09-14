import { Route, Routes, useParams, useSearchParams } from "react-router-dom";
import { expect, it } from "vitest";

import { HomeSmartList } from "@/components/home/home-smart-list";
import { render } from "@/test/render";

function Destination() {
  const { id } = useParams();
  const [params] = useSearchParams();
  return (
    <output>
      {id} — {params.get("view")}
    </output>
  );
}

it("opens the saved list and its Home view through the smart-list route", async () => {
  const screen = await render(
    <Routes>
      <Route
        path="/"
        element={
          <HomeSmartList
            section={{
              id: "読む & Watch",
              name: "My list",
              view: "Favorites & recent",
              total: 0,
              items: [],
            }}
          />
        }
      />
      <Route path="/lists/smart/:id" element={<Destination />} />
    </Routes>,
  );
  await screen.getByRole("link", { name: "See all" }).click();
  await expect
    .element(screen.getByRole("status"))
    .toHaveTextContent("読む & Watch — Favorites & recent");
});
