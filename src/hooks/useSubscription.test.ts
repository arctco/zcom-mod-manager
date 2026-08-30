// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { useSubscription } from "./useSubscription";

describe("useSubscription", () => {
  it("unsubscribes a handle that arrives after cleanup", async () => {
    const unsubscribe = vi.fn();
    let resolve: (fn: () => void) => void = () => {};
    const subscribe = () => new Promise<() => void>(done => { resolve = done; });

    const { unmount } = renderHook(() => useSubscription(subscribe, []));
    unmount();
    resolve(unsubscribe);
    await Promise.resolve();

    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });

  it("unsubscribes once on cleanup when the handle arrived in time", async () => {
    const unsubscribe = vi.fn();
    const subscribe = () => Promise.resolve(unsubscribe);

    const { unmount } = renderHook(() => useSubscription(subscribe, []));
    await Promise.resolve();
    expect(unsubscribe).not.toHaveBeenCalled();

    unmount();
    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });
});
