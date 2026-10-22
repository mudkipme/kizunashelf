import type { ReactNode } from "react";
import { Link, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";

import { SettingsPage } from "@/pages/settings-page";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({ save: vi.fn(), saveRaw: vi.fn() }));
const vault = {
  taxonomyRoot: "Taxonomy",
  assetRoot: "Assets",
  types: [],
  dailyNotes: { paths: [], dateFormat: "YYYY-MM-DD", log: { section: "Log" } },
};
vi.mock("@/api/queries", () => ({
  settingsConfigQuery: () => ({
    queryKey: ["settings"],
    queryFn: async () => ({ vault, revision: "r1" }),
  }),
  rawSettingsConfigQuery: () => ({
    queryKey: ["raw-settings"],
    queryFn: async () => ({ content: "types: []", revision: "r1" }),
  }),
  providerCatalogQuery: () => ({
    queryKey: ["providers"],
    queryFn: async () => ({ providers: [] }),
  }),
  languagesQuery: () => ({
    queryKey: ["languages"],
    queryFn: async () => ({ languages: [], userLanguages: [] }),
  }),
  typePresetsQuery: () => ({
    queryKey: ["presets"],
    queryFn: async () => ({ presets: [], categories: [] }),
  }),
}));
vi.mock(import("@/api/settings"), async (original) => ({
  ...(await original()),
  saveSettingsConfig: state.save,
  saveRawSettingsConfig: state.saveRaw,
  getPathSuggestions: vi.fn(async () => ({ suggestions: [] })),
}));
vi.mock(import("@/api/client"), async (original) => ({
  ...(await original()),
  isConflictError: (error: unknown) =>
    Boolean(error && typeof error === "object" && "status" in error && error.status === 409),
}));
vi.mock("@/lib/capabilities", () => ({
  useCapabilities: () => ({ settingsWritable: true }),
}));
vi.mock("@/components/layout/app-frame", () => ({
  AppFrame: ({ children }: { children: ReactNode }) => children,
}));

beforeEach(() => {
  state.save.mockReset().mockResolvedValue({ vault, revision: "r2" });
  state.saveRaw.mockReset().mockResolvedValue({ content: "types: []\n# draft", revision: "r2" });
});

function Navigation() {
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <>
      <Link to="/library?type=books">Library</Link>
      <button onClick={() => navigate(-1)}>Back</button>
      <button onClick={() => navigate(1)}>Forward</button>
      <p>Location: {location.pathname + location.search}</p>
    </>
  );
}

async function editor(mode: string) {
  const screen = await render(
    <>
      <Navigation />
      <Routes>
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/library" element={<p>Library destination</p>} />
        <Route path="/activity" element={<p>Activity destination</p>} />
      </Routes>
    </>,
    { initialEntries: ["/library", "/settings", "/activity"], initialIndex: 1 },
  );
  if (mode === "YAML") await screen.getByRole("button", { name: "YAML", exact: true }).click();
  const field = screen.getByRole("textbox", {
    name: mode === "YAML" ? "Edit config.yaml" : "Log section",
    exact: true,
  });
  await expect.element(field).toHaveValue(mode === "YAML" ? "types: []" : "Log");
  return {
    screen,
    field,
    save: mode === "YAML" ? state.saveRaw : state.save,
    draft: mode === "YAML" ? "types: []\n# draft" : "My log",
  };
}

it.each(["Form", "YAML"])(
  "protects %s drafts on route changes, keeps editing, and discards only on request",
  async (mode) => {
    const { screen, field, draft, save } = await editor(mode);
    await field.fill(draft);
    await screen.getByRole("link", { name: "Library" }).click();
    await expect.element(screen.getByRole("alertdialog")).toBeVisible();
    await screen.getByRole("button", { name: "Keep editing" }).click();
    await expect.element(field).toHaveValue(draft);
    await screen.getByRole("link", { name: "Library" }).click();
    await screen.getByRole("button", { name: "Discard", exact: true }).click();
    await expect
      .element(screen.getByText("Location: /library?type=books", { exact: true }))
      .toBeVisible();
    expect(save).not.toHaveBeenCalled();
  },
);

it.each(["Form", "YAML"])(
  "preserves a failed %s save and the destination, then waits for a successful retry",
  async (mode) => {
    const { screen, field, draft, save } = await editor(mode);
    save.mockRejectedValueOnce(new Error("Disk full"));
    await field.fill(draft);
    const fieldElement = field.element();
    await screen.getByRole("link", { name: "Library" }).click();
    await screen.getByRole("button", { name: "Save and leave" }).click();
    await expect.element(screen.getByRole("alertdialog").getByText("Disk full")).toBeVisible();
    await expect.element(fieldElement).toHaveValue(draft);
    let finish!: (value: unknown) => void;
    save.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await screen.getByRole("button", { name: "Save and leave" }).click();
    await expect
      .element(screen.getByRole("button", { name: "Discard", exact: true }))
      .toBeDisabled();
    await expect.element(screen.getByRole("button", { name: "Keep editing" })).toBeDisabled();
    await expect.element(fieldElement).toBeDisabled();
    expect(save).toHaveBeenLastCalledWith(
      mode === "YAML"
        ? { content: draft, revision: "r1" }
        : expect.objectContaining({
            revision: "r1",
            vault: expect.objectContaining({
              dailyNotes: expect.objectContaining({ log: { section: draft } }),
            }),
          }),
    );
    finish(mode === "YAML" ? { content: draft, revision: "r2" } : { vault, revision: "r2" });
    await expect
      .element(screen.getByText("Location: /library?type=books", { exact: true }))
      .toBeVisible();
  },
);

it.each(["Form", "YAML"])("protects %s drafts during Back and Forward navigation", async (mode) => {
  const { screen, field, draft } = await editor(mode);
  await field.fill(draft);
  await screen.getByRole("button", { name: "Back", exact: true }).click();
  await screen.getByRole("button", { name: "Keep editing" }).click();
  await expect.element(field).toHaveValue(draft);
  await screen.getByRole("button", { name: "Forward", exact: true }).click();
  await screen.getByRole("button", { name: "Discard", exact: true }).click();
  await expect.element(screen.getByText("Location: /activity", { exact: true })).toBeVisible();
});

it.each(["Form", "YAML"])("keeps the %s draft after a revision conflict", async (mode) => {
  const { screen, field, draft, save } = await editor(mode);
  save.mockRejectedValueOnce(Object.assign(new Error("Changed elsewhere"), { status: 409 }));
  await field.fill(draft);
  await screen.getByRole("link", { name: "Library" }).click();
  await screen.getByRole("button", { name: "Save and leave" }).click();
  await expect.element(screen.getByRole("alertdialog").getByRole("alert")).toBeVisible();
  await expect.element(screen.getByRole("button", { name: "Save and leave" })).toBeDisabled();
  await screen.getByRole("button", { name: "Keep editing" }).click();
  await expect.element(field).toHaveValue(draft);
  await expect.element(screen.getByRole("button", { name: "Reload latest version" })).toBeVisible();
});

it.each(["Form", "YAML"])("allows navigation after a normal %s save", async (mode) => {
  const { screen, field, draft, save } = await editor(mode);
  await field.fill(draft);
  await screen.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => save.mock.calls.length).toBe(1);
  await expect.element(screen.getByRole("button", { name: "Save", exact: true })).toBeEnabled();
  await screen.getByRole("link", { name: "Library" }).click();
  await expect
    .element(screen.getByText("Location: /library?type=books", { exact: true }))
    .toBeVisible();
});
