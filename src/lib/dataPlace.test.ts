import { afterEach, describe, expect, it } from "vitest";
import { backText, bytesLabel, currentText, diskOptions, heavyBytes, MARGIN, moveText, targetPlaces } from "./dataPlace";
import { setLang } from "./i18n";
import type { DataLocation, DiskChoice } from "./api";

const disk = (letter: string, free: number, total: number, extra: Partial<DiskChoice> = {}): DiskChoice => ({
  mount: `${letter}\\`,
  letter,
  name: "",
  total,
  free,
  removable: false,
  system: false,
  current: false,
  target: `${letter}\\Ludex`,
  ...extra,
});

const location = (extra: Partial<DataLocation> = {}): DataLocation => ({
  root: "",
  missing: false,
  cacheDir: "C:\\Users\\A\\AppData\\Roaming\\com.ludex.app",
  cacheBytes: 18_000_000,
  copiesDir: "C:\\Users\\A\\Documents\\Ludex\\Sauvegardes",
  copiesBytes: 2_000_000_000,
  copiesSeparate: false,
  keptDir: "C:\\Users\\A\\AppData\\Roaming\\com.ludex.app",
  keptBytes: 600_000,
  defaultCache: "C:\\Users\\A\\AppData\\Roaming\\com.ludex.app",
  defaultCopies: "C:\\Users\\A\\Documents\\Ludex\\Sauvegardes",
  disks: [],
  picked: null,
  preview: false,
  ...extra,
});

describe("bytesLabel", () => {
  it("Ko, Mo, Go, To à la française", () => {
    expect(bytesLabel(0)).toBe("0\u00a0Ko");
    expect(bytesLabel(512_000)).toBe("512\u00a0Ko");
    expect(bytesLabel(6_300_000)).toBe("6,3\u00a0Mo");
    expect(bytesLabel(19_400_000)).toBe("19\u00a0Mo");
    expect(bytesLabel(2_000_000_000)).toBe("2\u00a0Go");
    expect(bytesLabel(1_500_000_000_000)).toBe("1,5\u00a0To");
  });
});

describe("heavyBytes", () => {
  it("le cache, plus les copies seulement si elles suivent", () => {
    expect(heavyBytes(location())).toBe(2_018_000_000);
    expect(heavyBytes(location({ copiesSeparate: true }))).toBe(18_000_000);
  });
});

describe("diskOptions", () => {
  it("disque de Windows en tête, disque actuel et disque trop plein non choisissables", () => {
    const loc = location({
      disks: [
        disk("E:", 1_000_000_000, 2_000_000_000_000),
        disk("D:", 800_000_000_000, 1_000_000_000_000, { name: "Jeux" }),
        disk("C:", 100_000_000_000, 500_000_000_000, { system: true, current: true }),
      ],
    });
    const options = diskOptions(loc);
    expect(options.map((o) => o.label)).toEqual(["C:", "D: Jeux", "E:"]);
    expect(options[0].tags).toEqual(["Windows", "Actuel"]);
    expect(options[0].selectable).toBe(false);
    expect(options[1].selectable).toBe(true);
    expect(options[1].detail).toBe("800\u00a0Go libres sur 1\u00a0To");
    expect(options[1].used).toBeCloseTo(0.2);
    // 1 Go libre pour 2 Go de données : refusé, et presque plein.
    expect(options[2].fits).toBe(false);
    expect(options[2].low).toBe(true);
  });

  it("la marge compte", () => {
    const loc = location({ copiesBytes: 0, cacheBytes: 10_000_000, disks: [disk("D:", 10_000_000 + MARGIN - 1, 1e12)] });
    expect(diskOptions(loc)[0].fits).toBe(false);
  });
});

describe("textes", () => {
  it("dossiers cibles, copies choisies à part gardées", () => {
    expect(targetPlaces("D:\\Ludex\\", location())).toEqual({ cache: "D:\\Ludex\\Cache", copies: "D:\\Ludex\\Sauvegardes" });
    expect(targetPlaces("D:\\Ludex", location({ copiesSeparate: true, copiesDir: "E:\\Copies" })).copies).toBe("E:\\Copies");
    expect(moveText(location(), "D:\\Ludex")).toBe("2\u00a0Go à déplacer. Cache dans D:\\Ludex\\Cache. Copies de sauvegardes dans D:\\Ludex\\Sauvegardes.");
    expect(moveText(location({ copiesSeparate: true }), "D:\\Ludex")).toContain("restent dans le dossier choisi à part");
  });

  it("emplacement actuel, introuvable", () => {
    expect(currentText(location())).toContain("origine");
    expect(currentText(location({ root: "D:\\Ludex" }))).toBe("Dans D:\\Ludex.");
    expect(currentText(location({ root: "F:\\Ludex", missing: true }))).toContain("introuvable");
  });
});

describe("retour à l'origine", () => {
  it("cache et copies vers les dossiers d'origine, copies choisies à part laissées", () => {
    expect(backText(location())).toBe(
      "2\u00a0Go à déplacer. Cache dans C:\\Users\\A\\AppData\\Roaming\\com.ludex.app. Copies de sauvegardes dans C:\\Users\\A\\Documents\\Ludex\\Sauvegardes.",
    );
    expect(backText(location({ copiesSeparate: true }))).toBe("18\u00a0Mo à déplacer. Cache dans C:\\Users\\A\\AppData\\Roaming\\com.ludex.app.");
  });
});

describe("en anglais", () => {
  afterEach(() => setLang("fr"));

  it("KB, MB, GB, TB avec un point décimal", () => {
    setLang("en");
    expect(bytesLabel(0)).toBe("0\u00a0KB");
    expect(bytesLabel(6_300_000)).toBe("6.3\u00a0MB");
    expect(bytesLabel(2_000_000_000)).toBe("2\u00a0GB");
    expect(bytesLabel(1_500_000_000_000)).toBe("1.5\u00a0TB");
  });

  it("disques, emplacement et déplacement", () => {
    setLang("en");
    const options = diskOptions(
      location({
        disks: [
          disk("D:", 800_000_000_000, 1_000_000_000_000),
          disk("C:", 100_000_000_000, 500_000_000_000, { system: true, current: true, removable: true }),
        ],
      }),
    );
    expect(options[0].tags).toEqual(["Windows", "Current", "Removable"]);
    expect(options[1].detail).toBe("800\u00a0GB free of 1\u00a0TB");
    expect(currentText(location())).toBe("Original location, on the Windows drive.");
    expect(currentText(location({ root: "D:\\Ludex" }))).toBe("In D:\\Ludex.");
    expect(currentText(location({ root: "F:\\Ludex", missing: true }))).toContain("can't be found");
    expect(moveText(location(), "D:\\Ludex")).toBe("2\u00a0GB to move. Cache in D:\\Ludex\\Cache. Save backups in D:\\Ludex\\Sauvegardes.");
    expect(moveText(location({ copiesSeparate: true }), "D:\\Ludex")).toBe(
      "18\u00a0MB to move. Cache in D:\\Ludex\\Cache. Save backups stay in the folder you chose separately.",
    );
  });
});
