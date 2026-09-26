import { QueryClient } from "@tanstack/react-query";
import { expect, it } from "vitest";

import { resetVaultSession } from "./vault-session";

it("clears all vault data and rejects a late response from the old vault", async () => {
  const client = new QueryClient();
  client.setQueryData(["entity", "same-id"], "old vault");
  let finish!: (value: string) => void;
  const old = client
    .fetchQuery({
      queryKey: ["library"],
      queryFn: () =>
        new Promise<string>((resolve) => {
          finish = resolve;
        }),
    })
    .catch(() => undefined);
  await resetVaultSession(client);
  finish("old response");
  await old;
  expect(client.getQueryData(["entity", "same-id"])).toBeUndefined();
  expect(client.getQueryData(["library"])).toBeUndefined();
  expect(
    await client.fetchQuery({ queryKey: ["entity", "same-id"], queryFn: async () => "new vault" }),
  ).toBe("new vault");
  client.clear();
});
