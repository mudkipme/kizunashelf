import { expect, it } from "vitest";

import { EntityDateList } from "@/components/assets/entity-date-list";
import { testEntity } from "@/test/api-stub";
import { render } from "@/test/render";

it("names each date by its schema label, not just its value", async () => {
  // Field keys are arbitrary; the label comes from the schema's display name.
  const labelsByType = new Map([
    [
      "anime",
      new Map([
        ["aired", "Premiere"],
        ["fin", "Finished on"],
      ]),
    ],
  ]);
  const entity = testEntity({
    id: "frieren",
    dates: [
      { field: "aired", value: "2023-09-29", sortKey: "2023-09-29" },
      { field: "fin", value: "2026-11-13", sortKey: "2026-11-13" },
    ],
  });
  const screen = await render(<EntityDateList entity={entity} labelsByType={labelsByType} />);
  await expect.element(screen.getByTitle(/^Premiere: /)).toHaveTextContent(/^Premiere.*2023/);
  await expect.element(screen.getByTitle(/^Finished on: /)).toHaveTextContent(/^Finished on.*2026/);
});
