import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { addSuggestedSmartLists, fetchSmartListSuggestions } from "@/api/smart-lists";
import { SuggestedListsButton } from "@/components/smart-lists/suggested-lists-dialog";
import { useLanguageStore } from "@/lib/language";
import { render } from "@/test/render";

vi.mock("@/api/smart-lists", { spy: true });

beforeEach(() => {
  useLanguageStore.setState({ language: "en" });
  vi.mocked(fetchSmartListSuggestions).mockResolvedValue({
    suggestions: [
      {
        id: "ongoing-anime",
        name: "My renamed anime",
        type: "anime",
        existingListId: "My renamed anime",
        showOnHome: true,
      },
      {
        id: "ongoing-book",
        name: "My edited reading list",
        type: "book",
        existingListId: "My edited reading list",
        showOnHome: false,
      },
      { id: "upcoming-event", name: "Upcoming events", type: "event", showOnHome: false },
    ],
  });
  vi.mocked(addSuggestedSmartLists).mockResolvedValue({ lists: [] });
});
afterEach(() => vi.resetAllMocks());

describe("suggested lists picker", () => {
  it("keeps pinned suggestions disabled and submits only the user's selection", async () => {
    const screen = await render(<SuggestedListsButton />);
    await screen.getByRole("button", { name: "Add suggested lists" }).click();
    await expect
      .element(screen.getByRole("button", { name: "My renamed anime — Already on Home" }))
      .toBeDisabled();
    await screen.getByRole("button", { name: "Upcoming events" }).click();
    await screen.getByRole("button", { name: "Add to Home", exact: true }).click();
    await expect
      .poll(() => vi.mocked(addSuggestedSmartLists).mock.calls)
      .toEqual([["en", ["ongoing-book"]]]);
    await expect.element(screen.getByRole("dialog")).not.toBeInTheDocument();
  });

  it("does not submit an empty selection", async () => {
    const screen = await render(<SuggestedListsButton />);
    await screen.getByRole("button", { name: "Add suggested lists" }).click();
    await screen.getByRole("button", { name: "My edited reading list" }).click();
    await screen.getByRole("button", { name: "Upcoming events" }).click();
    await expect
      .element(screen.getByRole("button", { name: "Add to Home", exact: true }))
      .toBeDisabled();
    expect(addSuggestedSmartLists).not.toHaveBeenCalled();
  });
});
