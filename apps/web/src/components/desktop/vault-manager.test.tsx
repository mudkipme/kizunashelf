import { beforeEach, expect, it, vi } from "vitest";

import { VaultManager } from "@/components/desktop/vault-manager";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({
  list: vi.fn(),
  select: vi.fn(),
  create: vi.fn(),
  open: vi.fn(),
}));

vi.mock(import("@/lib/desktop"), async (original) => ({
  ...(await original()),
  isDesktopRuntime: () => true,
  listVaults: state.list,
  selectDirectory: state.select,
  createVault: state.create,
  switchVault: state.open,
}));

beforeEach(() => {
  state.list.mockReset().mockResolvedValue([]);
  state.select.mockReset().mockResolvedValue("/vaults");
  state.create.mockReset().mockRejectedValue(new Error("Cannot create folder"));
  state.open.mockReset().mockResolvedValue([]);
});

it("offers opening or creating a vault on first launch and retains a failed creation", async () => {
  const screen = await render(<VaultManager />);
  await expect.element(screen.getByText("No vaults yet.")).toBeVisible();
  await expect.element(screen.getByRole("button", { name: "Open folder" })).toBeVisible();
  await screen.getByRole("button", { name: "Select parent folder" }).click();
  const name = screen.getByRole("textbox", { name: "Vault name" });
  await name.fill("My library");
  await screen.getByRole("button", { name: "Create vault", exact: true }).click();
  await expect.poll(() => state.create.mock.calls.length).toBe(1);
  expect(state.create).toHaveBeenCalledWith("/vaults", "My library", false);
  await expect.element(name).toHaveValue("My library");
});

it("lets the active vault be reopened in the main window", async () => {
  state.list.mockResolvedValue([{ name: "Personal", path: "/vaults/personal", active: true }]);
  const screen = await render(<VaultManager />);
  await screen.getByRole("button", { name: "Open", exact: true }).click();
  expect(state.open).toHaveBeenCalledWith("/vaults/personal", false);
});
