import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("formats deployment sizes", () => {
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
  });
});
