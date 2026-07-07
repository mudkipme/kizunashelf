/// <reference types="vite/client" />

declare const __APP_VERSION__: string;

// Compiled by @lingui/vite-plugin at import time.
declare module "*.po" {
  import type { Messages } from "@lingui/core";
  export const messages: Messages;
}
