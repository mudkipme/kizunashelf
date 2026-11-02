import { beforeEach, expect, it, vi } from "vitest";

import { VaultPicker } from "@/components/desktop/vault-picker";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({ list: vi.fn(), switch: vi.fn(), manage: vi.fn() }));
const vaults = [
  { name: "Personal", path: "/vaults/personal", active: true },
  { name: "Work", path: "/vaults/work", active: false },
];
const switched = vaults.map((vault) => ({ ...vault, active: vault.name === "Work" }));

vi.mock(import("@/lib/desktop"), async (original) => ({
  ...(await original()),
  isDesktopRuntime: () => true,
  listVaults: state.list,
  switchVault: state.switch,
  openVaultManager: state.manage,
}));

beforeEach(() => {
  state.list.mockReset().mockResolvedValue(vaults);
  state.switch.mockReset().mockResolvedValue(switched);
  state.manage.mockReset().mockResolvedValue(undefined);
});

it("shows the active vault and switches directly from the sidebar menu", async () => {
  const screen = await render(<VaultPicker />);
  await screen.getByRole("button", { name: "Switch vault: Personal" }).click();
  await expect.element(screen.getByRole("menuitem", { name: "Personal" })).toBeDisabled();
  await screen.getByRole("menuitem", { name: "Work", exact: true }).click();
  await expect.element(screen.getByRole("button", { name: "Switch vault: Work" })).toBeVisible();
  expect(state.switch).toHaveBeenCalledWith("/vaults/work", false);
});

it("keeps edits until the user explicitly confirms discarding them", async () => {
  state.switch.mockImplementation(async (_path: string, discard: boolean) => {
    if (!discard) throw "vault_has_unsaved_changes";
    return switched;
  });
  const screen = await render(<VaultPicker />);
  await screen.getByRole("button", { name: "Switch vault: Personal" }).click();
  await screen.getByRole("menuitem", { name: "Work", exact: true }).click();
  await expect.element(screen.getByRole("alertdialog")).toBeVisible();
  await screen.getByRole("button", { name: "Keep editing" }).click();
  expect(state.switch).toHaveBeenCalledTimes(1);
  await screen.getByRole("button", { name: "Switch vault: Personal" }).click();
  await screen.getByRole("menuitem", { name: "Work", exact: true }).click();
  await screen.getByRole("button", { name: "Discard and switch" }).click();
  await expect.element(screen.getByRole("button", { name: "Switch vault: Work" })).toBeVisible();
  expect(state.switch).toHaveBeenLastCalledWith("/vaults/work", true);
});

it("opens the separate manager without switching vaults", async () => {
  const screen = await render(<VaultPicker />);
  await screen.getByRole("button", { name: "Switch vault: Personal" }).click();
  await screen.getByRole("menuitem", { name: "Manage vaults…" }).click();
  expect(state.manage).toHaveBeenCalledOnce();
  expect(state.switch).not.toHaveBeenCalled();
});
