import { describe, expect, it } from "vitest";
import type { StatsInfo } from "./api";
import { countedGames, genreStats, launcherShares, monthBars, parseMonth, peak, playtimeTotals, sourceLabel, statsEmpty, topGames } from "./stats";
import { fiche, jeu } from "../test/jeu";

const infoVide = (): StatsInfo => ({
  months: Array.from({ length: 12 }, (_, i) => ({ month: `2026-${String(i + 1).padStart(2, "0")}`, secs: 0, sessions: 0, days: 0, achievements: 0 })),
  bestStreak: 0,
  bestStreakEnd: null,
  currentStreak: 0,
  playedDays: 0,
  historySessions: 0,
  historySince: null,
});

describe("jeux comptés", () => {
  it("écarte les jeux masqués et les outils", () => {
    const games = [jeu(), jeu({ hidden: true }), jeu({ isTool: true })];
    expect(countedGames(games)).toHaveLength(1);
  });
});

describe("totaux du temps de jeu", () => {
  it("compte chaque jeu une fois, selon sa source, sans additionner les deux", () => {
    const games = [
      jeu({ playtimeMinutes: 600, playtimeSource: "launcher" }),
      jeu({ playtimeMinutes: 90, playtimeSource: "ludex", platform: "epic" }),
      jeu({ playtimeMinutes: 0, playtimeSource: "launcher" }),
      jeu({ playtimeMinutes: null }),
      jeu({ playtimeMinutes: 5000, playtimeSource: "launcher", hidden: true }),
    ];
    expect(playtimeTotals(games)).toEqual({ minutes: 690, played: 2, fromLauncher: 1, fromLudex: 1, unmeasured: 1 });
  });
});

describe("top des jeux", () => {
  it("classe par temps puis par titre, dix au plus, sans les jeux jamais joués", () => {
    const games = [
      jeu({ title: "Bravo", playtimeMinutes: 120 }),
      jeu({ title: "alpha", playtimeMinutes: 120 }),
      jeu({ title: "Long", playtimeMinutes: 900 }),
      jeu({ title: "Rien", playtimeMinutes: 0 }),
      jeu({ title: "Inconnu", playtimeMinutes: null }),
      ...Array.from({ length: 12 }, (_, i) => jeu({ title: `Petit ${i}`, playtimeMinutes: 10 + i })),
    ];
    const top = topGames(games);
    expect(top).toHaveLength(10);
    expect(top.slice(0, 3).map((g) => g.title)).toEqual(["Long", "alpha", "Bravo"]);
    expect(top.some((g) => g.title === "Rien" || g.title === "Inconnu")).toBe(false);
  });

  it("dit d'où vient le temps", () => {
    expect(sourceLabel(jeu({ playtimeSource: "launcher" }))).toBe("Steam");
    expect(sourceLabel(jeu({ playtimeSource: "launcher", platform: "gog" }))).toBe("GOG Galaxy");
    expect(sourceLabel(jeu({ playtimeSource: "ludex", platform: "epic" }))).toBe("Mesuré par Ludex");
    expect(sourceLabel(jeu())).toBe("Non mesuré");
  });
});

describe("répartition par launcher", () => {
  it("fait 100 % et range du plus joué au moins joué", () => {
    const games = [
      jeu({ playtimeMinutes: 300 }),
      jeu({ playtimeMinutes: 100 }),
      jeu({ platform: "riot", playtimeMinutes: 100, playtimeSource: "ludex" }),
      jeu({ platform: "epic", playtimeMinutes: null }),
    ];
    const shares = launcherShares(games);
    expect(shares.map((s) => [s.key, s.minutes, s.games])).toEqual([
      ["steam", 400, 2],
      ["riot", 100, 1],
    ]);
    expect(shares[0].share).toBeCloseTo(0.8);
    expect(shares.reduce((n, s) => n + s.share, 0)).toBeCloseTo(1);
  });

  it("est vide sans temps de jeu", () => {
    expect(launcherShares([jeu()])).toEqual([]);
  });
});

describe("répartition par genre", () => {
  it("compte un jeu dans chacun de ses genres, corrections comprises, part du plus joué", () => {
    const games = [
      jeu({ playtimeMinutes: 600, details: fiche({ genres: ["RPG", "Action"] }) }),
      jeu({ playtimeMinutes: 200, details: fiche({ genres: ["Action"] }) }),
      jeu({ playtimeMinutes: 50, details: fiche({ genres: ["Course"] }), customGenres: true }),
      jeu({ playtimeMinutes: null, details: fiche({ genres: ["Stratégie"] }) }),
      jeu({ playtimeMinutes: 500 }),
      jeu({ details: fiche({ genres: [] }) }),
    ];
    const { genres, withoutGenre } = genreStats(games);
    expect(genres.map((g) => [g.label, g.minutes, g.games])).toEqual([
      ["Action", 800, 2],
      ["RPG", 600, 1],
      ["Course", 50, 1],
      ["Stratégie", 0, 1],
    ]);
    expect(genres[0].share).toBe(1);
    expect(genres[1].share).toBeCloseTo(0.75);
    expect(withoutGenre).toBe(2);
  });
});

describe("mois", () => {
  it("lit un mois AAAA-MM en heure locale", () => {
    const d = parseMonth("2026-03");
    expect([d.getFullYear(), d.getMonth(), d.getDate()]).toEqual([2026, 2, 1]);
  });

  it("fait une barre par mois, le dernier étant le mois en cours", () => {
    const info = infoVide();
    info.months[2].secs = 3600;
    info.months[11].achievements = 4;
    const bars = monthBars(info.months, (m) => m.secs);
    expect(bars).toHaveLength(12);
    expect(bars[0]).toMatchObject({ label: "janv", title: "Janvier 2026", value: 0, current: false });
    expect(bars[2].value).toBe(3600);
    expect(bars[11]).toMatchObject({ label: "déc", current: true });
    expect(monthBars(info.months, (m) => m.achievements)[11].value).toBe(4);
  });

  it("trouve le mois le plus haut, le plus récent à égalité", () => {
    const info = infoVide();
    expect(peak(monthBars(info.months, (m) => m.secs))).toBeNull();
    info.months[3].secs = 500;
    info.months[7].secs = 500;
    expect(peak(monthBars(info.months, (m) => m.secs))?.title).toBe("Août 2026");
  });
});

describe("état vide", () => {
  it("vide seulement sans temps, sans partie mesurée ni succès", () => {
    const info = infoVide();
    expect(statsEmpty([jeu()], info)).toBe(true);
    expect(statsEmpty([jeu({ playtimeMinutes: 10 })], info)).toBe(false);
    expect(statsEmpty([], { ...info, historySessions: 1 })).toBe(false);
    info.months[0].achievements = 1;
    expect(statsEmpty([], info)).toBe(false);
  });
});
