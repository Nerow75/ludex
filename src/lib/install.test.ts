import { describe, expect, it } from "vitest";
import type { InstallEvent } from "./api";
import { downloadDetail, installPatch, progressBar } from "./install";
import { espaces } from "../test/jeu";

const evenement = (fields: Partial<InstallEvent>): InstallEvent => ({
  id: "steam:730",
  installState: { kind: "updating", progress: 42 },
  sizeOnDisk: null,
  speed: null,
  remaining: null,
  ...fields,
});

describe("installPatch", () => {
  it("pose l'état et le débit pendant un téléchargement", () => {
    expect(installPatch(evenement({ speed: 12_400_000, remaining: 180 }))).toEqual({
      installState: { kind: "updating", progress: 42 },
      download: { speed: 12_400_000, remaining: 180 },
    });
  });

  it("garde la taille connue quand l'événement n'en donne pas", () => {
    expect(installPatch(evenement({}))).not.toHaveProperty("sizeOnDisk");
    expect(installPatch(evenement({ sizeOnDisk: 5e9 })).sizeOnDisk).toBe(5e9);
  });

  it("efface le débit en pause, en file et à la fin", () => {
    for (const installState of [{ kind: "paused", progress: 50 }, { kind: "queued" }, { kind: "ready" }] as const) {
      const patch = installPatch(evenement({ installState, speed: 10, remaining: 5 }));
      expect(patch).toHaveProperty("download", undefined);
      expect(patch.installState).toEqual(installState);
    }
    expect(installPatch(evenement({})).download).toBeUndefined();
  });
});

describe("progressBar", () => {
  it("remplit la barre pendant un téléchargement ou une pause dont le % est connu", () => {
    expect(progressBar({ kind: "updating", progress: 42 })).toBe(42);
    expect(progressBar({ kind: "paused", progress: 0 })).toBe(0);
    expect(progressBar({ kind: "updating", progress: 140 })).toBe(100);
  });

  it("aucune barre sans % ou hors téléchargement", () => {
    expect(progressBar({ kind: "updating", progress: null })).toBeNull();
    expect(progressBar({ kind: "queued" })).toBeNull();
    expect(progressBar({ kind: "update_required" })).toBeNull();
    expect(progressBar({ kind: "ready" })).toBeNull();
  });
});

describe("downloadDetail", () => {
  it("donne le débit et le temps restant", () => {
    expect(espaces(downloadDetail({ speed: 12_400_000, remaining: 180 })!)).toBe("12,4 Mo/s · 3 min restantes");
    expect(downloadDetail({ speed: 850_000, remaining: 4_800 })).toBe("850 Ko/s · 1 h 20 restantes");
    expect(downloadDetail({ speed: null, remaining: 30 })).toBe("moins d'une minute");
    expect(downloadDetail({ speed: 2_000_000, remaining: null })).toBe("2 Mo/s");
  });

  it("rien sans débit ni temps", () => {
    expect(downloadDetail(undefined)).toBeNull();
    expect(downloadDetail({ speed: null, remaining: null })).toBeNull();
    expect(downloadDetail({ speed: 0, remaining: null })).toBeNull();
  });
});
