import type { Hono } from "hono";

import { buildCalendar } from "../calendar";
import type { GetLibrary } from "../library-cache";
import { clampNumber } from "../utils";

export function registerCalendarRoutes(app: Hono, getLibrary: GetLibrary) {
  app.get("/api/calendar", async (c) => {
    const library = await getLibrary();
    const now = new Date();
    const year = clampNumber(Number(c.req.query("year") ?? now.getFullYear()), 1970, 2100);
    const month = clampNumber(Number(c.req.query("month") ?? now.getMonth() + 1), 1, 12);
    const type = c.req.query("type");
    const source = c.req.query("source");

    return c.json(
      await buildCalendar(library, {
        year,
        month,
        type: type && type !== "all" ? type : undefined,
        source: source === "taxonomy" || source === "daily-note" ? source : "all",
      }),
    );
  });
}
