import { toggleTask } from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";

/// Checks/unchecks one Markdown task item (`- [ ]`) written in an entity's notes
/// through `/tasks/toggle`. Checking stamps `date` as the item's Obsidian Tasks
/// `✅` completion date; unchecking clears it. `date` is the client's local date,
/// so the stamp matches the user's day rather than a UTC server clock.
///
/// The item is located by its 1-based `line` in the rendered `notesBody` plus that
/// line's source `text`, which the core verifies before writing — a locator that
/// has drifted fails instead of toggling a different item. Returns the refreshed
/// entity detail. The episodes list has its own write path and is never touched.
export function setTaskDone(
  id: string,
  args: { revision: string; line: number; text: string; done: boolean; date: string },
) {
  return toggleTask(
    id,
    {
      revision: args.revision,
      line: args.line,
      text: args.text,
      done: args.done,
      date: args.date,
    },
    undefined,
    apiFetch,
  );
}
