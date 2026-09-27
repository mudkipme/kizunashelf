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

it("preselects Finish without a journal, hides the unused note, and previews metadata effects", async () => {
  state.log.mockResolvedValue({
    willStampDate: { field: "done", value: "2026-09-20" },
    willFlipStatus: { value: "Done" },
  });
  const complete = vi.fn();
  const screen = await render(
    <QuickLogDialog
      open
      entityId="book"
      revision="r1"
      kinds={["started", "completed"]}
      initialKind="completed"
      writesNote={false}
      fieldLabel={() => "Finished on"}
      onOpenChange={vi.fn()}
      onCompleted={complete}
    />,
  );
  await expect
    .element(screen.getByRole("button", { name: "Completed", exact: true }))
    .toHaveAttribute("aria-pressed", "true");
  await expect.element(screen.getByRole("textbox", { name: /Note/ })).not.toBeInTheDocument();
  await expect.element(screen.getByText("Set Finished on to 2026-09-20")).toBeVisible();
  await expect.element(screen.getByText("Mark as Done")).toBeVisible();
  await expect.element(screen.getByText("Nothing to log for this type.")).not.toBeInTheDocument();
  await screen.getByLabelText("Date", { exact: true }).fill("2026-09-20");
  await expect.element(screen.getByRole("button", { name: "Finish", exact: true })).toBeEnabled();
  await screen.getByRole("button", { name: "Finish", exact: true }).click();
  await expect.poll(() => complete.mock.calls.length).toBe(1);
  expect(state.log.mock.calls.at(-1)?.[1]).toMatchObject({ kind: "completed", date: "2026-09-20" });
  expect(state.log.mock.calls.at(-1)?.[1].note).toBeUndefined();
});

it("keeps journal Markdown collapsed until requested", async () => {
  state.log.mockResolvedValue({ line: "- Read [[Example]]", notePath: "Daily/2026-09-27.md" });
  const screen = await render(
    <QuickLogDialog open entityId="book" revision="r1" onOpenChange={vi.fn()} />,
  );
  await expect.element(screen.getByText("Add to daily note", { exact: true })).toBeVisible();
  await expect.element(screen.getByText("- Read [[Example]]", { exact: true })).not.toBeVisible();
  await screen.getByText("Journal preview", { exact: true }).click();
  await expect.element(screen.getByText("- Read [[Example]]", { exact: true })).toBeVisible();
});

it("retains the draft if a refresh removes the available actions", async () => {
  state.log.mockResolvedValue({ line: "Journal entry" });
  const close = vi.fn();
  const screen = await render(
    <QuickLogDialog open entityId="book" revision="r1" onOpenChange={close} />,
  );
  await screen.getByRole("textbox", { name: /Note/ }).fill("Keep this draft");
  await screen.rerender(
    <QuickLogDialog open entityId="book" revision="r2" kinds={[]} onOpenChange={close} />,
  );
  await expect
    .element(screen.getByRole("textbox", { name: /Note/ }))
    .toHaveValue("Keep this draft");
  await expect.element(screen.getByText("No activity actions are available.")).toBeVisible();
  await expect.element(screen.getByRole("button", { name: "Log", exact: true })).toBeDisabled();
  expect(close).not.toHaveBeenCalled();
});
