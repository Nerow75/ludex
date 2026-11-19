import { describe, expect, it } from "vitest";
import { EDGE, gameMenu, placeMenu, placeSubmenu, stepEntry, type MenuEntry } from "./contextMenu";
import type { Collection } from "./api";
import { jeu } from "../test/jeu";

const vue = { width: 1000, height: 800 };
const menu = { width: 200, height: 300 };

const cles = (entries: MenuEntry[]) => entries.map((e) => e.key);
const sousMenu = (entries: MenuEntry[], key: string) => {
  const e = entries.find((x) => x.key === key);
  if (e?.kind !== "submenu") throw new Error(`pas de sous-menu ${key}`);
  return e.children;
};

describe("gameMenu : entrées selon le jeu", () => {
  const collections: Collection[] = [
    { id: "c-1", name: "RPG", games: [] },
    { id: "c-2", name: "Coop", games: [] },
  ];

  it("jeu ordinaire avec dossier", () => {
    const g = jeu({ installDir: "D:\\Jeux\\Hades" });
    const entries = gameMenu(g, collections);
    expect(cles(entries)).toEqual(["play", "open", "sep-1", "favorite", "status", "collections", "sep-2", "folder", "rename", "hide"]);
    const label = (key: string) => (entries.find((e) => e.key === key) as { label: string }).label;
    expect(label("favorite")).toBe("Ajouter aux favoris");
    expect(label("hide")).toBe("Masquer");
  });
  it("favori, masqué, sans dossier", () => {
    const entries = gameMenu(jeu({ favorite: true, hidden: true, installDir: null }), []);
    expect(cles(entries)).not.toContain("folder");
    expect(entries.find((e) => e.key === "favorite")).toMatchObject({ label: "Retirer des favoris" });
    expect(entries.find((e) => e.key === "hide")).toMatchObject({ label: "Afficher", action: { kind: "hide" } });
  });
  it("statut : quatre choix puis Aucun, le statut du jeu coché", () => {
    const statuts = sousMenu(gameMenu(jeu({ status: "done" }), []), "status");
    expect(statuts.map((e) => (e as { label: string }).label)).toEqual(["À faire", "En cours", "Terminé", "Abandonné", "Aucun"]);
    expect(statuts.filter((e) => e.kind === "item" && e.checked).map((e) => e.key)).toEqual(["status-done"]);
    expect(statuts.every((e) => e.kind === "item" && e.role === "radio")).toBe(true);
    expect(statuts[4]).toMatchObject({ action: { kind: "status", status: null } });
  });
  it("sans statut : « Aucun » coché", () => {
    const statuts = sousMenu(gameMenu(jeu(), []), "status");
    expect(statuts.filter((e) => e.kind === "item" && e.checked).map((e) => e.key)).toEqual(["status-none"]);
  });
  it("collections cochées si le jeu y est ; l'action inverse l'appartenance", () => {
    const g = jeu();
    const cols = sousMenu(gameMenu(g, [{ id: "c-1", name: "RPG", games: [g.id] }, collections[1]]), "collections");
    expect(cles(cols)).toEqual(["col-c-1", "col-c-2", "col-sep", "col-new"]);
    expect(cols[0]).toMatchObject({ role: "checkbox", checked: true, action: { kind: "collection", id: "c-1", value: false } });
    expect(cols[1]).toMatchObject({ role: "checkbox", checked: false, action: { kind: "collection", id: "c-2", value: true } });
    expect(cols[3]).toMatchObject({ label: "Nouvelle collection…", action: { kind: "newCollection" } });
  });
  it("sans collection : seulement « Nouvelle collection… », sans séparateur", () => {
    expect(cles(sousMenu(gameMenu(jeu(), []), "collections"))).toEqual(["col-new"]);
  });
});

describe("placeMenu", () => {
  it("à droite et en dessous du point quand il y a la place", () => {
    expect(placeMenu({ x: 100, y: 100 }, menu, vue)).toEqual({ x: 100, y: 100 });
  });
  it("de l'autre côté s'il déborde à droite ou en bas", () => {
    expect(placeMenu({ x: 900, y: 700 }, menu, vue)).toEqual({ x: 700, y: 400 });
  });
  it("la marge du bord compte comme un débordement", () => {
    expect(placeMenu({ x: 1000 - EDGE - 200 + 1, y: 10 }, menu, vue).x).toBe(1000 - EDGE - 200 + 1 - 200);
    expect(placeMenu({ x: 1000 - EDGE - 200, y: 10 }, menu, vue).x).toBe(1000 - EDGE - 200);
  });
  it("jamais hors de la fenêtre, même retourné", () => {
    // Retourné vers la gauche il sortirait (x = -50) : collé à la marge.
    expect(placeMenu({ x: 150, y: 790 }, { width: 900, height: 300 }, vue)).toEqual({ x: EDGE, y: 490 });
  });
  it("plus haut que la fenêtre : collé en haut (la liste défile)", () => {
    expect(placeMenu({ x: 10, y: 400 }, { width: 200, height: 1200 }, vue).y).toBe(EDGE);
  });
});

describe("placeSubmenu", () => {
  const entree = { left: 300, top: 200, right: 500, bottom: 232 };

  it("à droite de l'entrée, aligné sur son haut", () => {
    expect(placeSubmenu(entree, menu, vue)).toEqual({ x: 500, y: 200 });
  });
  it("à gauche s'il déborde à droite", () => {
    expect(placeSubmenu({ left: 700, top: 200, right: 900, bottom: 232 }, menu, vue)).toEqual({ x: 500, y: 200 });
  });
  it("remonté s'il déborde en bas", () => {
    expect(placeSubmenu({ ...entree, top: 700 }, menu, vue).y).toBe(800 - EDGE - 300);
  });
});

describe("stepEntry", () => {
  const entries = gameMenu(jeu({ installDir: "C:\\x" }), []);
  const index = (key: string) => entries.findIndex((e) => e.key === key);

  it("sans entrée active : la première ou la dernière", () => {
    expect(stepEntry(entries, -1, 1)).toBe(0);
    expect(stepEntry(entries, -1, -1)).toBe(entries.length - 1);
  });
  it("saute les séparateurs", () => {
    expect(stepEntry(entries, index("open"), 1)).toBe(index("favorite"));
    expect(stepEntry(entries, index("folder"), -1)).toBe(index("collections"));
  });
  it("en boucle", () => {
    expect(stepEntry(entries, entries.length - 1, 1)).toBe(0);
    expect(stepEntry(entries, 0, -1)).toBe(entries.length - 1);
  });
  it("que des séparateurs ou rien : -1", () => {
    expect(stepEntry([], -1, 1)).toBe(-1);
    expect(stepEntry([{ kind: "separator", key: "s" }], -1, 1)).toBe(-1);
  });
});
