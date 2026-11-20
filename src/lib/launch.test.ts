import { describe, expect, it } from "vitest";
import type { SessionEvent } from "./api";
import {
  afterError,
  afterSession,
  autoClose,
  launchBackdrop,
  launchText,
  relayName,
  RELAY_HINT_MS,
  RUNNING_HOLD_MS,
  SILENT_MS,
  startLaunch,
} from "./launch";
import { jeu } from "../test/jeu";

const session = (id: string, phase: SessionEvent["phase"]): SessionEvent => ({ id, phase, playtimeMinutes: null, playtimeSource: null });

describe("écran de lancement", () => {
  it("part de « Lancement… », ou de « Bonne partie » si le jeu tourne déjà", () => {
    const g = jeu();
    expect(startLaunch(g, false, 1000)).toEqual({ id: g.id, phase: "launching", since: 1000, error: null });
    expect(startLaunch(g, true, 1000).phase).toBe("running");
  });

  it("suit les événements game-session du jeu lancé seulement", () => {
    const g = jeu();
    const v = startLaunch(g, false, 0);
    expect(afterSession(v, session(g.id, "detecting"), 500)).toBe(v);
    expect(afterSession(v, session("autre:1", "running"), 500)).toBe(v);
    const running = afterSession(v, session(g.id, "running"), 3000);
    expect(running).toEqual({ ...v, phase: "running", since: 3000 });
    // Un second « running » ne repousse pas la fermeture.
    expect(afterSession(running!, session(g.id, "running"), 4000)).toBe(running);
    expect(afterSession(running!, session(g.id, "ended"), 4000)).toBeNull();
    expect(afterSession(v, session(g.id, "ended"), 100)).toBeNull();
  });

  it("garde l'erreur affichée, quoi qu'il arrive ensuite", () => {
    const g = jeu();
    const v = afterError(startLaunch(g, false, 0), g.id, "Le launcher n'a pas répondu : accès refusé", 200);
    expect(v.phase).toBe("error");
    expect(v.error).toBe("Le launcher n'a pas répondu : accès refusé");
    expect(afterSession(v, session(g.id, "running"), 300)).toBe(v);
    expect(autoClose(v, 99_999)).toBeNull();
    expect(afterError(startLaunch(g, false, 0), g.id, "  ", 1).error).toBe("Le launcher n'a pas répondu.");
    // Erreur d'un autre jeu (lancement précédent) : ignorée.
    const other = startLaunch(g, false, 0);
    expect(afterError(other, "autre:1", "x", 1)).toBe(other);
  });

  it("se ferme 2 s après « Bonne partie », ou en douceur après 20 s sans jeu vu", () => {
    const g = jeu();
    const v = startLaunch(g, false, 1000);
    expect(autoClose(v, 1000)).toEqual({ ms: SILENT_MS, soft: true });
    expect(autoClose(v, 1000 + SILENT_MS + 50)).toEqual({ ms: 0, soft: true });
    const running = afterSession(v, session(g.id, "running"), 5000)!;
    expect(autoClose(running, 5500)).toEqual({ ms: RUNNING_HOLD_MS - 500, soft: false });
  });

  it("dit qui prend le relais après quelques secondes", () => {
    const steam = jeu({ launchLabel: "steam://rungameid/730" });
    const v = startLaunch(steam, false, 0);
    expect(launchText(v, steam, 1000)).toEqual({ status: "Lancement…", detail: null });
    expect(launchText(v, steam, RELAY_HINT_MS)).toEqual({ status: "Lancement…", detail: "Steam prend le relais…" });
    const exe = jeu({ platform: "other", launchLabel: "Cyberpunk2077.exe" });
    expect(launchText(startLaunch(exe, false, 0), exe, RELAY_HINT_MS).detail).toBe("Le jeu démarre…");
    expect(launchText({ ...v, phase: "running" }, steam, 0)).toEqual({ status: "Bonne partie", detail: null });
    expect(launchText(afterError(v, steam.id, "Refusé", 0), steam, 0)).toEqual({ status: "Impossible de lancer le jeu", detail: "Refusé" });
  });

  it("reconnaît les launchers d'après la commande de lancement", () => {
    const name = (launchLabel: string) => relayName(jeu({ launchLabel }));
    expect(name("steam://run/730//-novid/")).toBe("Steam");
    expect(name("com.epicgames.launcher://apps/Sugar?action=launch&silent=true")).toBe("Epic Games");
    expect(name("uplay://launch/635/0")).toBe("Ubisoft Connect");
    expect(name("battlenet://S2")).toBe("Battle.net");
    expect(name("GalaxyClient.exe /command=runGame /gameId=1 /path=C:\\Jeux")).toBe("GOG Galaxy");
    expect(name("RiotClientServices.exe --launch-product=league_of_legends")).toBe("Riot Client");
    expect(name("explorer.exe shell:AppsFolder\\Microsoft.Minecraft!App")).toBe("Xbox");
    expect(name("roblox://jeu")).toBeNull();
    expect(name("PlayGTAV.exe")).toBeNull();
    expect(name("explorer_mod.exe")).toBeNull();
  });

  it("prend l'image large, sinon la jaquette, sinon la teinte du titre", () => {
    expect(launchBackdrop(jeu({ heroUrl: "h.jpg", coverUrl: "c.jpg" }))).toEqual({ url: "h.jpg", kind: "hero" });
    expect(launchBackdrop(jeu({ coverUrl: "c.jpg" }))).toEqual({ url: "c.jpg", kind: "cover" });
    expect(launchBackdrop(jeu())).toEqual({ url: null, kind: "tint" });
  });
});
