//! The shape a list is edited in, and the signature used to tell an edited list
//! from the one the server last handed back.
//!
//! Sections and items carry a stable `key` while being edited so drag-and-drop
//! and React reconciliation have an identity that survives reordering — the key
//! is local only and never round-trips to Markdown.

import type { ListItem, ListMarker, ListSection } from "@/types/api";

export type EditableItem = ListItem & { key: string };
export type EditableSection = {
  key: string;
  heading: string | null;
  marker: ListMarker;
  items: EditableItem[];
};

// A stable-ish signature of the editable sections, for dirty-tracking and for
// comparing local edits against the server's last-loaded state. Only the parts
// that round-trip to Markdown matter (heading, marker, item order + task state).
export function sectionsSignature(
  sections: Array<{ heading: string | null; marker: ListMarker; items: Array<{ text: string; checked?: boolean | null }> }>,
) {
  return JSON.stringify(
    sections.map((section) => ({
      heading: section.heading,
      marker: section.marker,
      items: section.items.map((item) => ({ text: item.text, checked: item.checked ?? null })),
    })),
  );
}

export function serverSections(sections: ListSection[]) {
  return sections.map((section) => ({
    heading: section.heading ?? null,
    marker: section.marker,
    items: section.items,
  }));
}
