//! The drag handle on the sidebar's trailing edge.
//!
//! It is a `separator` in the window-splitter sense, not a decoration: the
//! same resize is reachable with the arrow keys, because a pointer-only
//! affordance would put the sidebar width out of reach for anyone navigating
//! by keyboard.

import { useRef } from "react";
import { useLingui } from "@lingui/react/macro";

import { SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH } from "@/lib/sidebar";
import { cn } from "@/lib/utils";

/// How much one arrow press moves the edge; Shift takes a coarser step.
const KEYBOARD_STEP = 16;
const KEYBOARD_STEP_COARSE = 64;

export function SidebarResizer({
  width,
  onWidth,
  onResizingChange,
  onReset,
}: {
  width: number;
  onWidth: (width: number) => void;
  /** Reports whether a pointer drag is in progress, so the sidebar can drop the
   * width easing it collapses with — see `resizing` in `AppFrame`. Keyboard
   * resizing deliberately does not report: a stepped move reads better eased. */
  onResizingChange?: (resizing: boolean) => void;
  /** Restores the default width — the escape hatch from an awkward drag. */
  onReset: () => void;
}) {
  const { t } = useLingui();
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    const step = event.shiftKey ? KEYBOARD_STEP_COARSE : KEYBOARD_STEP;
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      onWidth(width - step);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      onWidth(width + step);
    } else if (event.key === "Home") {
      event.preventDefault();
      onWidth(SIDEBAR_MIN_WIDTH);
    } else if (event.key === "End") {
      event.preventDefault();
      onWidth(SIDEBAR_MAX_WIDTH);
    } else if (event.key === "Enter") {
      event.preventDefault();
      onReset();
    }
  }

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={t`Resize sidebar`}
      aria-valuenow={width}
      aria-valuemin={SIDEBAR_MIN_WIDTH}
      aria-valuemax={SIDEBAR_MAX_WIDTH}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onDoubleClick={onReset}
      onPointerDown={(event) => {
        // Only a primary-button drag; a right-click here should do nothing.
        if (event.button !== 0) return;
        event.preventDefault();
        drag.current = { startX: event.clientX, startWidth: width };
        onResizingChange?.(true);
        // Capture so the drag survives the pointer outrunning this thin strip,
        // which at speed it always does.
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (!drag.current) return;
        onWidth(drag.current.startWidth + (event.clientX - drag.current.startX));
      }}
      onPointerUp={(event) => {
        drag.current = null;
        onResizingChange?.(false);
        event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      onPointerCancel={() => {
        drag.current = null;
        onResizingChange?.(false);
      }}
      // Sits astride the border so the grab target is comfortably wider than
      // the hairline it appears to move.
      className={cn(
        "group absolute inset-y-0 -right-1 z-10 hidden w-2 cursor-col-resize touch-none md:block",
        "focus-visible:outline-none",
      )}
    >
      {/* The line only shows on hover or focus — an always-visible grip would
          add a second vertical rule beside the border that is already there. */}
      <span
        aria-hidden="true"
        className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-primary opacity-0 transition-opacity group-hover:opacity-40 group-focus-visible:opacity-100"
      />
    </div>
  );
}
