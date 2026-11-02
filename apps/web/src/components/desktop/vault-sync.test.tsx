import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useBlocker, useLocation } from "react-router-dom";
import { beforeEach, expect, it, vi } from "vitest";

import { DesktopVaultSync } from "@/components/desktop/vault-sync";
import { useUnsavedChangesWarning } from "@/hooks/use-unsaved-changes-warning";
import type { VaultInfo } from "@/lib/desktop";
import { desktopVaultsKey } from "@/lib/desktop-vaults";
import { useVaultSession } from "@/lib/vault-session";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({
  listener: undefined as ((vaults: VaultInfo[]) => void) | undefined,
  dirty: vi.fn(),
}));
const vaults = [{ name: "Personal", path: "/vaults/personal", active: true }];
const next = [{ name: "Work", path: "/vaults/work", active: true }];

vi.mock(import("@/lib/desktop"), async (original) => ({
  ...(await original()),
  isDesktopRuntime: () => true,
  listVaults: async () => vaults,
  setUnsavedChanges: state.dirty,
  onVaultsChanged: async (callback: (vaults: VaultInfo[]) => void) => {
    state.listener = callback;
    return () => {};
  },
}));

beforeEach(() => {
  state.listener = undefined;
  state.dirty.mockReset().mockResolvedValue(undefined);
});

function Editor() {
  const [draft, setDraft] = useState("");
  const location = useLocation();
  useUnsavedChangesWarning(Boolean(draft));
  const blocker = useBlocker(Boolean(draft));
  const client = useQueryClient();
  return (
    <>
      <input aria-label="Draft" value={draft} onChange={(event) => setDraft(event.target.value)} />
      <p>{location.pathname}</p>
      <p>{blocker.state}</p>
      <button onClick={() => client.setQueryData(["entity", "same-id"], "old vault")}>
        Seed cache
      </button>
      <button
        onClick={() => {
          client.setQueryData(desktopVaultsKey, next);
          state.listener?.(next);
        }}
      >
        Switch from manager
      </button>
      <button
        onClick={() =>
          state.listener?.([...vaults, { name: "Other", path: "/other", active: false }])
        }
      >
        Forget inactive vault
      </button>
      <p>Cached entity: {String(client.getQueryData(["entity", "same-id"]))}</p>
    </>
  );
}

function Shell() {
  const session = useVaultSession();
  return (
    <>
      <DesktopVaultSync />
      <Editor key={session} />
    </>
  );
}

it("resets drafts and old vault data before navigating, even when the picker cache is already updated", async () => {
  const screen = await render(<Shell />, { route: "/entities/old/edit" });
  await expect.poll(() => Boolean(state.listener)).toBe(true);
  await screen.getByRole("button", { name: "Seed cache" }).click();
  await screen.getByRole("textbox", { name: "Draft" }).fill("Unsaved draft");
  await expect.poll(() => state.dirty.mock.calls.at(-1)).toEqual([true]);
  await screen.getByRole("button", { name: "Switch from manager" }).click();
  await expect.element(screen.getByText("/", { exact: true })).toBeVisible();
  await expect.element(screen.getByRole("textbox", { name: "Draft" })).toHaveValue("");
  await expect.element(screen.getByText("Cached entity: undefined", { exact: true })).toBeVisible();
  await expect.element(screen.getByText("unblocked", { exact: true })).toBeVisible();
});

it("preserves drafts when the remembered vault list changes without changing the active vault", async () => {
  const screen = await render(<Shell />, { route: "/entities/old/edit" });
  await expect.poll(() => Boolean(state.listener)).toBe(true);
  await screen.getByRole("textbox", { name: "Draft" }).fill("Keep this draft");
  await screen.getByRole("button", { name: "Forget inactive vault" }).click();
  await expect
    .element(screen.getByRole("textbox", { name: "Draft" }))
    .toHaveValue("Keep this draft");
  await expect.element(screen.getByText("/entities/old/edit", { exact: true })).toBeVisible();
});
