import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { VaultChangeSync } from "@/components/vault-change-sync";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({
  refresh: vi.fn(),
  wait: vi.fn(),
  error: vi.fn(),
  dismiss: vi.fn(),
}));
vi.mock("@/api/settings", () => ({ refreshLibrary: state.refresh }));
vi.mock("@/api/vault-changes", () => ({ waitForVaultChange: state.wait }));
vi.mock("sonner", () => ({ toast: { error: state.error, dismiss: state.dismiss } }));
beforeEach(() => {
  vi.clearAllMocks();
  state.wait.mockResolvedValue({ supported: false, generation: 0, changed: false });
});

it("coalesces focus events and refreshes even without a native watcher", async () => {
  let finish!: () => void;
  state.refresh.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  let invalidate!: ReturnType<typeof vi.spyOn>;
  function Probe() {
    const client = useQueryClient();
    useEffect(() => {
      invalidate = vi.spyOn(client, "invalidateQueries");
    }, [client]);
    return <VaultChangeSync />;
  }
  const screen = await render(<Probe />);
  window.dispatchEvent(new Event("focus"));
  window.dispatchEvent(new Event("focus"));
  expect(state.refresh).toHaveBeenCalledOnce();
  finish();
  await expect.poll(() => invalidate.mock.calls.length).toBe(1);
  await screen.unmount();
});

it("shows a retry after failure and ignores an old response after unmount", async () => {
  state.refresh.mockRejectedValueOnce(new Error("offline"));
  let invalidate!: ReturnType<typeof vi.spyOn>;
  function Probe() {
    const client = useQueryClient();
    useEffect(() => {
      invalidate = vi.spyOn(client, "invalidateQueries");
    }, [client]);
    return <VaultChangeSync />;
  }
  const screen = await render(<Probe />);
  window.dispatchEvent(new Event("focus"));
  await expect.poll(() => state.error.mock.calls.length).toBe(1);
  expect(invalidate).not.toHaveBeenCalled();
  let finish!: () => void;
  state.refresh.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  state.error.mock.calls[0][1].action.onClick();
  await screen.unmount();
  finish();
  await Promise.resolve();
  expect(invalidate).not.toHaveBeenCalled();
});
