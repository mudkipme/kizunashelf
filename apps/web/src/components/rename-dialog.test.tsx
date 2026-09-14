import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { userEvent } from "vitest/browser";

import { RenameDialog } from "@/components/rename-dialog";
import { render } from "@/test/render";

function Harness({ onRename }: { onRename: (name: string) => void }) {
  const [open, setOpen] = useState(true);
  return (
    <>
      <button onClick={() => setOpen(true)}>Open</button>
      <RenameDialog
        open={open}
        onOpenChange={setOpen}
        title="Rename list"
        currentName="Original"
        saving={false}
        disabled={false}
        onRename={onRename}
      />
    </>
  );
}

describe("shared rename dialog", () => {
  it("rejects unchanged, empty and invalid names, and submits a normalized name", async () => {
    const rename = vi.fn();
    const screen = await render(<Harness onRename={rename} />);
    const input = screen.getByRole("textbox", { name: "Name" });
    const submit = screen.getByRole("button", { name: "Rename", exact: true });
    await expect.element(submit).toBeDisabled();
    await input.fill("   ");
    await expect.element(submit).toBeDisabled();
    await input.fill("bad/name");
    await expect.element(input).toHaveAttribute("aria-invalid", "true");
    await expect.element(submit).toBeDisabled();
    await input.fill("  Revised  ");
    await userEvent.keyboard("{Enter}");
    expect(rename).toHaveBeenCalledExactlyOnceWith("Revised");
  });

  it("discards a cancelled name when reopened", async () => {
    const screen = await render(<Harness onRename={vi.fn()} />);
    await screen.getByRole("textbox", { name: "Name" }).fill("Unsaved");
    await screen.getByRole("button", { name: "Cancel" }).click();
    await screen.getByRole("button", { name: "Open", exact: true }).click();
    await expect.element(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Original");
  });

  it("blocks submission and dismissal while saving", async () => {
    const rename = vi.fn();
    const close = vi.fn();
    await render(
      <RenameDialog
        open
        onOpenChange={close}
        title="Rename"
        currentName="Original"
        saving
        disabled={false}
        onRename={rename}
      />,
    );
    document
      .querySelector("[role=dialog] form")!
      .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await userEvent.keyboard("{Escape}");
    expect(rename).not.toHaveBeenCalled();
    expect(close).not.toHaveBeenCalled();
  });
});
