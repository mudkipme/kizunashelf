import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigationType } from "react-router-dom";

/**
 * Where the app sits in its own history, so back and forward can be shown
 * disabled rather than silently doing nothing.
 *
 * The History API deliberately will not tell a page whether there is anything
 * ahead of it — `history.length` counts the whole session, not the forward
 * stack — so the stack is tracked here from what the router reports: the key of
 * the entry now showing, and how it was reached. A PUSH truncates everything
 * ahead of the current entry and appends; a POP moves to an entry already in
 * the stack; a REPLACE swaps the current one in place.
 *
 * Keyed off the router rather than `window.history.state.idx` so it holds for
 * any router — including the in-memory one, which never touches the browser's
 * history at all.
 */
export function useHistoryPosition(): { canGoBack: boolean; canGoForward: boolean } {
  const navigationType = useNavigationType();
  const { key } = useLocation();
  const stack = useRef<string[]>([key]);
  const [position, setPosition] = useState(0);

  useEffect(() => {
    setPosition((current) => {
      const existing = stack.current.indexOf(key);
      if (navigationType === "REPLACE") {
        stack.current = stack.current.map((entry, index) => (index === current ? key : entry));
        return current;
      }
      // A POP only ever lands on an entry that is already known; anything else
      // is a push, whatever it was labelled.
      if (navigationType === "POP" && existing !== -1) return existing;
      stack.current = [...stack.current.slice(0, current + 1), key];
      return stack.current.length - 1;
    });
  }, [navigationType, key]);

  return { canGoBack: position > 0, canGoForward: position < stack.current.length - 1 };
}
