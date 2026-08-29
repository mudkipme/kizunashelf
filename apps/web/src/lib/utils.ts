import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

/**
 * `text-prose` and `text-code` are font sizes this app adds to the theme, and
 * tailwind-merge only ships the built-in scale. Left untaught it files them
 * under text *colour* instead, so a caller's `text-code` would not displace a
 * component's own `text-prose`: both would land on the element and the
 * stylesheet's ordering, rather than the caller, would decide which one won.
 */
const twMerge = extendTailwindMerge({
  extend: { classGroups: { "font-size": [{ text: ["prose", "code"] }] } },
});

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
