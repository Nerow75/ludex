import { describe, expect, it } from "vitest";
import { sourceList, summarize } from "./welcome";
import type { Platform, SourceReport } from "./api";
import { jeu } from "../test/jeu";

const source = (platform: Platform, installed: boolean, count: number, error: string | null = null): SourceReport => ({
  platform,
  installed,
  count,
  error,
  disabled: false,
});

describe("sourceList", () => {
  it("une, deux, trois sources, puis « et N autres »", () => {
    expect(sourceList([])).toBe("");
    expect(sourceList(["Steam"])).toBe("Steam");
    expect(sourceList(["Steam", "EA"])).toBe("Steam et EA");
    expect(sourceList(["Steam", "EA", "Riot"])).toBe("Steam, EA et Riot");
    expect(sourceList(["Steam", "EA", "Riot", "GOG", "Xbox"])).toBe("Steam, EA et 3 autres");
  });
});

describe("summarize", () => {
  it("phrase du titre, sources de la plus fournie à la moins fournie, dossiers de jeux nommés", () => {
    const games = [
      ...[1, 2, 3].map(() => jeu({ platform: "steam" })),
      ...[1, 2].map(() => jeu({ platform: "other" })),
      jeu({ platform: "riot" }),
      jeu({ platform: "ea" }),
      jeu({ platform: "steam", isTool: true, hidden: true }),
    ];
    const s = summarize({ games, sources: [source("steam", true, 4), source("riot", true, 1), source("ea", true, 1), source("other", true, 2)] });
    expect(s.total).toBe(7);
    expect(s.found).toEqual([
      { platform: "steam", label: "Steam", count: 3 },
      { platform: "other", label: "Dossiers de jeux", count: 2 },
      { platform: "ea", label: "EA", count: 1 },
      { platform: "riot", label: "Riot", count: 1 },
    ]);
    expect(s.where).toBe("trouvés dans Steam, tes dossiers de jeux et 2 autres");
    expect(s.tools).toBe(1);
  });
  it("singulier pour un seul jeu", () => {
    const s = summarize({ games: [jeu({ platform: "gog" })], sources: [source("gog", true, 1)] });
    expect(s.where).toBe("trouvé dans GOG");
  });
  it("sans jeu : phrase vide", () => {
    const s = summarize({ games: [], sources: [source("steam", false, 0)] });
    expect(s).toMatchObject({ total: 0, where: "", found: [], missing: ["Steam"] });
  });
  it("launchers sans jeu, absents, illisibles ; « Autres » jamais listé comme launcher", () => {
    const s = summarize({
      games: [jeu({ platform: "steam", isTool: true, hidden: true })],
      sources: [
        source("steam", true, 1),
        source("ubisoft", true, 0),
        source("epic", true, 0),
        source("gog", false, 0),
        source("xbox", true, 0, "Accès refusé"),
        source("other", true, 0),
      ],
    });
    // Steam n'a qu'un outil masqué : il n'est pas « sans jeu ».
    expect(s.empty).toEqual(["Epic Games", "Ubisoft"]);
    expect(s.missing).toEqual(["GOG"]);
    expect(s.failed).toEqual(["Xbox"]);
    expect(s.total).toBe(0);
    expect(s.where).toBe("");
  });
  it("jeu masqué par l'utilisateur (pas un outil) : ni compté ni dans les applis masquées", () => {
    const s = summarize({ games: [jeu({ hidden: true }), jeu()], sources: [source("steam", true, 2)] });
    expect(s.total).toBe(1);
    expect(s.tools).toBe(0);
  });
});
