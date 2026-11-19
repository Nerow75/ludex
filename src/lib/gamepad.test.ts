import { describe, expect, it } from "vitest";
import { PadReader, type PadSnapshot } from "./gamepad";

/** Relevé : boutons enfoncés (numéros du « standard mapping ») et stick gauche. */
const releve = (boutons: number[] = [], x = 0, y = 0): PadSnapshot => {
  const pressed = Array.from({ length: 17 }, (_, i) => boutons.includes(i));
  return { pressed, axes: [x, y, 0, 0] };
};

describe("PadReader", () => {
  it("un bouton agit à l'appui, pas tant qu'il reste enfoncé", () => {
    const pad = new PadReader();
    expect(pad.update(releve([0]), 0)).toEqual(["confirm"]);
    expect(pad.update(releve([0]), 16)).toEqual([]);
    expect(pad.update(releve(), 32)).toEqual([]);
    expect(pad.update(releve([0]), 48)).toEqual(["confirm"]);
  });
  it("boutons de la manette Xbox", () => {
    const pad = new PadReader();
    expect(pad.update(releve([1, 2, 3, 4, 5, 6, 7]), 0)).toEqual([
      "back",
      "details",
      "favorite",
      "prevSection",
      "nextSection",
      "prevFilter",
      "nextFilter",
    ]);
  });
  it("Vue et Menu agissent au relâchement", () => {
    const pad = new PadReader();
    expect(pad.update(releve([8]), 0)).toEqual([]);
    expect(pad.update(releve(), 16)).toEqual(["nextSort"]);
    expect(pad.update(releve([9]), 32)).toEqual([]);
    expect(pad.update(releve(), 48)).toEqual(["menu"]);
  });
  it("Vue + Menu ensemble : mode canapé, sans leurs actions", () => {
    const pad = new PadReader();
    expect(pad.update(releve([8]), 0)).toEqual([]);
    expect(pad.update(releve([8, 9]), 16)).toEqual(["couch"]);
    expect(pad.update(releve([8, 9]), 32)).toEqual([]);
    expect(pad.update(releve([8]), 48)).toEqual([]);
    expect(pad.update(releve(), 64)).toEqual([]);
    // Puis chacun retrouve son action.
    pad.update(releve([9]), 80);
    expect(pad.update(releve(), 96)).toEqual(["menu"]);
  });
  it("enfoncés dans le même relevé : mode canapé une seule fois", () => {
    const pad = new PadReader();
    expect(pad.update(releve([8, 9]), 0)).toEqual(["couch"]);
    expect(pad.update(releve(), 16)).toEqual([]);
  });
  it("bouton Guide : mode canapé", () => {
    expect(new PadReader().update(releve([16]), 0)).toEqual(["couch"]);
  });
  it("clics des sticks ignorés", () => {
    expect(new PadReader().update(releve([10, 11]), 0)).toEqual([]);
  });
  it("direction maintenue : un pas, puis répétition après le délai, puis au rythme", () => {
    const pad = new PadReader();
    expect(pad.update(releve([13]), 0)).toEqual(["down"]);
    expect(pad.update(releve([13]), 200)).toEqual([]);
    expect(pad.update(releve([13]), 380)).toEqual(["down"]);
    expect(pad.update(releve([13]), 450)).toEqual([]);
    expect(pad.update(releve([13]), 490)).toEqual(["down"]);
  });
  it("changer de direction ou relâcher remet le délai à zéro", () => {
    const pad = new PadReader();
    pad.update(releve([15]), 0);
    expect(pad.update(releve([14]), 10)).toEqual(["left"]);
    expect(pad.update(releve(), 20)).toEqual([]);
    expect(pad.update(releve([14]), 30)).toEqual(["left"]);
  });
  it("stick gauche : zone morte, axe dominant", () => {
    const pad = new PadReader();
    expect(pad.update(releve([], 0.4, 0.3), 0)).toEqual([]);
    expect(pad.update(releve([], -0.9, 0.6), 10)).toEqual(["left"]);
    expect(new PadReader().update(releve([], 0.2, -0.8), 0)).toEqual(["up"]);
  });
  it("la croix passe avant le stick", () => {
    expect(new PadReader().update(releve([12], 0.9, 0), 0)).toEqual(["up"]);
  });
});
