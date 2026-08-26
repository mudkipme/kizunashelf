import { vi } from "vitest";

import type { EntitySummary, StatsResponse } from "@/types/api";

/**
 * Serves the API calls the shell makes on mount.
 *
 * Without this the runner's dev server answers `/api/*` with its own HTML, and
 * because the generated client only validates a JSON content type, that HTML
 * arrives as a plain string where a response object was expected — a failure
 * that surfaces far from its cause. Answering from the contract keeps a shell
 * test about the shell.
 */
export function stubApi({
  stats = testStats,
  entities = [],
}: { stats?: StatsResponse; entities?: EntitySummary[] } = {}) {
  vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
    const url =
      typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url;
    if (url.includes("/api/stats")) return json(stats);
    if (url.includes("/api/entities")) {
      return json({
        items: entities,
        total: entities.length,
        page: 1,
        pageSize: 20,
        totalPages: 1,
      });
    }
    return new Response("not found", { status: 404 });
  });
}

function json(body: unknown) {
  return new Response(JSON.stringify(body), {
    headers: { "content-type": "application/json" },
  });
}

export const testStats: StatsResponse = {
  generatedAt: "2026-01-01T00:00:00Z",
  total: 2,
  relations: 0,
  byType: [{ id: "anime", label: "Anime", count: 2 }],
  byCanonicalStatus: { planning: 0, ongoing: 1, paused: 0, completed: 1, dropped: 0 },
  dateFields: [],
};

export function testEntity(
  overrides: Partial<EntitySummary> & Pick<EntitySummary, "id">,
): EntitySummary {
  return {
    type: "anime",
    typeLabel: "Anime",
    title: overrides.id,
    titles: {},
    dates: [],
    image: null,
    path: `${overrides.id}.md`,
    basename: overrides.id,
    externalRefs: {},
    relationCount: 0,
    ...overrides,
  };
}
