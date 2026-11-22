import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { GogAccount, GogLibrary, GogProgress } from "./api";
import { accountText, gogDiagState, gogPercent, gogProgressText, gogRemaining, jobActive, librarySummary } from "./gog";
import { setLang } from "./i18n";
import { espaces, maintenant } from "../test/jeu";

beforeAll(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-22T12:00:00+01:00"));
});
afterAll(() => vi.useRealTimers());
afterEach(() => setLang("fr"));

const GO = 1_000_000_000;
const avancement = (champs: Partial<GogProgress> = {}): GogProgress => ({
  id: 1207658924,
  title: "The Witcher: Enhanced Edition",
  phase: "downloading",
  received: 4.3 * GO,
  total: 10.2 * GO,
  speed: 12_000_000,
  error: null,
  gameId: null,
  ...champs,
});
const compte = (champs: Partial<GogAccount> = {}): GogAccount => ({ state: "connected", username: "JoueurEssai", loginOpen: false, error: null, ...champs });
const liste = (champs: Partial<GogLibrary> = {}): GogLibrary => ({
  account: compte(),
  games: [
    { id: 1, title: "Fallout Classic", imageUrl: null },
    { id: 2, title: "The Witcher", imageUrl: null },
  ],
  owned: 12,
  checked: maintenant() - 2 * 3600,
  error: null,
  job: null,
  ...champs,
});

describe("jobActive", () => {
  it("vrai tant que l'installation tourne, pause comprise", () => {
    for (const phase of ["preparing", "downloading", "paused", "verifying", "installing"] as const) expect(jobActive(avancement({ phase }))).toBe(true);
    for (const phase of ["done", "failed", "cancelled"] as const) expect(jobActive(avancement({ phase }))).toBe(false);
    expect(jobActive(null)).toBe(false);
  });
});

describe("gogPercent et gogRemaining", () => {
  it("pourcentage arrondi vers le bas, inconnu sans total", () => {
    expect(gogPercent(avancement())).toBe(42);
    expect(gogPercent(avancement({ received: 10.2 * GO }))).toBe(100);
    expect(gogPercent(avancement({ total: 0 }))).toBeNull();
  });
  it("temps restant d'après le débit", () => {
    expect(gogRemaining(avancement())).toBe(492);
    expect(gogRemaining(avancement({ speed: null }))).toBeNull();
    expect(gogRemaining(avancement({ speed: 0 }))).toBeNull();
  });
});

describe("gogProgressText", () => {
  it("téléchargement : pourcentage, tailles, débit et temps restant", () => {
    expect(espaces(gogProgressText(avancement({ received: 10.1 * GO, speed: 1_000_000 })))).toBe("Téléchargement 99 % · 10,1 Go sur 10,2 Go · 1 Mo/s · 2 min restantes");
    expect(espaces(gogProgressText(avancement({ speed: null, total: 0, received: 0 })))).toBe("Téléchargement");
  });
  it("chaque étape a son texte", () => {
    expect(espaces(gogProgressText(avancement({ phase: "paused" })))).toBe("En pause, 42 %");
    expect(gogProgressText(avancement({ phase: "verifying" }))).toBe("Vérification de la signature de GOG…");
    expect(gogProgressText(avancement({ phase: "installing" }))).toBe("Installation par l'installeur de GOG…");
    expect(gogProgressText(avancement({ phase: "done", gameId: "gog:1207658924" }))).toBe("Installé : le jeu est dans ta bibliothèque");
    expect(gogProgressText(avancement({ phase: "done" }))).toContain("F5");
    expect(gogProgressText(avancement({ phase: "failed", error: "Signature refusée." }))).toBe("Échec : Signature refusée.");
    expect(gogProgressText(avancement({ phase: "failed" }))).toBe("Échec : erreur inconnue");
    expect(gogProgressText(avancement({ phase: "cancelled" }))).toBe("Téléchargement annulé");
  });
  it("en anglais", () => {
    setLang("en");
    expect(gogProgressText(avancement({ received: 10.1 * GO, speed: 1_000_000 }))).toBe("Downloading 99% · 10.1 GB of 10.2 GB · 1 MB/s · 2 min left");
    expect(gogProgressText(avancement({ phase: "paused" }))).toBe("Paused, 42%");
  });
});

describe("accountText", () => {
  it("état du compte, pseudo seulement s'il est connu", () => {
    expect(accountText(compte())).toBe("Connecté en tant que JoueurEssai");
    expect(accountText(compte({ username: null }))).toBe("Connecté");
    expect(accountText(compte({ state: "off", username: null }))).toBe("Non connecté");
    expect(accountText(compte({ state: "off", loginOpen: true }))).toBe("Connexion en cours dans la fenêtre de GOG…");
    expect(accountText(compte({ state: "expired" }))).toContain("À reconnecter");
    setLang("en");
    expect(accountText(compte())).toBe("Signed in as JoueurEssai");
  });
});

describe("librarySummary", () => {
  it("jeux à installer, possédés et date de la liste", () => {
    expect(librarySummary(liste())).toBe("2 jeux à installer sur 12 possédés. Liste relue il y a 2 h.");
    expect(librarySummary(liste({ games: [], owned: 1, checked: null }))).toBe("1 jeu possédé, rien à installer.");
    setLang("en");
    expect(librarySummary(liste({ checked: null }))).toBe("2 games to install out of 12 owned.");
  });
});

describe("gogDiagState", () => {
  it("jamais d'identifiant, seulement l'état", () => {
    expect(gogDiagState("connected")).toEqual({ text: "Connecté", tone: "ok" });
    expect(gogDiagState("off")).toEqual({ text: "Non connecté", tone: "off" });
    expect(gogDiagState("expired").tone).toBe("warn");
  });
});
