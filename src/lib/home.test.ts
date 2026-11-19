import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { downloads, lastAchievement, moodIdeas, neverPlayed, pickTonight, pileGames } from "./home";
import { jeu, jeuAmbiance, JOUR, maintenant } from "../test/jeu";

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-19T20:00:00Z"));
});
afterEach(() => vi.useRealTimers());

const titres = (games: { title: string }[]) => games.map((g) => g.title);
const aucun: ReadonlySet<string> = new Set();
const ilYa = (jours: number) => maintenant() - jours * JOUR;

describe("neverPlayed", () => {
  it("ni date de lancement ni temps de jeu", () => {
    expect(neverPlayed(jeu())).toBe(true);
    expect(neverPlayed(jeu({ playtimeMinutes: 0 }))).toBe(true);
    expect(neverPlayed(jeu({ playtimeMinutes: 12 }))).toBe(false);
    expect(neverPlayed(jeu({ lastPlayed: ilYa(3) }))).toBe(false);
  });
});

describe("pickTonight : jeu de « Ce soir »", () => {
  const dernier = jeu({ title: "Dernier lancé", lastPlayed: ilYa(1) });
  const enCours = jeu({ title: "En cours récent", status: "playing", lastPlayed: ilYa(10) });
  const enCoursVieux = jeu({ title: "En cours ancien", status: "playing", lastPlayed: ilYa(45) });
  const pile = [jeu({ title: "Pile 1" }), jeu({ title: "Pile 2" })];

  it("une partie en cours passe avant tout", () => {
    const live = jeu({ title: "Partie en cours", lastPlayed: ilYa(100) });
    expect(pickTonight([dernier, enCours, live], new Set([live.id]), pile)).toEqual({ game: live, reason: "live" });
  });
  it("puis un jeu « En cours » joué ces 30 derniers jours, même s'il n'est pas le dernier lancé", () => {
    expect(pickTonight([dernier, enCours, enCoursVieux], aucun, pile)).toEqual({ game: enCours, reason: "playing" });
  });
  it("un « En cours » de plus de 30 jours ne compte pas : dernier jeu lancé", () => {
    expect(pickTonight([enCoursVieux, dernier], aucun, pile)).toEqual({ game: dernier, reason: "last" });
  });
  it("rien de lancé : le premier de la pile, sinon rien", () => {
    expect(pickTonight([jeu()], aucun, pile)).toEqual({ game: pile[0], reason: "new" });
    expect(pickTonight([], aucun, [])).toBeNull();
  });
  it("jeux masqués et outils jamais proposés, même en cours de partie", () => {
    const masque = jeu({ title: "Masqué", hidden: true, lastPlayed: ilYa(0) });
    const outil = jeu({ title: "Outil", isTool: true, lastPlayed: ilYa(0), status: "playing" });
    expect(pickTonight([masque, outil, dernier], new Set([masque.id, outil.id]), pile)).toEqual({ game: dernier, reason: "last" });
  });
  it("plusieurs parties en cours : la plus récente", () => {
    const a = jeu({ title: "A", lastPlayed: ilYa(5) });
    const b = jeu({ title: "B", lastPlayed: ilYa(2) });
    expect(pickTonight([a, b], new Set([a.id, b.id]), pile)?.game).toBe(b);
  });
});

describe("pileGames : « Ta pile »", () => {
  it("« À faire » et jeux sans statut jamais lancés, les derniers installés d'abord puis par titre", () => {
    const afaire = jeu({ title: "Zelda-like", status: "todo", lastPlayed: ilYa(3) });
    const neuf = jeu({ title: "Neuf" });
    const ancien = jeu({ title: "Ancien" });
    const autre = jeu({ title: "Autre" });
    const joue = jeu({ title: "Joué", lastPlayed: ilYa(1) });
    const termine = jeu({ title: "Terminé", status: "done" });
    const abandonne = jeu({ title: "Abandonné", status: "dropped" });
    const enCours = jeu({ title: "En cours", status: "playing" });
    const masque = jeu({ title: "Masqué", hidden: true });
    const installes = new Map([
      [neuf.id, 300],
      [ancien.id, 100],
    ]);
    expect(titres(pileGames([afaire, neuf, ancien, autre, joue, termine, abandonne, enCours, masque], installes))).toEqual([
      "Neuf",
      "Ancien",
      "Autre",
      "Zelda-like",
    ]);
  });
});

describe("moodIdeas : rangées par ambiance", () => {
  it("pas lancés depuis longtemps (le plus ancien d'abord), puis jamais lancés, puis joués ce mois-ci", () => {
    const recent = jeuAmbiance(["chill"], { title: "Récent", lastPlayed: ilYa(3) });
    const jamais = jeuAmbiance(["chill"], { title: "Jamais" });
    const vieux = jeuAmbiance(["chill"], { title: "Vieux", lastPlayed: ilYa(200) });
    const moinsVieux = jeuAmbiance(["chill"], { title: "Moins vieux", lastPlayed: ilYa(60) });
    const [chill] = moodIdeas([recent, jamais, vieux, moinsVieux], 10, aucun);
    expect(chill.mood).toBe("chill");
    expect(titres(chill.games)).toEqual(["Vieux", "Moins vieux", "Jamais", "Récent"]);
  });
  it("abandonnés et jeu exclu écartés mais comptés ; masqués ni montrés ni comptés", () => {
    const ok = jeuAmbiance(["story"], { title: "OK" });
    const abandonne = jeuAmbiance(["story"], { title: "Abandonné", status: "dropped" });
    const banniere = jeuAmbiance(["story"], { title: "Bannière" });
    const masque = jeuAmbiance(["story"], { title: "Masqué", hidden: true });
    const outil = jeuAmbiance(["story"], { title: "Outil", isTool: true });
    expect(moodIdeas([ok, abandonne, banniere, masque, outil], 10, new Set([banniere.id]))).toEqual([
      { mood: "story", count: 3, games: [ok] },
    ]);
  });
  it("un jeu déjà proposé dans une ambiance précédente passe après les autres", () => {
    const deux = jeuAmbiance(["chill", "intense"], { title: "Deux ambiances", lastPlayed: ilYa(300) });
    const intense = jeuAmbiance(["intense"], { title: "Intense seul", lastPlayed: ilYa(5) });
    const [chill, inten] = moodIdeas([deux, intense], 10, aucun);
    expect(titres(chill.games)).toEqual(["Deux ambiances"]);
    expect(titres(inten.games)).toEqual(["Intense seul", "Deux ambiances"]);
  });
  it("au plus perMood jeux, ambiances vides retirées, ordre fixe des ambiances", () => {
    const jeux = [1, 2, 3, 4].map((i) => jeuAmbiance(["friends"], { title: `Ami ${i}` }));
    jeux.push(jeuAmbiance(["chill"], { title: "Calme" }));
    const idees = moodIdeas(jeux, 2, aucun);
    expect(idees.map((m) => m.mood)).toEqual(["chill", "friends"]);
    expect(idees[1]).toMatchObject({ count: 4, games: [jeux[0], jeux[1]] });
  });
});

describe("downloads", () => {
  it("téléchargements, pauses et files seulement, par titre", () => {
    const jeux = [
      jeu({ title: "B", installState: { kind: "updating", progress: 40 } }),
      jeu({ title: "A", installState: { kind: "paused", progress: null } }),
      jeu({ title: "C", installState: { kind: "queued" } }),
      jeu({ title: "D", installState: { kind: "update_required" } }),
      jeu({ title: "E", installState: { kind: "incomplete" } }),
      jeu({ title: "F", installState: { kind: "updating", progress: 1 }, hidden: true }),
    ];
    expect(titres(downloads(jeux))).toEqual(["A", "B", "C"]);
  });
});

describe("lastAchievement", () => {
  it("le plus récent des jeux non masqués", () => {
    const a = jeu({ achievements: { unlocked: 1, total: 10, lastUnlocked: 100 } });
    const b = jeu({ achievements: { unlocked: 2, total: 10, lastUnlocked: 300 } });
    const masque = jeu({ hidden: true, achievements: { unlocked: 2, total: 10, lastUnlocked: 999 } });
    const sans = jeu({ achievements: { unlocked: 0, total: 10, lastUnlocked: null } });
    expect(lastAchievement([a, b, masque, sans, jeu()])).toEqual({ game: b, at: 300 });
    expect(lastAchievement([sans, jeu()])).toBeNull();
  });
});
