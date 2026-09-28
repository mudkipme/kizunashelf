import { createContext, type ReactNode, useContext, useEffect, useState } from "react";
import { useLocation, useNavigationType } from "react-router-dom";

const HistoryPositionContext = createContext({ canGoBack: false, canGoForward: false });

/**
 * Where the app sits in its own history, so back and forward can be shown
 * disabled rather than silently doing nothing.
 * Mount this above the routes: each page owns a separate AppFrame, which
 * remounts on navigation and must not own the history stack.
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
export function HistoryPositionProvider({ children }: { children: ReactNode }) {
  const navigationType = useNavigationType();
  const { key } = useLocation();
  const [history, setHistory] = useState(() => ({ stack: [key], position: 0 }));

  useEffect(() => {
    setHistory((current) => {
      const { stack, position } = current;
      // Ignore the initial entry and effect replays. Keep the whole update pure
      // so StrictMode cannot mutate the stack twice for one navigation.
      if (stack[position] === key) return current;
      if (navigationType === "REPLACE") {
        return {
          stack: stack.map((entry, index) => (index === position ? key : entry)),
          position,
        };
      }
      const existing = stack.indexOf(key);
      // A POP only ever lands on an entry that is already known; anything else
      // is a push, whatever it was labelled.
      if (navigationType === "POP" && existing !== -1) return { stack, position: existing };
      return { stack: [...stack.slice(0, position + 1), key], position: position + 1 };
    });
  }, [navigationType, key]);

  return (
    <HistoryPositionContext.Provider
      value={{
        canGoBack: history.position > 0,
        canGoForward: history.position < history.stack.length - 1,
      }}
    >
      {children}
    </HistoryPositionContext.Provider>
  );
}

export function useHistoryPosition(): { canGoBack: boolean; canGoForward: boolean } {
  return useContext(HistoryPositionContext);
}
