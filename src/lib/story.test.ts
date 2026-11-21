import { describe, expect, it } from "vitest";
import type { RareUnlock, Recap } from "./api";
import { hasStory, MAX_PAGES, periodTitle, playerProfile, rarityText, stepPage, storyAction, storyComparison, storyPages, type StoryContext } from "./story";

const H = 3600;

/** Récap de test : une semaine passée, sans rien, à compléter. */
function recap(fields: Partial<Recap> = {}): Recap {
  return {
    period: "week",
    back: 1,
    start: "2026-11-09",
    end: "2026-11-15",
    elapsedDays: 7,
    totalSecs: 0,
    previousSecs: 0,
    previousToDateSecs: 0,
    sessions: 0,
    games: 0,
    top: [],
    longest: null,
    bestStreak: 0,
    currentStreak: 0,
    weekdaySecs: [0, 0, 0, 0, 0, 0, 0],
    favoriteWeekday: null,
    momentSecs: [0, 0, 0, 0],
    favoriteMoment: null,
    days: [0, 0, 0, 0, 0, 0, 0],
    newGames: [],
    achievements: 0,
    achievementGames: [],
    historySessions: 10,
    historySince: 1_700_000_000,
    ...fields,
  };
}

const titres: Record<string, string> = { "steam:1": "Hades", "steam:2": "Celeste", "steam:3": "Terraria" };
const ctx = (fields: Partial<StoryContext> = {}): StoryContext => ({
  title: (id) => titres[id] ?? "Jeu retiré de la bibliothèque",
  known: (id) => id in titres,
  rarest: null,
  ...fields,
});

/** Semaine bien remplie : toutes les pages ont leur donnée. */
const pleine = recap({
  totalSecs: 20 * H,
  previousSecs: 10 * H,
  sessions: 8,
  games: 3,
  top: [
    { gameId: "steam:1", secs: 12 * H, sessions: 4 },
    { gameId: "steam:2", secs: 6 * H, sessions: 3 },
    { gameId: "steam:3", secs: 2 * H, sessions: 1 },
  ],
  longest: { gameId: "steam:2", start: Date.UTC(2026, 10, 14, 19, 30) / 1000, secs: 4 * H },
  bestStreak: 4,
  weekdaySecs: [H, 2 * H, 0, 3 * H, 2 * H, 8 * H, 4 * H],
  favoriteWeekday: 5,
  momentSecs: [2 * H, 2 * H, 6 * H, 10 * H],
  favoriteMoment: 3,
  newGames: [{ gameId: "steam:3", firstPlayed: Date.UTC(2026, 10, 12, 18) / 1000 }],
  achievements: 7,
  achievementGames: [
    { gameId: "steam:1", count: 5 },
    { gameId: "steam:2", count: 2 },
  ],
});

const rare: RareUnlock = { gameId: "steam:2", name: "Cœur de cristal", description: null, icon: null, percent: 1.24, unlockedAt: 0 };

describe("pages de la story", () => {
  it("aucune story sans temps de jeu", () => {
    const vide = recap({ achievements: 4, achievementGames: [{ gameId: "steam:1", count: 4 }] });
    expect(hasStory(vide)).toBe(false);
    expect(storyPages(vide, ctx())).toEqual([]);
  });

  it("une semaine pleine donne les neuf pages dans l'ordre", () => {
    const pages = storyPages(pleine, ctx({ rarest: rare }));
    expect(pages.map((p) => p.kind)).toEqual(["total", "top", "rhythm", "longest", "streak", "discoveries", "achievements", "profile", "end"]);
    expect(pages.length).toBe(MAX_PAGES);
    expect(pages[0]).toMatchObject({ eyebrow: "9 au 15 novembre 2026", value: "20 h", title: "de jeu cette semaine-là", detail: "+100 % par rapport à la semaine précédente", gameId: "steam:1" });
    expect(pages[1].title).toBe("Ton top 3");
    expect(pages[1].items.map((i) => [i.label, i.value, i.share])).toEqual([
      ["Hades", "12 h", 1],
      ["Celeste", "6 h", 0.5],
      ["Terraria", "2 h", 2 / 12],
    ]);
    expect(pages[2]).toMatchObject({ value: "Samedi", detail: "Et surtout le soir, de 18 h à minuit", gameId: "steam:2" });
    expect(pages[2].bars.filter((b) => b.active).map((b) => b.label)).toEqual(["S"]);
    expect(pages[3]).toMatchObject({ value: "4 h", title: "Celeste", detail: "Samedi 14 novembre, à partir de 20:30", logo: true });
    expect(pages[4]).toMatchObject({ value: "4 jours", detail: "Ta plus longue série cette semaine-là" });
    expect(pages[5]).toMatchObject({ value: "1", title: "nouveau jeu lancé", gameId: "steam:3" });
    expect(pages[5].items[0]).toMatchObject({ label: "Terraria", value: "12 novembre" });
    expect(pages[6]).toMatchObject({ value: "7", title: "succès débloqués", detail: "dont 5 dans Hades", achievement: rare, gameId: "steam:2" });
    expect(pages[8]).toMatchObject({ title: "Fin du récap", detail: "8 sessions · 3 jeux" });
  });

  it("une seule session : ni rythme, ni plus longue session, ni série", () => {
    const une = recap({
      totalSecs: 2 * H,
      sessions: 1,
      games: 1,
      top: [{ gameId: "steam:1", secs: 2 * H, sessions: 1 }],
      longest: { gameId: "steam:1", start: 1_763_000_000, secs: 2 * H },
      bestStreak: 1,
      weekdaySecs: [0, 0, 2 * H, 0, 0, 0, 0],
      favoriteWeekday: 2,
      momentSecs: [0, 0, 2 * H, 0],
      favoriteMoment: 2,
    });
    const pages = storyPages(une, ctx());
    expect(pages.map((p) => p.kind)).toEqual(["total", "top", "profile", "end"]);
    expect(pages[1].title).toBe("Ton jeu cette semaine-là");
  });

  it("jeu retiré de la bibliothèque : pas d'image de fond, titre de repli", () => {
    const pages = storyPages(recap({ totalSecs: H, sessions: 1, games: 1, top: [{ gameId: "steam:99", secs: H, sessions: 1 }] }), ctx());
    expect(pages[0].gameId).toBeNull();
    expect(pages[1].items[0].label).toBe("Jeu retiré de la bibliothèque");
  });

  it("période en cours : comparaison au même stade et fin qui donne rendez-vous", () => {
    const enCours = recap({ back: 0, elapsedDays: 3, totalSecs: 3 * H, previousToDateSecs: 4 * H, previousSecs: 9 * H, sessions: 2, games: 1, currentStreak: 3, bestStreak: 3 });
    const pages = storyPages(enCours, ctx());
    expect(pages[0].title).toBe("de jeu cette semaine");
    expect(pages[0].detail).toBe("−25 % par rapport à la semaine précédente au même stade");
    expect(pages.find((p) => p.kind === "streak")?.detail).toBe("Série en cours : 3 jours");
    expect(pages[pages.length - 1].title).toBe("À la semaine prochaine");
    const annee = storyPages({ ...enCours, period: "year", start: "2026-01-01", end: "2026-12-31", days: Array(365).fill(0) }, ctx());
    expect(annee[annee.length - 1].title).toBe("À l'année prochaine");
  });

  it("succès sans le plus rare connu, plus de cinq nouveaux jeux", () => {
    const news = Array.from({ length: 7 }, (_, i) => ({ gameId: `steam:${i + 1}`, firstPlayed: 1_763_000_000 + i }));
    const pages = storyPages(recap({ totalSecs: H, sessions: 1, games: 7, newGames: news, achievements: 1, achievementGames: [{ gameId: "steam:1", count: 1 }] }), ctx());
    const disc = pages.find((p) => p.kind === "discoveries")!;
    expect(disc.items.length).toBe(5);
    expect(disc.detail).toBe("Les 5 premiers, et 2 autres");
    const ach = pages.find((p) => p.kind === "achievements")!;
    expect(ach).toMatchObject({ title: "succès débloqué", detail: "dans Hades", achievement: null });
  });
});

describe("profil de joueur", () => {
  const titre = (id: string) => titres[id] ?? "?";

  it("le noctambule quand la nuit domine", () => {
    const p = playerProfile(recap({ totalSecs: 10 * H, sessions: 5, games: 3, momentSecs: [6 * H, 0, 1 * H, 3 * H] }), titre);
    expect(p).toEqual({ id: "night", title: "Le noctambule", line: "60 % de ton temps de jeu entre minuit et 6 h" });
  });

  it("le marathonien pour de longues sessions", () => {
    const p = playerProfile(
      recap({ totalSecs: 9 * H, sessions: 2, games: 2, longest: { gameId: "steam:1", start: 0, secs: 6 * H }, momentSecs: [0, 0, 5 * H, 4 * H] }),
      titre,
    );
    expect(p.id).toBe("marathon");
    expect(p.line).toBe("4 h 30 par session en moyenne, 6 h pour la plus longue");
  });

  it("l'explorateur quand beaucoup de jeux sont nouveaux", () => {
    const news = [1, 2, 3].map((i) => ({ gameId: `steam:${i}`, firstPlayed: i }));
    const p = playerProfile(recap({ totalSecs: 3 * H, sessions: 3, games: 3, newGames: news, momentSecs: [0, 0, H, 2 * H] }), titre);
    expect(p).toMatchObject({ id: "explorer", title: "L'explorateur", line: "3 nouveaux jeux lancés cette semaine-là" });
  });

  it("l'explorateur demande que les nouveaux jeux pèsent", () => {
    const news = [1, 2].map((i) => ({ gameId: `steam:${i}`, firstPlayed: i }));
    expect(playerProfile(recap({ totalSecs: 3 * H, sessions: 3, games: 10, newGames: news }), titre).id).not.toBe("explorer");
  });

  it("le fidèle quand un jeu prend presque tout", () => {
    const p = playerProfile(
      recap({ totalSecs: 10 * H, sessions: 6, games: 2, top: [{ gameId: "steam:1", secs: 9 * H, sessions: 5 }, { gameId: "steam:2", secs: H, sessions: 1 }], momentSecs: [0, 0, 5 * H, 5 * H] }),
      titre,
    );
    expect(p).toMatchObject({ id: "loyal", line: "90 % de ton temps sur Hades" });
  });

  it("le chasseur de succès, le régulier, le joueur du week-end, le touche-à-tout", () => {
    expect(playerProfile(recap({ totalSecs: H, sessions: 1, achievements: 30 }), titre).id).toBe("hunter");
    expect(playerProfile(recap({ totalSecs: H, sessions: 1, bestStreak: 7 }), titre).id).toBe("regular");
    expect(playerProfile(recap({ totalSecs: 4 * H, sessions: 4, games: 2, weekdaySecs: [H, 0, 0, 0, 0, 2 * H, H] }), titre).id).toBe("weekend");
    const top = [1, 2, 3, 4, 5].map((i) => ({ gameId: `steam:${i}`, secs: H, sessions: 1 }));
    expect(playerProfile(recap({ totalSecs: 6 * H, sessions: 6, games: 6, top }), titre).id).toBe("zapper");
  });

  it("les seuils grandissent avec la période", () => {
    expect(playerProfile(recap({ totalSecs: H, sessions: 1, achievements: 30, period: "year" }), titre).id).toBe("casual");
  });

  it("le joueur tranquille quand rien ne ressort", () => {
    expect(playerProfile(recap({ totalSecs: H, sessions: 2, games: 2, momentSecs: [0, 0, H, 0] }), titre)).toEqual({
      id: "casual",
      title: "Le joueur tranquille",
      line: "2 sessions, sans se presser",
    });
  });

  it("le plus haut score l'emporte", () => {
    // Nuit à 100 % (score 2,5) contre une série de 6 jours (score 1,2).
    expect(playerProfile(recap({ totalSecs: 6 * H, sessions: 6, games: 1, bestStreak: 6, momentSecs: [6 * H, 0, 0, 0] }), titre).id).toBe("night");
  });
});

describe("textes et touches", () => {
  it("titre de la période", () => {
    expect(periodTitle({ period: "week", start: "2026-11-30", end: "2026-12-06" })).toBe("30 novembre au 6 décembre 2026");
    expect(periodTitle({ period: "month", start: "2026-11-01", end: "2026-11-30" })).toBe("Novembre 2026");
    expect(periodTitle({ period: "year", start: "2026-01-01", end: "2026-12-31" })).toBe("2026");
  });

  it("comparaison", () => {
    expect(storyComparison(recap({ totalSecs: H }))).toBe("Rien de mesuré la semaine précédente");
    expect(storyComparison(recap({ totalSecs: H, previousSecs: H }))).toBe("Autant que la semaine précédente");
    expect(storyComparison(recap({ period: "month", totalSecs: 3 * H, previousSecs: 2 * H }))).toBe("+50 % par rapport au mois précédent");
  });

  it("rareté", () => {
    expect(rarityText(1.24)).toBe("Débloqué par 1,2 % des joueurs Steam");
    expect(rarityText(0.02)).toBe("Débloqué par 0,1 % des joueurs Steam ou moins");
  });

  it("touches et pages", () => {
    expect(storyAction("ArrowRight")).toBe("next");
    expect(storyAction("Enter")).toBe("next");
    expect(storyAction("ArrowLeft")).toBe("prev");
    expect(storyAction(" ")).toBe("pause");
    expect(storyAction("Escape")).toBe("close");
    expect(storyAction("a")).toBeNull();
    expect(stepPage(0, 3, "prev")).toBe(0);
    expect(stepPage(1, 3, "next")).toBe(2);
    expect(stepPage(2, 3, "next")).toBeNull();
  });
});
