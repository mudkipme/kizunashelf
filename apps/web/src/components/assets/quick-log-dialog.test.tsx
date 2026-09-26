import { beforeEach, expect, it, vi } from "vitest";

import { QuickLogDialog } from "@/components/assets/quick-log-dialog";
import { render } from "@/test/render";

const state = vi.hoisted(() => ({ log: vi.fn(), revision: "r1" }));
vi.mock("@/api/log", () => ({ postLogActivity: state.log }));
vi.mock("@/api/client", () => ({
  errorMessage: (error: Error) => error.message,
  isConflictError: (error?: { status: number }) => error?.status === 409,
}));
vi.mock("@/api/invalidate-entity-data", () => ({ useInvalidateEntityData: () => vi.fn() }));
vi.mock("@/api/queries", () => ({
  queryKeys: { logPreview: (...args: string[]) => ["logPreview", ...args] },
  entityQuery: () => ({
    queryKey: ["entity"],
    queryFn: async () => ({ entity: { revision: state.revision } }),
  }),
}));
beforeEach(() => {
  state.log.mockReset();
  state.revision = "r1";
});

it("keeps the note on conflict and previews the same draft with the explicitly reloaded revision", async () => {
  state.log.mockImplementation(async (_id, request, preview) => {
    if (!preview && request.revision === "r1")
      throw Object.assign(new Error("Changed"), { status: 409 });
    return { line: request.note ?? "Log", notePath: "Daily.md" };
  });
  const close = vi.fn();
  const screen = await render(
    <QuickLogDialog open entityId="book" revision="r1" onOpenChange={close} />,
  );
  await screen.getByRole("textbox", { name: /Note/ }).fill("Episode 4");
  await expect.element(screen.getByRole("button", { name: "Log", exact: true })).toBeEnabled();
  await screen.getByRole("button", { name: "Log", exact: true }).click();
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Your edits are kept");
  expect(close).not.toHaveBeenCalled();
  state.revision = "r2";
  await screen.getByRole("button", { name: "Reload" }).click();
  await expect.element(screen.getByRole("textbox", { name: /Note/ })).toHaveValue("Episode 4");
  await expect.element(screen.getByRole("button", { name: "Log", exact: true })).toBeEnabled();
  await screen.getByRole("button", { name: "Log", exact: true }).click();
  await expect.poll(() => close.mock.calls.length).toBe(1);
  const requests = state.log.mock.calls.filter((call) => call[1].revision === "r2");
  expect(requests.length).toBeGreaterThanOrEqual(2);
  expect(requests[0][1]).toEqual(requests.at(-1)![1]);
  expect(requests[0][1].note).toBe("Episode 4");
});
