//! The app's navigation destinations, in the order the sidebar shows them.
//!
//! One list, three readers: the sidebar renders it, the command palette offers
//! it, and `mod+1`…`mod+9` index straight into it — so the number a user learns
//! from the palette is the position they see in the sidebar, and a destination
//! can never be added to one and forgotten in the others.
//!
//! Labels are `msg` descriptors rather than `<Trans>` because two of those three
//! readers are not JSX. The msgids are the same strings either way, so the
//! existing catalog entries carry over untouched.

import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import {
  ActivityIcon,
  BarChart3Icon,
  CalendarDaysIcon,
  ClipboardCheckIcon,
  DatabaseIcon,
  DownloadIcon,
  HomeIcon,
  ListIcon,
  type LucideIcon,
  SettingsIcon,
} from "lucide-react";

export type NavDestination = {
  to: string;
  label: MessageDescriptor;
  icon: LucideIcon;
  /** `primary` is the sidebar's top group; `secondary` sits at the bottom. */
  group: "primary" | "secondary";
  /** Match the path exactly (only "/", which is a prefix of everything). */
  end?: boolean;
};

export const navDestinations: NavDestination[] = [
  { to: "/", label: msg`Home`, icon: HomeIcon, group: "primary", end: true },
  { to: "/library", label: msg`Library`, icon: DatabaseIcon, group: "primary" },
  { to: "/calendar", label: msg`Calendar`, icon: CalendarDaysIcon, group: "primary" },
  { to: "/activity", label: msg`Activity`, icon: ActivityIcon, group: "primary" },
  { to: "/lists", label: msg`Lists`, icon: ListIcon, group: "primary" },
  { to: "/entities/import", label: msg`Import`, icon: DownloadIcon, group: "secondary" },
  { to: "/statistics", label: msg`Statistics`, icon: BarChart3Icon, group: "secondary" },
  { to: "/review", label: msg`Review`, icon: ClipboardCheckIcon, group: "secondary" },
  { to: "/settings", label: msg`Settings`, icon: SettingsIcon, group: "secondary" },
];

/**
 * The destination a path belongs to, or `undefined` for one that belongs to
 * none — an entity page, or Home, which is a prefix of everything and so is
 * matched only exactly.
 *
 * Detail routes resolve to their section (`/lists/reading` → Lists), which is
 * what makes the window title read as a place rather than going blank.
 */
export function destinationForPath(pathname: string): NavDestination | undefined {
  return navDestinations.find(
    (destination) =>
      !destination.end &&
      (pathname === destination.to || pathname.startsWith(`${destination.to}/`)),
  );
}
