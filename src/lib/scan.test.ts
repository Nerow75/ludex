import { describe, expect, it } from "vitest";
import { awaitingGames, libraryToast, nextScan, welcomeReady } from "./scan";
import type { ScanResult } from "./api";
import { jeu } from "../test/jeu";

const scan = (partial: boolean, games = [jeu()]): ScanResult => ({ games, sources: [], partial });
const changement = (added: string[], removed: string[], complement = false) => ({ added, removed, scan: scan(false), complement });

describe("nextScan", () => {
  it("prend le nouveau scan, sauf un partiel arrivé après le scan complet", () => {
    const partiel = scan(true);
    const complet = scan(false);
    expect(nextScan(null, partiel)).toBe(partiel);
    expect(nextScan(partiel, complet)).toBe(complet);
    expect(nextScan(complet, partiel)).toBe(complet);
    const autre = scan(false);
    expect(nextScan(complet, autre)).toBe(autre);
  });
});

describe("awaitingGames", () => {
  it("attend tant que rien n'est reçu ou que le premier affichage n'a aucun jeu", () => {
    expect(awaitingGames(null)).toBe(true);
    expect(awaitingGames(scan(true, []))).toBe(true);
    expect(awaitingGames(scan(true, [jeu({ isTool: true })]))).toBe(true);
    expect(awaitingGames(scan(true))).toBe(false);
    // Scan complet sans jeu : « Aucun jeu trouvé », plus de squelette.
    expect(awaitingGames(scan(false, []))).toBe(false);
  });
});

describe("welcomeReady", () => {
  it("écran dû montré après le scan complet seulement", () => {
    expect(welcomeReady(null, true)).toBe(false);
    expect(welcomeReady(scan(true), true)).toBe(false);
    expect(welcomeReady(scan(false), true)).toBe(true);
    expect(welcomeReady(scan(false, []), true)).toBe(true);
    expect(welcomeReady(scan(false), false)).toBe(false);
  });
});

describe("libraryToast", () => {
  it("nouveaux jeux et jeux retirés", () => {
    expect(libraryToast(changement(["Hades"], []))).toEqual({ title: "Nouveau jeu : Hades", detail: undefined });
    expect(libraryToast(changement(["Hades", "Celeste", "Portal"], []))).toEqual({ title: "3 nouveaux jeux", detail: "Hades, Celeste, Portal" });
    expect(libraryToast(changement(["Hades"], ["Portal"]))).toEqual({ title: "Nouveau jeu : Hades", detail: "Jeu retiré : Portal" });
    expect(libraryToast(changement([], ["Portal", "Celeste"]))).toEqual({ title: "2 jeux retirés", detail: "Portal, Celeste" });
    expect(libraryToast(changement([], []))).toBeNull();
  });

  it("aucun toast pour le complément du premier scan", () => {
    expect(libraryToast(changement(["Cyberpunk 2077", "Dofus 3", "FC 24"], [], true))).toBeNull();
    expect(libraryToast(changement([], ["GTA V"], true))).toBeNull();
  });
});
