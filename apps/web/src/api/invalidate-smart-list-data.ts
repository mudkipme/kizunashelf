import type { QueryClient } from "@tanstack/react-query";

/** Smart-list files drive the list index, their detail pages, and Home. */
export async function invalidateSmartListData(queryClient: QueryClient) {
  await Promise.all(
    ["lists", "smartList", "smartListResults", "smartListSuggestions", "home"].map((key) =>
      queryClient.invalidateQueries({ queryKey: [key] }),
    ),
  );
}
