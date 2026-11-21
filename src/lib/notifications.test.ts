import { describe, expect, it } from "vitest";
import { NOTIFICATION_ROWS } from "./notifications";

describe("NOTIFICATION_ROWS", () => {
  it("propose chaque type une seule fois, avec un libellé", () => {
    const kinds = NOTIFICATION_ROWS.map((r) => r.kind);
    expect(new Set(kinds).size).toBe(kinds.length);
    expect(kinds.sort()).toEqual(["download", "lowDisk", "news", "sale", "savesError", "update"]);
    for (const r of NOTIFICATION_ROWS) {
      expect(r.label.length).toBeGreaterThan(0);
      expect(r.detail).not.toContain("—");
    }
  });
});
