import { afterEach, describe, expect, it } from "vitest";
import { NOTIFICATION_ROWS } from "./notifications";
import { setLang } from "./i18n";

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

describe("en anglais", () => {
  afterEach(() => setLang("fr"));

  it("libellés relus dans la langue du moment", () => {
    expect(NOTIFICATION_ROWS[0].label).toBe("Téléchargement terminé");
    setLang("en");
    expect(NOTIFICATION_ROWS[0].label).toBe("Download finished");
    for (const r of NOTIFICATION_ROWS) {
      expect(r.label.length).toBeGreaterThan(0);
      expect(r.detail).not.toContain("—");
    }
  });
});
