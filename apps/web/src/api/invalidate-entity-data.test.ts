import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";

import { invalidateEntityData } from "./invalidate-entity-data";
import { queryKeys } from "./queries";

describe("entity cache invalidation", () => {
  it.each([
    ["episode sources", queryKeys.episodeSources("item", "", "en")],
    ["activity preview", queryKeys.logPreview("item", "2026-10-03", "started", "")],
  ])(
    "reloads %s after an entity edit even when its query parameters stay the same",
    async (_, key) => {
      const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
      try {
        const queryFn = vi
          .fn()
          .mockResolvedValueOnce("before edit")
          .mockResolvedValueOnce("after edit");
        const query = { queryKey: key, queryFn };
        await client.fetchQuery(query);

        await invalidateEntityData(client);

        expect(await client.fetchQuery(query)).toBe("after edit");
        expect(queryFn).toHaveBeenCalledTimes(2);
      } finally {
        client.clear();
      }
    },
  );

  it("keeps configuration and provider availability cached when entity content changes", async () => {
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
    try {
      const keys = [queryKeys.config, queryKeys.capabilities, queryKeys.externalProviders("books")];
      const queryFn = vi.fn().mockResolvedValue("unchanged");
      for (const queryKey of keys) await client.fetchQuery({ queryKey, queryFn });

      await invalidateEntityData(client);

      for (const queryKey of keys) {
        expect(await client.fetchQuery({ queryKey, queryFn })).toBe("unchanged");
      }
      expect(queryFn).toHaveBeenCalledTimes(keys.length);
    } finally {
      client.clear();
    }
  });
});
