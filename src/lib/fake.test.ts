import { describe, expect, it } from "vitest";
import { jeu } from "../test/jeu";
import { FAKE_GAMES, withFakeGames } from "./fake";

describe("withFakeGames", () => {
  it("complète la bibliothèque avec des copies numérotées aux ids uniques", () => {
    const vrais = [jeu({ id: "steam:1", title: "Hades" }), jeu({ id: "epic:a", title: "Celeste" })];
    const tous = withFakeGames(vrais, 7);
    expect(tous).toHaveLength(7);
    expect(tous.slice(0, 2)).toEqual(vrais);
    expect(tous.map((g) => g.title)).toEqual(["Hades", "Celeste", "Hades 2", "Celeste 2", "Hades 3", "Celeste 3", "Hades 4"]);
    expect(tous[2].id).toBe("steam:1~factice-2");
    expect(new Set(tous.map((g) => g.id)).size).toBe(7);
    // Mêmes données que l'original (jaquette comprise).
    expect(tous[2].platform).toBe("steam");
  });

  it("ne change rien sans jeu ou si la bibliothèque est déjà assez grande", () => {
    const vrais = [jeu(), jeu()];
    expect(withFakeGames(vrais, 2)).toBe(vrais);
    expect(withFakeGames(vrais, 0)).toBe(vrais);
    expect(withFakeGames([], 100)).toEqual([]);
  });

  it("est coupée sans la variable de construction", () => {
    expect(FAKE_GAMES).toBe(0);
  });
});
