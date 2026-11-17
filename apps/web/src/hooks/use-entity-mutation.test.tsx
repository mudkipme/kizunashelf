import { afterEach, expect, it, vi } from "vitest";

import { useEntityMutation } from "@/hooks/use-entity-mutation";
import { render } from "@/test/render";

vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

const pending: Array<{ name: string; resolve: () => void; reject: (error: Error) => void }> = [];
const started: string[] = [];

afterEach(() => {
  pending.length = 0;
  started.length = 0;
});

function write(name: string) {
  return () =>
    new Promise<void>((resolve, reject) => {
      started.push(name);
      pending.push({ name, resolve, reject });
    });
}

function Harness() {
  const { saving, run } = useEntityMutation();
  return (
    <>
      <button onClick={() => void run(write("episode"))}>Episode</button>
      <button onClick={() => void run(write("task"))}>Task</button>
      <button
        onClick={() => {
          // Two writes in the same tick, before React re-renders with `saving`.
          void run(write("first"));
          void run(write("second"));
        }}
      >
        Both
      </button>
      <output data-testid="saving">{String(saving)}</output>
    </>
  );
}

it("refuses a second write to the entity while one is in flight", async () => {
  const screen = await render(<Harness />);
  await screen.getByRole("button", { name: "Episode" }).click();
  await expect.element(screen.getByTestId("saving")).toHaveTextContent("true");
  await screen.getByRole("button", { name: "Task" }).click();
  expect(started).toEqual(["episode"]);

  pending[0].resolve();
  await expect.element(screen.getByTestId("saving")).toHaveTextContent("false");
  await screen.getByRole("button", { name: "Task" }).click();
  expect(started).toEqual(["episode", "task"]);
});

it("refuses a second write started in the same tick", async () => {
  const screen = await render(<Harness />);
  await screen.getByRole("button", { name: "Both" }).click();
  expect(started).toEqual(["first"]);
});

it("frees the slot after a failed write", async () => {
  const screen = await render(<Harness />);
  await screen.getByRole("button", { name: "Episode" }).click();
  pending[0].reject(new Error("offline"));
  await expect.element(screen.getByTestId("saving")).toHaveTextContent("false");
  await screen.getByRole("button", { name: "Task" }).click();
  expect(started).toEqual(["episode", "task"]);
});
