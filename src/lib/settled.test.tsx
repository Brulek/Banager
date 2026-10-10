import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useSettled } from "./settled";

describe("useSettled", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts as the value it is given", () => {
    const { result } = renderHook(() => useSettled("wget", 800));
    expect(result.current).toBe("wget");
  });

  it("takes a new value only once it has stayed the same for the delay", () => {
    const { result, rerender } = renderHook(({ value }) => useSettled(value, 800), { initialProps: { value: "" } });
    rerender({ value: "w" });
    act(() => vi.advanceTimersByTime(500));
    expect(result.current).toBe("");
    // Each change starts the wait over.
    rerender({ value: "wx" });
    act(() => vi.advanceTimersByTime(500));
    expect(result.current).toBe("");
    act(() => vi.advanceTimersByTime(300));
    expect(result.current).toBe("wx");
  });

  it("never takes a value that changed back before the delay was up", () => {
    const { result, rerender } = renderHook(({ value }) => useSettled(value, 800), { initialProps: { value: "wg" } });
    rerender({ value: "wgx" });
    act(() => vi.advanceTimersByTime(400));
    rerender({ value: "wg" });
    expect(result.current).toBe("wg");
    act(() => vi.advanceTimersByTime(2000));
    expect(result.current).toBe("wg");
  });
});
