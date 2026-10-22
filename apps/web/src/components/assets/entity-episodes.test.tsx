import { expect, it, vi } from "vitest";
import { userEvent } from "vitest/browser";

import { EntityEpisodesPanel } from "@/components/assets/entity-episodes";
import { render } from "@/test/render";
import type { EntityEpisodes } from "@/types/api";

const episodes: EntityEpisodes = {
  heading: "Episodes",
  tracking: "checklist",
  total: 1,
  watched: 1,
  description: "",
  trailing: "",
  groups: [
    {
      label: "Season 1",
      items: [
        {
          key: "1",
          title: "Journey’s End [Details](https://example.com)",
          watched: true,
          done: "2026-10-21",
        },
      ],
    },
  ],
};

it("keeps episode toggling, links, and completion dates independently operable", async () => {
  const onToggle = vi.fn();
  const onSetDate = vi.fn();
  const screen = await render(
    <EntityEpisodesPanel
      episodes={episodes}
      disabled={false}
      saving={false}
      onToggle={onToggle}
      onSetDate={onSetDate}
      relations={[]}
    />,
  );
  const checkbox = screen.getByRole("checkbox", { name: "1 Journey’s End Details" });
  const date = screen.getByLabelText("Change completion date");
  await expect.element(checkbox).toBeChecked();
  expect(date.element().closest('[role="checkbox"]')).toBeNull();
  await date.fill("2026-10-22");
  expect(onSetDate).toHaveBeenLastCalledWith("Season 1", "1", 0, "2026-10-22");
  expect(onToggle).not.toHaveBeenCalled();

  const link = screen.getByRole("link", { name: "Details" });
  expect(link.element().closest('[role="checkbox"]')).toBeNull();
  link.element().addEventListener("click", (event) => event.preventDefault());
  await link.click();
  expect(onToggle).not.toHaveBeenCalled();

  (checkbox.element() as HTMLElement).focus();
  await userEvent.keyboard("[Space]");
  expect(onToggle).toHaveBeenCalledExactlyOnceWith("Season 1", "1", 0, false);
  await userEvent.keyboard("[Enter]");
  expect(onToggle).toHaveBeenCalledTimes(2);
  await checkbox.click();
  expect(onToggle).toHaveBeenCalledTimes(3);
});

it.each([
  { disabled: true, saving: false },
  { disabled: false, saving: true },
])("keeps episode controls inert when %j", async (props) => {
  const onToggle = vi.fn();
  const screen = await render(
    <EntityEpisodesPanel
      episodes={episodes}
      {...props}
      onToggle={onToggle}
      onSetDate={vi.fn()}
      relations={[]}
    />,
  );
  await expect.element(screen.getByRole("checkbox")).toBeDisabled();
  await expect.element(screen.getByLabelText("Change completion date")).not.toBeInTheDocument();
  await screen.getByText("Journey’s End", { exact: false }).click();
  expect(onToggle).not.toHaveBeenCalled();
});
