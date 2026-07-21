import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export type DebouncedCallback<TArgs extends unknown[]> = {
  schedule: (...args: TArgs) => void;
  cancel: () => void;
};

/**
 * A trailing-edge callback shared by searches, URL synchronization, and
 * autosave. Scheduling again replaces the pending call; unmounting cancels it.
 *
 * The latest callback is kept separately from the timer so a render does not
 * restart pending work merely because an inline callback changed identity.
 */
export function useDebouncedCallback<TArgs extends unknown[]>(
  callback: (...args: TArgs) => void,
  delayMs: number,
): DebouncedCallback<TArgs> {
  const callbackRef = useRef(callback);
  const timerRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  const cancel = useCallback(() => {
    if (timerRef.current === undefined) return;
    window.clearTimeout(timerRef.current);
    timerRef.current = undefined;
  }, []);

  const schedule = useCallback(
    (...args: TArgs) => {
      cancel();
      timerRef.current = window.setTimeout(() => {
        timerRef.current = undefined;
        callbackRef.current(...args);
      }, delayMs);
    },
    [cancel, delayMs],
  );

  useEffect(() => cancel, [cancel]);

  return useMemo(() => ({ schedule, cancel }), [schedule, cancel]);
}

/**
 * Debounces request-like work and owns its AbortController. Rescheduling or
 * unmounting aborts an in-flight invocation as well as cancelling a pending one.
 */
export function useDebouncedAbortableCallback<TArgs extends unknown[]>(
  callback: (signal: AbortSignal, ...args: TArgs) => void,
  delayMs: number,
): DebouncedCallback<TArgs> {
  const controllerRef = useRef<AbortController | undefined>(undefined);
  const { schedule: schedulePending, cancel: cancelPending } = useDebouncedCallback(
    (...args: TArgs) => {
      const controller = new AbortController();
      controllerRef.current = controller;
      callback(controller.signal, ...args);
    },
    delayMs,
  );

  const cancel = useCallback(() => {
    cancelPending();
    controllerRef.current?.abort();
    controllerRef.current = undefined;
  }, [cancelPending]);

  const schedule = useCallback(
    (...args: TArgs) => {
      cancel();
      schedulePending(...args);
    },
    [cancel, schedulePending],
  );

  useEffect(() => cancel, [cancel]);

  return useMemo(() => ({ schedule, cancel }), [schedule, cancel]);
}

/** A value that adopts the latest input after a trailing-edge delay. */
export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  const { schedule, cancel } = useDebouncedCallback(setDebounced, delayMs);

  useEffect(() => {
    schedule(value);
    return cancel;
  }, [value, schedule, cancel]);

  return debounced;
}
