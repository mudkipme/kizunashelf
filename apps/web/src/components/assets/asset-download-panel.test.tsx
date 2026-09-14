import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";

import { fetchAssetJob, startAssetJob } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { queryKeys } from "@/api/queries";
import { AssetDownloadPanel } from "@/components/assets/asset-download-panel";
import { render } from "@/test/render";
import type { AssetDownloadJob, Capabilities, ConfigResponse } from "@/types/api";

vi.mock("@/api/entities", { spy: true });
afterEach(() => vi.resetAllMocks());

const capabilities: Capabilities = {
  settingsWritable: true,
  contentWritable: true,
  vaultWatchEnabled: false,
  externalSearchEnabled: true,
  externalApplyEnabled: true,
  assetDownloadEnabled: true,
};
const config: ConfigResponse = {
  types: [],
  taxonomyRoot: "Taxonomy",
  vaultRoot: "/vault",
  assetRoot: "Assets",
};
const finished: AssetDownloadJob = {
  id: "download",
  status: "completed",
  scope: "all",
  total: 1,
  processed: 1,
  downloaded: 1,
  failed: 0,
  skipped: 0,
  startedAt: "2026-09-14T00:00:00Z",
};

function MutationButton() {
  const invalidate = useInvalidateEntityData();
  return <button onClick={() => void invalidate()}>Entity changed</button>;
}

function cachedClient() {
  const client = new QueryClient({
    defaultOptions: { queries: { staleTime: Infinity, retry: false } },
  });
  client.setQueryData(queryKeys.capabilities, capabilities);
  client.setQueryData(queryKeys.config, config);
  client.setQueryData(queryKeys.providerCatalog, { providers: [] });
  return client;
}

describe("entity-derived cache refresh", () => {
  it("refreshes saved and browsing results after batch covers finish without a vault watcher", async () => {
    vi.mocked(startAssetJob).mockResolvedValue(finished);
    vi.mocked(fetchAssetJob).mockResolvedValue(finished);
    const client = cachedClient();
    const keys = [
      queryKeys.home,
      queryKeys.entity("story"),
      queryKeys.smartListResults("favorites", {}),
      queryKeys.smartListPreview({ filters: { conjunction: "all", rules: [] } }),
    ];
    for (const key of keys) client.setQueryData(key, { items: [] });
    const screen = await render(
      <QueryClientProvider client={client}>
        <AssetDownloadPanel />
      </QueryClientProvider>,
    );
    await screen.getByRole("button", { name: "Start", exact: true }).click();
    await expect
      .poll(() => keys.every((key) => client.getQueryState(key)?.isInvalidated))
      .toBe(true);
    expect(client.getQueryState(queryKeys.config)?.isInvalidated).toBe(false);
    expect(client.getQueryState(queryKeys.providerCatalog)?.isInvalidated).toBe(false);
    client.clear();
  });

  it("refreshes tag suggestions and search membership after an entity changes", async () => {
    const client = cachedClient();
    const searchKey = queryKeys.externalSearch({ type: "stories", q: "new book" });
    client.setQueryData(queryKeys.tags, { tags: ["old"] });
    client.setQueryData(searchKey, { items: [] });
    const screen = await render(
      <QueryClientProvider client={client}>
        <MutationButton />
      </QueryClientProvider>,
    );
    await screen.getByRole("button", { name: "Entity changed" }).click();
    expect(client.getQueryState(queryKeys.tags)?.isInvalidated).toBe(true);
    expect(client.getQueryState(searchKey)?.isInvalidated).toBe(true);
    expect(client.getQueryState(queryKeys.capabilities)?.isInvalidated).toBe(false);
    client.clear();
  });
});
