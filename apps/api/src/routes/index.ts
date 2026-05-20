import type { Hono } from "hono";

import type { GetLibrary } from "../library-cache";
import { registerCalendarRoutes } from "./calendar";
import { registerEntityRoutes } from "./entities";
import { registerRelationRoutes } from "./relations";
import { registerSystemRoutes } from "./system";

export function registerRoutes(app: Hono, getLibrary: GetLibrary) {
  registerSystemRoutes(app, getLibrary);
  registerCalendarRoutes(app, getLibrary);
  registerEntityRoutes(app, getLibrary);
  registerRelationRoutes(app, getLibrary);
}
