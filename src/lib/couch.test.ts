import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { couchKey, couchKeyAction, couchPadAction, couchRows, initialFocus, localDate, stepRow, todaySecs } from "./couch";
import { jeu, jeuAmbiance, JOUR, maintenant } from "../test/jeu";
import { setLang } from "./i18n";

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-19T20:00:00Z"));
});
afterEach(() => {
  vi.useRealTimers();
  setLang("fr");
});

const titres = (games: { title: string }[]) => games.map((g) => g.title);
const aucun: ReadonlySet<string> = new Set();
const ilYa = (jours: number) => maintenant() - jours * JOUR;
const rangee = (rows: ReturnType<typeof couchRows>, id: string) => rows.find((r) => r.id === id);

describe("couchRows : rangées du mode canapé", () => {
  it("ordre des rangées, vides absentes", () => {
    const rows = couchRows([jeu({ title: "B", lastPlayed: ilYa(2) }), jeu({ title: "A" })], aucun);
    expect(rows.map((r) => r.id)).toEqual(["recent", "all"]);
    expect(titres(rangee(rows, "all")!.games)).toEqual(["A", "B"]);
  });

  it("Reprendre : partie en cours d'abord, puis « En cours » du plus récent au plus ancien", () => {
    const vieux = jeu({ title: "En cours ancien", status: "playing", lastPlayed: ilYa(40) });
    const recent = jeu({ title: "En cours récent", status: "playing", lastPlayed: ilYa(1) });
    const live = jeu({ title: "En jeu", lastPlayed: ilYa(60) });
    const rows = couchRows([vieux, recent, live], new Set([live.id]));
    expect(titres(rangee(rows, "resume")!.games)).toEqual(["En jeu", "En cours récent", "En cours ancien"]);
  });

  it("Récents sans les jeux de Reprendre", () => {
    const enCours = jeu({ title: "En cours", status: "playing", lastPlayed: ilYa(1) });
    const autre = jeu({ title: "Autre", lastPlayed: ilYa(3) });
    const rows = couchRows([enCours, autre], aucun);
    expect(titres(rangee(rows, "recent")!.games)).toEqual(["Autre"]);
  });

  it("jeux masqués et outils écartés partout", () => {
    const rows = couchRows(
      [
        jeu({ title: "Masqué", hidden: true, favorite: true, lastPlayed: ilYa(1) }),
        jeu({ title: "Outil", isTool: true, hidden: true, lastPlayed: ilYa(1) }),
        jeuAmbiance(["chill"], { title: "Masqué chill", hidden: true }),
        jeu({ title: "Visible" }),
      ],
      aucun,
    );
    expect(rows.flatMap((r) => titres(r.games))).toEqual(["Visible"]);
  });

  it("Favoris du plus récent au plus ancien", () => {
    const rows = couchRows([jeu({ title: "F1", favorite: true, lastPlayed: ilYa(9) }), jeu({ title: "F2", favorite: true, lastPlayed: ilYa(1) })], aucun);
    expect(titres(rangee(rows, "favorites")!.games)).toEqual(["F2", "F1"]);
  });

  it("une rangée par ambiance, avec son libellé", () => {
    const rows = couchRows([jeuAmbiance(["chill"], { title: "Calme" }), jeuAmbiance(["friends"], { title: "Coop" })], aucun);
    expect(rows.filter((r) => r.id.startsWith("mood:")).map((r) => [r.id, r.title, titres(r.games)])).toEqual([
      ["mood:chill", "Chill", ["Calme"]],
      ["mood:friends", "Entre amis", ["Coop"]],
    ]);
  });

  it("rangées limitées, sauf « Tous les jeux »", () => {
    const games = Array.from({ length: 30 }, (_, i) => jeu({ favorite: true, lastPlayed: ilYa(i + 1) }));
    const rows = couchRows(games, aucun, 5);
    expect(rangee(rows, "favorites")!.games).toHaveLength(5);
    expect(rangee(rows, "recent")!.games).toHaveLength(5);
    expect(rangee(rows, "all")!.games).toHaveLength(30);
  });
});

describe("initialFocus : jaquette choisie à l'ouverture", () => {
  it("jeu de « Ce soir », dans la première rangée qui le contient", () => {
    const enCours = jeu({ title: "En cours", status: "playing", lastPlayed: ilYa(2) });
    const games = [jeu({ lastPlayed: ilYa(1) }), enCours];
    const rows = couchRows(games, aucun);
    expect(initialFocus(rows, games, aucun)).toBe(couchKey("resume", enCours.id));
  });
  it("sinon le dernier lancé, dans Récents", () => {
    const dernier = jeu({ lastPlayed: ilYa(1) });
    const games = [jeu(), dernier];
    expect(initialFocus(couchRows(games, aucun), games, aucun)).toBe(couchKey("recent", dernier.id));
  });
  it("aucune rangée : rien", () => {
    expect(initialFocus([], [], aucun)).toBeNull();
  });
});

describe("couchKeyAction : clavier", () => {
  it("touches reconnues", () => {
    const touches = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Enter", " ", "PageUp", "PageDown", "Home", "End", "Escape", "Backspace", "F11"];
    expect(touches.map((key) => couchKeyAction({ key }))).toEqual([
      "up", "down", "left", "right", "confirm", "fiche", "prevRow", "nextRow", "first", "last", "back", "back", "fullscreen",
    ]);
  });
  it("Ctrl+D favori, autres raccourcis ignorés", () => {
    expect(couchKeyAction({ key: "d", ctrlKey: true })).toBe("favorite");
    expect(couchKeyAction({ key: "D", ctrlKey: true })).toBe("favorite");
    expect(couchKeyAction({ key: "k", ctrlKey: true })).toBeNull();
    expect(couchKeyAction({ key: "Enter", altKey: true })).toBeNull();
    expect(couchKeyAction({ key: "a" })).toBeNull();
  });
});

describe("couchPadAction : manette", () => {
  it("A jouer, X fiche, Y favori, B retour, LB/RB rangée, Guide sortie", () => {
    expect(["confirm", "details", "favorite", "back", "prevSection", "nextSection", "couch", "left"].map((a) => couchPadAction(a as never))).toEqual([
      "confirm", "fiche", "favorite", "back", "prevRow", "nextRow", "exit", "left",
    ]);
  });
  it("gâchettes, Vue et Menu seuls : rien", () => {
    expect(["prevFilter", "nextFilter", "nextSort", "menu"].map((a) => couchPadAction(a as never))).toEqual([null, null, null, null]);
  });
});

describe("todaySecs : temps de jeu du jour", () => {
  const semaine = { start: "2026-11-16", days: [60, 0, 120, 3600, 0, 0, 0] };
  it("jour de la semaine en cours", () => {
    expect(todaySecs(semaine, new Date(2026, 10, 19, 23, 30))).toBe(3600);
    expect(todaySecs(semaine, new Date(2026, 10, 16, 0, 5))).toBe(60);
  });
  it("hors de la période : 0", () => {
    expect(todaySecs(semaine, new Date(2026, 10, 15))).toBe(0);
    expect(todaySecs(semaine, new Date(2026, 10, 30))).toBe(0);
  });
  it("passage à l'heure d'hiver sans décalage", () => {
    expect(todaySecs({ start: "2026-10-19", days: [0, 0, 0, 0, 0, 0, 900] }, new Date(2026, 9, 25, 12))).toBe(900);
  });
});

describe("divers", () => {
  it("localDate", () => {
    expect(localDate(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
  it("stepRow : sans boucler", () => {
    expect(stepRow(3, 0, -1)).toBeNull();
    expect(stepRow(3, 0, 1)).toBe(1);
    expect(stepRow(3, 2, 1)).toBeNull();
  });
});

describe("mode canapé en anglais", () => {
  it("titres des rangées et des ambiances en anglais", () => {
    setLang("en");
    const enCours = jeu({ title: "En cours", status: "playing", lastPlayed: ilYa(1) });
    const rows = couchRows(
      [enCours, jeu({ title: "Fav", favorite: true, lastPlayed: ilYa(2) }), jeuAmbiance(["friends"], { title: "Coop" })],
      aucun,
    );
    expect(rows.map((r) => [r.id, r.title])).toEqual([
      ["resume", "Jump back in"],
      ["favorites", "Favorites"],
      ["recent", "Recent"],
      ["mood:friends", "With friends"],
      ["all", "All games"],
    ]);
  });
});
