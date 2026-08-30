import { useEffect, type DependencyList } from "react";

/**
 * Keeps an event subscription alive for exactly the lifetime of the effect.
 *
 * Tauri hands back the unsubscribe function through a promise, which regularly
 * resolves after the effect has already been cleaned up: React's development
 * double-mount tears the first pass down within the same tick. Assigning the
 * handle in `.then` and calling it from the cleanup therefore misses it, the
 * listener stays registered, and every later event is delivered once per leaked
 * subscription — a dropped archive gets inspected twice, and an `nxm://` link
 * downloads twice. Unsubscribing immediately when the handle arrives late keeps
 * exactly one listener alive.
 */
export function useSubscription(
  subscribe: () => Promise<() => void>,
  deps: DependencyList
): void {
  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    void subscribe().then(unsubscribe => {
      if (cancelled) unsubscribe();
      else stop = unsubscribe;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
