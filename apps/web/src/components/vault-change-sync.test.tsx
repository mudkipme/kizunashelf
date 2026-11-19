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
  let finish!: (value: { generatedAt: string; changed: boolean }) => void;
  state.refresh.mockImplementation(
    () =>
      new Promise<{ generatedAt: string; changed: boolean }>((resolve) => {
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
  expect(state.refresh).toHaveBeenCalledWith({ ifChanged: true }, expect.any(Object));
  finish({ generatedAt: "2026-11-19T00:00:00.000Z", changed: true });
  await expect.poll(() => invalidate.mock.calls.length).toBe(1);
  await screen.unmount();
});

it("skips query invalidation on focus when the vault is unchanged", async () => {
  state.refresh.mockResolvedValue({ generatedAt: "2026-11-19T00:00:00.000Z", changed: false });
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
  await expect.poll(() => state.refresh.mock.calls.length).toBe(1);
  expect(invalidate).not.toHaveBeenCalled();

  // A different generatedAt (e.g. another tab edited the vault) still invalidates.
  state.refresh.mockResolvedValueOnce({
    generatedAt: "2026-11-19T00:01:00.000Z",
    changed: false,
  });
  window.dispatchEvent(new Event("focus"));
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
  let finish!: (value: { generatedAt: string; changed: boolean }) => void;
  state.refresh.mockImplementation(
    () =>
      new Promise<{ generatedAt: string; changed: boolean }>((resolve) => {
        finish = resolve;
      }),
  );
  state.error.mock.calls[0][1].action.onClick();
  await screen.unmount();
  finish({ generatedAt: "2026-11-19T00:00:00.000Z", changed: true });
  await Promise.resolve();
  expect(invalidate).not.toHaveBeenCalled();
});
