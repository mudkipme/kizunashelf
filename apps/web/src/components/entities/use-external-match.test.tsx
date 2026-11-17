import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { reviewMatch, searchSources } from "@/api/entities";
import { externalProvidersQuery } from "@/api/queries";
import { useExternalMatch } from "@/components/entities/use-external-match";
import { useLanguageStore } from "@/lib/language";
import { render } from "@/test/render";
import type {
  ExternalMatch,
  ExternalProviderCatalog,
  ExternalSearchResponse,
  TypeConfig,
} from "@/types/api";

vi.mock("@/api/entities", { spy: true });

const providerCatalog: ExternalProviderCatalog = {
  providers: [
    {
      id: "bangumi",
      label: "Bangumi",
      fields: [],
      types: [],
      defaultExternalTypes: [],
      credentials: [],
      searchSupported: true,
    },
  ],
};
const typeConfig: TypeConfig = {
  id: "stories",
  label: "Stories",
  path: "Stories",
  fields: [{ field: "source", fieldType: "externalRef", externalRef: "bangumi" }],
};
const providers = [{ id: "bangumi", label: "Bangumi", enabled: true, searchSupported: true }];
function result(title: string): ExternalSearchResponse {
  return {
    providers,
    items: [
      {
        entityType: "stories",
        candidate: {
          provider: "bangumi",
          sourceId: title,
          title,
          url: `https://bgm.tv/subject/${title}`,
          titles: {},
          metadata: {},
        },
      },
    ],
  };
}
const pending = new Map<
  string,
  { resolve: (value: ExternalSearchResponse) => void; signal?: AbortSignal | null }
>();

beforeEach(() => {
  pending.clear();
  useLanguageStore.setState({ language: "en" });
  vi.mocked(searchSources).mockImplementation((params, init) => {
    if (!params.q) return Promise.resolve({ providers, items: [] });
    // Deliberately ignore abort: even an uncooperative transport must not
    // overwrite a newer search when its old response eventually arrives.
    return new Promise((resolve) => pending.set(params.q!, { resolve, signal: init?.signal }));
  });
});
afterEach(() => {
  vi.resetAllMocks();
  useLanguageStore.setState({ language: "en" });
});

function Harness() {
  const [entityId, setEntityId] = useState("first");
  const match = useExternalMatch({
    typeConfig,
    providerCatalog,
    entityId,
    entityType: typeConfig.id,
  });
  // Simulate capture's availability query sharing the same cache as matching.
  useQuery(externalProvidersQuery(typeConfig.id));
  return (
    <>
      <button onClick={() => match.setOpen(true)}>Open</button>
      <button onClick={() => match.setOpen(false)}>Close</button>
      <button onClick={() => match.search("bangumi", "old")}>Old search</button>
      <button onClick={() => match.search("bangumi", "new")}>New search</button>
      <button onClick={() => setEntityId("second")}>Switch entity</button>
      <button onClick={() => useLanguageStore.setState({ language: "zh-Hant" })}>
        Switch language
      </button>
      <button onClick={() => match.chooseCandidate(result("chosen").items[0] as ExternalMatch)}>
        Choose
      </button>
      <output data-testid="results">
        {match.candidates.map((item) => item.candidate.title).join(",")}
      </output>
      <output data-testid="loading">{String(match.searching)}</output>
      <button onClick={() => match.setSelectedFields(new Set(["title"]))}>Toggle title</button>
      <button onClick={match.retryReview}>Retry review</button>
      <output data-testid="selected">{[...match.selectedFields].join(",")}</output>
      <output data-testid="review">{match.reviewStatus}</output>
      <output data-testid="applicable">{match.candidateForApply?.title ?? ""}</output>
    </>
  );
}

describe("external matching", () => {
  it.each(["zh-Hans", "zh-Hant"])(
    "preserves %s when reviewing provider details",
    async (language) => {
      vi.mocked(reviewMatch).mockResolvedValue({
        entityType: "stories",
        candidate: result("chosen").items[0].candidate,
        fields: [],
        sections: [],
      });
      useLanguageStore.setState({ language });
      const screen = await render(<Harness />);
      await screen.getByRole("button", { name: "Open", exact: true }).click();
      await screen.getByRole("button", { name: "Choose" }).click();
      expect(reviewMatch).toHaveBeenLastCalledWith("first", {
        candidate: result("chosen").items[0].candidate,
        language,
      });
    },
  );

  it("shares provider availability and ignores a superseded response", async () => {
    const screen = await render(<Harness />);
    await screen.getByRole("button", { name: "Open", exact: true }).click();
    await screen.getByRole("button", { name: "Old search" }).click();
    await expect.poll(() => pending.has("old")).toBe(true);
    const old = pending.get("old")!;
    await screen.getByRole("button", { name: "New search" }).click();
    await expect.poll(() => pending.has("new")).toBe(true);
    expect(old.signal?.aborted).toBe(true);
    pending.get("new")!.resolve(result("new"));
    await expect.element(screen.getByTestId("results")).toHaveTextContent("new");
    old.resolve(result("old"));
    await expect.element(screen.getByTestId("loading")).toHaveTextContent("false");
    await expect.element(screen.getByTestId("results")).toHaveTextContent("new");
    expect(vi.mocked(searchSources).mock.calls.filter(([params]) => !params.q)).toHaveLength(1);
  });

  it.each(["Close", "Switch entity", "Switch language"])(
    "cancels a search on %s",
    async (action) => {
      const screen = await render(<Harness />);
      await screen.getByRole("button", { name: "Open", exact: true }).click();
      await screen.getByRole("button", { name: "Old search" }).click();
      await expect.poll(() => pending.has("old")).toBe(true);
      const old = pending.get("old")!;
      await screen.getByRole("button", { name: action, exact: true }).click();
      await expect.poll(() => old.signal?.aborted).toBe(true);
      old.resolve(result("old"));
      await expect.element(screen.getByTestId("results")).toHaveTextContent("");
      await expect.element(screen.getByTestId("loading")).toHaveTextContent("false");
    },
  );

  it("drops a late candidate review after closing", async () => {
    let resolveReview!: (value: Awaited<ReturnType<typeof reviewMatch>>) => void;
    vi.mocked(reviewMatch).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveReview = resolve;
        }),
    );
    const screen = await render(<Harness />);
    await screen.getByRole("button", { name: "Open", exact: true }).click();
    await screen.getByRole("button", { name: "Choose" }).click();
    await screen.getByRole("button", { name: "Close", exact: true }).click();
    resolveReview({
      entityType: "stories",
      candidate: result("chosen").items[0].candidate,
      sections: [],
      fields: [
        {
          field: "source",
          value: "new",
          source: "bangumi",
          hasValue: true,
          selected: true,
          locked: false,
          current: null,
        },
      ],
    });
    await screen.getByRole("button", { name: "Open", exact: true }).click();
    await expect.element(screen.getByTestId("selected")).toHaveTextContent("");
  });

  it("only accepts choices once the candidate's review has landed", async () => {
    let resolveReview!: (value: Awaited<ReturnType<typeof reviewMatch>>) => void;
    vi.mocked(reviewMatch).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveReview = resolve;
        }),
    );
    const screen = await render(<Harness />);
    await screen.getByRole("button", { name: "Open", exact: true }).click();
    await screen.getByRole("button", { name: "Choose" }).click();
    await expect.element(screen.getByTestId("review")).toHaveTextContent("reviewing");
    // A toggle before the review lands has nothing to apply to, and nothing is
    // applicable yet, so the core's locks can't be bypassed.
    await screen.getByRole("button", { name: "Toggle title" }).click();
    await expect.element(screen.getByTestId("selected")).toHaveTextContent("");
    await expect.element(screen.getByTestId("applicable")).toHaveTextContent("");

    resolveReview({
      entityType: "stories",
      candidate: { ...result("chosen").items[0].candidate, title: "enriched" },
      sections: [],
      fields: [
        {
          field: "source",
          value: "new",
          source: "bangumi",
          hasValue: true,
          selected: true,
          locked: true,
          current: null,
        },
      ],
    });
    await expect.element(screen.getByTestId("review")).toHaveTextContent("ready");
    await expect.element(screen.getByTestId("selected")).toHaveTextContent("source");
    await expect.element(screen.getByTestId("applicable")).toHaveTextContent("enriched");
    // Once ready, the user's choices are kept.
    await screen.getByRole("button", { name: "Toggle title" }).click();
    await expect.element(screen.getByTestId("selected")).toHaveTextContent("title");
  });

  it("reports a failed review and retries it", async () => {
    vi.mocked(reviewMatch).mockRejectedValueOnce(new Error("provider down"));
    const screen = await render(<Harness />);
    await screen.getByRole("button", { name: "Open", exact: true }).click();
    await screen.getByRole("button", { name: "Choose" }).click();
    await expect.element(screen.getByTestId("review")).toHaveTextContent("failed");
    await expect.element(screen.getByTestId("applicable")).toHaveTextContent("");

    vi.mocked(reviewMatch).mockResolvedValueOnce({
      entityType: "stories",
      candidate: result("chosen").items[0].candidate,
      fields: [],
      sections: [],
    });
    await screen.getByRole("button", { name: "Retry review" }).click();
    await expect.element(screen.getByTestId("review")).toHaveTextContent("ready");
    await expect.element(screen.getByTestId("applicable")).toHaveTextContent("chosen");
    expect(reviewMatch).toHaveBeenCalledTimes(2);
  });
});
