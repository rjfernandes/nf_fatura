import { describe, expect, it } from "vitest";
import { isFutureMonth, monthRange, monthRangeUntilToday } from "./period.js";

describe("period", () => {
  it("computes month ranges, including leap years", () => {
    expect(monthRange("2026-09")).toEqual({
      from: "2026-09-01",
      to: "2026-09-30",
    });
    expect(monthRange("2028-02").to).toBe("2028-02-29");
    expect(monthRange("2026-12").to).toBe("2026-12-31");
  });
  it("caps the current month at today and leaves past months whole", () => {
    expect(monthRangeUntilToday("2026-10", new Date(2026, 9, 15))).toEqual({
      from: "2026-10-01",
      to: "2026-10-15",
    });
    expect(monthRangeUntilToday("2026-09", new Date(2026, 9, 15)).to).toBe(
      "2026-09-30",
    );
  });
  it("only rejects months that have not started", () => {
    expect(isFutureMonth("2026-10", new Date(2026, 9, 1))).toBe(false);
    expect(isFutureMonth("2026-11", new Date(2026, 9, 31))).toBe(true);
  });
});
