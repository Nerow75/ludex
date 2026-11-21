import { afterEach, describe, expect, it } from "vitest";
import type { Screenshot, ScreenshotGroup } from "./api";
import { allShots, arrangeFolders, byMonth, kindCounts, SHOT_KINDS, SHOT_SORTS, shotCount, shotFolders } from "./screenshots";
import { setLang } from "./i18n";

afterEach(() => setLang("fr"));

let n = 0;
const capture = (fields: Partial<Screenshot> = {}): Screenshot => {
  n += 1;
  return {
    key: `k${String(n).padStart(3, "0")}`,
    gameId: null,
    source: "steam",
    steamAppid: null,
    taken: 1_700_000_000 + n,
    size: 1000,
    width: 1920,
    height: 1080,
    video: false,
    thumbUrl: "",
    url: "",
    ...fields,
  };
};

// 15 novembre 2026, midi à Paris, et la veille.
const NOV = Date.UTC(2026, 10, 15, 11) / 1000;
const DAY = 86_400;

describe("shotFolders", () => {
  it("garde un dossier par jeu, la plus récente d'abord", () => {
    const a = capture({ gameId: "steam:1", taken: 10 });
    const b = capture({ gameId: "steam:1", taken: 30 });
    const folders = shotFolders([{ gameId: "steam:1", title: "Portal", shots: [a, b] }]);
    expect(folders).toHaveLength(1);
    expect(folders[0]).toMatchObject({ key: "steam:1", gameId: "steam:1", title: "Portal", note: null });
    expect(folders[0].shots.map((s) => s.taken)).toEqual([30, 10]);
  });

  it("éclate « Autres captures » par appid Steam et garde le reste en dernier", () => {
    const groups: ScreenshotGroup[] = [
      { gameId: "steam:1", title: "Portal", shots: [capture({ gameId: "steam:1" })] },
      {
        gameId: null,
        title: "Autres captures",
        shots: [
          capture({ steamAppid: 218, taken: 5 }),
          capture({ source: "gamebar", taken: 7 }),
          capture({ steamAppid: 218, taken: 9 }),
          capture({ steamAppid: 2676230, taken: 3 }),
        ],
      },
    ];
    const folders = shotFolders(groups);
    expect(folders.map((f) => f.key)).toEqual(["steam:1", "appid:218", "appid:2676230", "autres"]);
    expect(folders[1]).toMatchObject({ title: "Jeu Steam 218", note: "Désinstallé", gameId: null });
    expect(folders[1].shots.map((s) => s.taken)).toEqual([9, 5]);
    expect(folders[3].shots).toHaveLength(1);
  });

  it("écarte les dossiers vides", () => {
    expect(shotFolders([{ gameId: "steam:1", title: "Portal", shots: [] }, { gameId: null, title: "Autres captures", shots: [] }])).toEqual([]);
  });
});

describe("arrangeFolders", () => {
  const folders = shotFolders([
    { gameId: "a", title: "Zelda", shots: [capture({ taken: 50 })] },
    { gameId: "b", title: "alan wake", shots: [capture({ taken: 10 }), capture({ taken: 20, video: true })] },
    { gameId: "c", title: "Hades", shots: [capture({ taken: 40 }), capture({ taken: 41 }), capture({ taken: 5 })] },
    { gameId: null, title: "Autres captures", shots: [capture({ source: "gamebar", taken: 99 })] },
  ]);
  const keys = (f: { key: string }[]) => f.map((x) => x.key);

  it("trie par capture la plus récente, « Autres captures » en dernier", () => {
    expect(keys(arrangeFolders(folders, "recent", "all"))).toEqual(["a", "c", "b", "autres"]);
  });

  it("trie par titre sans tenir compte de la casse", () => {
    expect(keys(arrangeFolders(folders, "title", "all"))).toEqual(["b", "c", "a", "autres"]);
  });

  it("trie par nombre de captures, puis par titre", () => {
    expect(keys(arrangeFolders(folders, "count", "all"))).toEqual(["c", "b", "a", "autres"]);
  });

  it("filtre images et vidéos et écarte les dossiers devenus vides", () => {
    const videos = arrangeFolders(folders, "recent", "videos");
    expect(keys(videos)).toEqual(["b"]);
    expect(videos[0].shots.map((s) => s.taken)).toEqual([20]);
    const images = arrangeFolders(folders, "count", "images");
    expect(images.find((f) => f.key === "b")?.shots).toHaveLength(1);
    // Les dossiers d'origine ne sont pas touchés.
    expect(folders.find((f) => f.key === "b")?.shots).toHaveLength(2);
  });
});

describe("allShots", () => {
  it("mêle les dossiers, la plus récente d'abord", () => {
    const folders = shotFolders([
      { gameId: "a", title: "A", shots: [capture({ taken: 1 }), capture({ taken: 3 })] },
      { gameId: "b", title: "B", shots: [capture({ taken: 2 })] },
    ]);
    expect(allShots(folders).map((s) => s.taken)).toEqual([3, 2, 1]);
  });
});

describe("byMonth", () => {
  it("range par mois en heure locale et note le début de chaque mois", () => {
    const shots = [
      capture({ taken: NOV }),
      capture({ taken: NOV - DAY }),
      // 31 octobre 2026 à 23 h 30 à Paris (22 h 30 UTC) : encore octobre.
      capture({ taken: Date.UTC(2026, 9, 31, 22, 30) / 1000 }),
      capture({ taken: Date.UTC(2025, 0, 2) / 1000 }),
    ];
    const months = byMonth(shots);
    expect(months.map((m) => [m.key, m.label, m.shots.length, m.start])).toEqual([
      ["2026-11", "Novembre 2026", 2, 0],
      ["2026-10", "Octobre 2026", 1, 2],
      ["2025-01", "Janvier 2025", 1, 3],
    ]);
  });

  it("ne renvoie rien sans capture", () => {
    expect(byMonth([])).toEqual([]);
  });
});

describe("kindCounts et shotCount", () => {
  it("compte images et vidéos", () => {
    const folders = shotFolders([{ gameId: "a", title: "A", shots: [capture(), capture({ video: true }), capture()] }]);
    expect(kindCounts(folders)).toEqual({ all: 3, images: 2, videos: 1 });
  });

  it("accorde le mot", () => {
    expect(shotCount(1)).toBe("1 capture");
    expect(shotCount(128)).toBe("128 captures");
    expect(shotCount(2, "videos")).toBe("2 vidéos");
    expect(shotCount(1, "images")).toBe("1 image");
  });
});

describe("page Captures en anglais", () => {
  it("dossiers sans jeu, mois, nombres et libellés en anglais", () => {
    setLang("en");
    const folders = shotFolders([
      { gameId: null, title: "Autres captures", shots: [capture({ steamAppid: 218, taken: 9 }), capture({ source: "gamebar", taken: 7 })] },
    ]);
    expect(folders.map((f) => [f.title, f.note])).toEqual([
      ["Steam game 218", "Uninstalled"],
      ["Other screenshots", null],
    ]);
    expect(byMonth([capture({ taken: NOV })]).map((m) => m.label)).toEqual(["November 2026"]);
    expect(shotCount(1)).toBe("1 screenshot");
    expect(shotCount(1240)).toBe("1,240 screenshots");
    expect(shotCount(0, "videos")).toBe("0 videos");
    expect(shotCount(1, "images")).toBe("1 image");
    expect(SHOT_SORTS.map((s) => s.label)).toEqual(["Recent", "A → Z", "Count"]);
    expect(SHOT_KINDS.map((k) => k.label)).toEqual(["All", "Images", "Videos"]);
  });

  it("libellés relus à chaque accès : retour au français", () => {
    setLang("en");
    expect(SHOT_SORTS[0].label).toBe("Recent");
    setLang("fr");
    expect(SHOT_SORTS[0].label).toBe("Récentes");
    expect(SHOT_KINDS.map((k) => k.id)).toEqual(["all", "images", "videos"]);
  });
});
