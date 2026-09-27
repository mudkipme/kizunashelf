import { expect, it, vi } from "vitest";

import { ActivityActions } from "@/components/assets/activity-actions";
import { render } from "@/test/render";

it("opens the sheet with the selected Start or Finish action", async () => {
  const select = vi.fn();
  const screen = await render(
    <ActivityActions kinds={["started", "completed"]} onSelect={select} />,
  );
  await screen.getByRole("button", { name: "Start", exact: true }).click();
  expect(select).toHaveBeenLastCalledWith("started");
  await screen.getByRole("button", { name: "Activity actions" }).click();
  await screen.getByRole("menuitem", { name: "Finish" }).click();
  expect(select).toHaveBeenLastCalledWith("completed");
});

it("hides actions the core did not offer", async () => {
  const screen = await render(<ActivityActions kinds={[]} onSelect={vi.fn()} />);
  await expect.element(screen.getByRole("button")).not.toBeInTheDocument();
});
