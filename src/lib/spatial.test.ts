import { describe, expect, it } from "vitest";
import { spatialNext } from "./spatial";

/** Faux élément : seul `getBoundingClientRect` est lu. */
function boite(nom: string, left: number, top: number, width = 100, height = 150) {
  const rect = { left, top, width, height, right: left + width, bottom: top + height };
  return { nom, getBoundingClientRect: () => rect } as unknown as HTMLElement & { nom: string };
}

// Deux rangées de jaquettes et un bouton large au-dessus (bannière « Ce soir »).
const jouer = boite("jouer", 0, 0, 120, 40);
const a1 = boite("a1", 0, 100);
const a2 = boite("a2", 120, 100);
const a3 = boite("a3", 240, 100);
const b1 = boite("b1", 50, 300);
const b2 = boite("b2", 200, 300);
const cache = boite("caché", 120, 500, 0, 0);
const tous = [jouer, a1, a2, a3, b1, b2, cache];

const nom = (el: HTMLElement | null) => (el as { nom?: string } | null)?.nom ?? null;

describe("spatialNext", () => {
  it("gauche et droite restent dans la rangée", () => {
    expect(nom(spatialNext(tous, a2, "right"))).toBe("a3");
    expect(nom(spatialNext(tous, a2, "left"))).toBe("a1");
    expect(nom(spatialNext(tous, a3, "right"))).toBeNull();
    expect(nom(spatialNext(tous, a1, "left"))).toBeNull();
  });
  it("bas : rangée la plus proche, puis l'élément le plus proche en largeur", () => {
    expect(nom(spatialNext(tous, a1, "down"))).toBe("b1");
    expect(nom(spatialNext(tous, a3, "down"))).toBe("b2");
    expect(nom(spatialNext(tous, jouer, "down"))).toBe("a1");
  });
  it("haut : rangée du dessus", () => {
    expect(nom(spatialNext(tous, b2, "up"))).toBe("a3");
    expect(nom(spatialNext(tous, b1, "up"))).toBe("a1");
    expect(nom(spatialNext(tous, a2, "up"))).toBe("jouer");
    expect(nom(spatialNext(tous, jouer, "up"))).toBeNull();
  });
  it("élément caché (taille nulle) jamais choisi", () => {
    expect(nom(spatialNext(tous, b1, "down"))).toBeNull();
  });
  it("élément de départ absent de la liste : accepté", () => {
    const seul = boite("seul", 0, 100);
    expect(nom(spatialNext([a2], seul, "right"))).toBe("a2");
  });
});
