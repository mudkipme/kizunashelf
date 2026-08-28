import { useEffect, useState } from "react";

/**
 * Keeps a panel mounted for the length of its exit transition.
 *
 * A panel that unmounts the instant it closes can only ever animate *in* — by
 * the time the closing frame paints there is nothing left to animate. This
 * returns the two booleans that fix that:
 *
 * - `mounted` — whether to render the panel at all. Stays `true` through the
 *   exit so the transition has something to run on, then goes `false`.
 * - `shown` — whether the panel wears its *open* classes. Flips one paint after
 *   mount on the way in (the browser needs a closed state to transition away
 *   from) and immediately on the way out.
 *
 * The panel is genuinely gone at rest, not merely translated off-screen or
 * `visibility: hidden`, which keeps it out of the accessibility tree and out of
 * anything that counts what's on screen.
 *
 * `durationMs` must match the CSS duration; a mismatch either cuts the exit
 * short or leaves the panel mounted after it finishes.
 */
export function useTransitionPresence(open: boolean, durationMs: number) {
  const [mounted, setMounted] = useState(open);
  const [shown, setShown] = useState(open);

  useEffect(() => {
    if (open) {
      setMounted(true);
      // Two frames, not one: a single `requestAnimationFrame` can still land in
      // the same paint as the mount, and a transition between two values set in
      // one paint never runs. Both ids are tracked because cancelling the outer
      // request does nothing about the inner one it already scheduled.
      let inner = 0;
      const outer = requestAnimationFrame(() => {
        inner = requestAnimationFrame(() => setShown(true));
      });
      return () => {
        cancelAnimationFrame(outer);
        cancelAnimationFrame(inner);
      };
    }

    setShown(false);
    const timer = setTimeout(() => setMounted(false), durationMs);
    return () => clearTimeout(timer);
  }, [open, durationMs]);

  return { mounted, shown };
}
