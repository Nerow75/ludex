import { describe, expect, it } from "vitest";
import { discordHelp, discordState } from "./discord";
import type { DiscordState } from "./api";

describe("discordHelp", () => {
  it("dit ce qui est montré, avec ou sans le nom du jeu", () => {
    expect(discordHelp(true, false)).toContain("le jeu, son launcher");
    expect(discordHelp(true, true)).toContain("sans le nom du jeu");
    expect(discordHelp(false, false)).toBe("Pas encore disponible dans cette version de Ludex.");
    for (const text of [discordHelp(true, false), discordHelp(true, true), discordHelp(false, true)]) expect(text).not.toContain("—");
  });
});

describe("discordState", () => {
  it("donne un texte et un ton pour chaque état", () => {
    const states: DiscordState[] = ["unconfigured", "off", "waiting", "connected", "absent", "refused"];
    for (const state of states) {
      const r = discordState({ state, error: null });
      expect(r.text.length).toBeGreaterThan(0);
      expect(r.text).not.toContain("—");
    }
    expect(discordState({ state: "connected", error: null }).tone).toBe("ok");
    expect(discordState({ state: "absent", error: "x" }).tone).toBe("warn");
    expect(discordState({ state: "refused", error: "Invalid Client ID" })).toEqual({ text: "Refusé par Discord : Invalid Client ID", tone: "error" });
  });
});
