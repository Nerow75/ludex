import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanTitle,
  formatAchievements,
  formatBytes,
  formatClock,
  formatDate,
  formatDateTime,
  formatDay,
  formatDuration,
  formatPercent,
  formatPlaytime,
  formatSize,
  installBadge,
  installNote,
  norm,
  queryMatch,
  relativeDay,
  relativeTime,
  releaseYear,
} from "./format";
import { espaces, fiche, jeu, JOUR, maintenant } from "../test/jeu";

// Jeudi 19 novembre 2026, 15:00 à Paris (UTC+1).
const MAINTENANT = new Date("2026-11-19T14:00:00Z");

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(MAINTENANT);
});
afterEach(() => vi.useRealTimers());

describe("fuseau des tests", () => {
  it("est celui de Paris", () => {
    expect(new Date().getHours()).toBe(15);
  });
});

describe("norm et cleanTitle", () => {
  it("retire accents et casse", () => {
    expect(norm("Pokémon ÉPÉE")).toBe("pokemon epee");
  });
  it("retire ® et ™ et les espaces autour", () => {
    expect(cleanTitle("Call of Duty® ")).toBe("Call of Duty");
    expect(cleanTitle("Tom Clancy’s The Division™")).toBe("Tom Clancy’s The Division");
  });
});

describe("relativeTime", () => {
  const il_y_a = (secs: number) => relativeTime(maintenant() - secs);

  it("jamais lancé sans date", () => {
    expect(relativeTime(null)).toBe("Jamais lancé");
    expect(relativeTime(0)).toBe("Jamais lancé");
  });
  it("à l'instant sous une minute, et pour une date future", () => {
    expect(il_y_a(30)).toBe("à l'instant");
    expect(relativeTime(maintenant() + 3600)).toBe("à l'instant");
  });
  it("minutes puis heures", () => {
    expect(il_y_a(5 * 60)).toBe("il y a 5 min");
    expect(il_y_a(3 * 3600)).toBe("il y a 3 h");
  });
  it("hier entre un et deux jours", () => {
    expect(il_y_a(JOUR)).toBe("hier");
    expect(il_y_a(1.9 * JOUR)).toBe("hier");
  });
  it("jours, mois, puis plus d'un an", () => {
    expect(il_y_a(5 * JOUR)).toBe("il y a 5 jours");
    expect(il_y_a(29 * JOUR)).toBe("il y a 29 jours");
    expect(il_y_a(90 * JOUR)).toBe("il y a 3 mois");
    expect(il_y_a(400 * JOUR)).toBe("il y a plus d'un an");
  });
  it("l'arrondi ne passe pas à l'unité suivante", () => {
    expect(il_y_a(59 * 60 + 50)).toBe("il y a 59 min");
    expect(il_y_a(23 * 3600 + 50 * 60)).toBe("il y a 23 h");
    expect(il_y_a(350 * JOUR)).toBe("il y a 11 mois");
    expect(il_y_a(365 * JOUR)).toBe("il y a plus d'un an");
  });
});

describe("formatPlaytime", () => {
  it("inconnu et jamais joué", () => {
    expect(formatPlaytime(null)).toBe("Non mesuré");
    expect(formatPlaytime(0)).toBe("Jamais joué");
    expect(formatPlaytime(0.5)).toBe("Jamais joué");
  });
  it("minutes sous une heure", () => {
    expect(formatPlaytime(45)).toBe("45 min");
    expect(formatPlaytime(59.4)).toBe("59 min");
  });
  it("heures avec une décimale sous 10 h, arrondies au-delà", () => {
    expect(formatPlaytime(60)).toBe("1 h");
    expect(formatPlaytime(150)).toBe("2,5 h");
    expect(formatPlaytime(720)).toBe("12 h");
    expect(formatPlaytime(725)).toBe("12 h");
  });
  it("séparateur de milliers", () => {
    expect(espaces(formatPlaytime(1240 * 60))).toBe("1 240 h");
  });
});

describe("formatDuration", () => {
  it("une minute au moins", () => {
    expect(formatDuration(10)).toBe("1 min");
    expect(formatDuration(45 * 60)).toBe("45 min");
  });
  it("heures et minutes sur deux chiffres", () => {
    expect(formatDuration(80 * 60)).toBe("1 h 20");
    expect(formatDuration(65 * 60)).toBe("1 h 05");
    expect(formatDuration(3 * 3600)).toBe("3 h");
  });
});

describe("tailles", () => {
  it("formatSize : inconnue, Mo, Go, To", () => {
    expect(formatSize(null)).toBe("Inconnue");
    expect(formatSize(0)).toBe("Inconnue");
    expect(formatSize(200_000)).toBe("1 Mo");
    expect(formatSize(350e6)).toBe("350 Mo");
    expect(formatSize(1.54e9)).toBe("1,5 Go");
    expect(formatSize(2.3e12)).toBe("2,3 To");
  });
  it("formatBytes : petites tailles comprises", () => {
    expect(formatBytes(512)).toBe("512 o");
    expect(formatBytes(12_300)).toBe("12 Ko");
    expect(formatBytes(1_400_000)).toBe("1,4 Mo");
    expect(formatBytes(2.1e9)).toBe("2,1 Go");
  });
});

describe("succès et pourcentages", () => {
  it("formatAchievements", () => {
    expect(formatAchievements(34, 52)).toBe("34 / 52");
    expect(espaces(formatAchievements(1328, 1328))).toBe("1 328 / 1 328");
  });
  it("formatPercent", () => {
    expect(formatPercent(0.05)).toBe("moins de 0,1 %");
    expect(formatPercent(0.34)).toBe("0,3 %");
    expect(formatPercent(12.4)).toBe("12 %");
  });
});

describe("état d'installation", () => {
  it("installBadge", () => {
    expect(installBadge({ kind: "ready" })).toBeNull();
    expect(installBadge({ kind: "update_required" })).toBe("Mise à jour requise");
    expect(installBadge({ kind: "queued" })).toBe("En file d'attente");
    expect(installBadge({ kind: "updating", progress: 42 })).toBe("Téléchargement 42 %");
    expect(installBadge({ kind: "updating", progress: null })).toBe("Téléchargement");
    expect(installBadge({ kind: "paused", progress: 10 })).toBe("En pause 10 %");
    expect(installBadge({ kind: "incomplete" })).toBe("Installation incomplète");
  });
  it("installNote nomme le launcher", () => {
    expect(installNote({ kind: "ready" }, "Steam")).toBeNull();
    expect(installNote({ kind: "update_required" }, "Steam")).toBe("Mise à jour requise : Steam la fera au lancement");
    expect(installNote({ kind: "updating", progress: null }, "Steam")).toBe("Téléchargement en cours");
    expect(installNote({ kind: "paused", progress: 7 }, "Epic Games")).toBe("Mise à jour en pause 7 % : Epic Games la reprendra au lancement");
    expect(installNote({ kind: "incomplete" }, "Epic Games")).toBe("Installation incomplète : Epic Games la terminera au lancement");
  });
});

describe("dates", () => {
  // 10 décembre 2020, 14:23 à Paris.
  const ts = Date.UTC(2020, 11, 10, 13, 23) / 1000;

  it("formatDate, formatDateTime, formatClock à l'heure locale", () => {
    expect(formatDate(ts)).toBe("10 décembre 2020");
    expect(formatDateTime(ts)).toBe("10 décembre 2020 à 14:23");
    expect(formatClock(ts)).toBe("14:23");
  });
  it("formatClock passe à l'heure d'été", () => {
    expect(formatClock(Date.UTC(2026, 6, 1, 12, 5) / 1000)).toBe("14:05");
  });
  it("releaseYear", () => {
    expect(releaseYear(null)).toBeNull();
    expect(releaseYear(ts)).toBe(2020);
    // 31 décembre 2019 à 23:30 UTC = 1ᵉʳ janvier 2020 à Paris.
    expect(releaseYear(Date.UTC(2019, 11, 31, 23, 30) / 1000)).toBe(2020);
  });
  it("formatDay", () => {
    expect(formatDay("2026-11-18")).toBe("mercredi 18 novembre 2026");
  });
  it("relativeDay", () => {
    expect(relativeDay("2026-11-19")).toBe("Aujourd'hui");
    expect(relativeDay("2026-11-20")).toBe("Aujourd'hui");
    expect(relativeDay("2026-11-18")).toBe("Hier");
    expect(relativeDay("2026-11-14")).toBe("Il y a 5 jours");
    expect(relativeDay("2026-08-19")).toBe("Il y a 3 mois");
    expect(relativeDay("2025-11-01")).toBe("Il y a 1 an");
    expect(relativeDay("2023-11-19")).toBe("Il y a 3 ans");
  });
  it("relativeDay compte les jours de calendrier, pas les heures", () => {
    // Le changement d'heure du 25 octobre (journée de 25 h) ne décale pas le compte.
    expect(relativeDay("2026-10-26")).toBe("Il y a 24 jours");
    expect(relativeDay("2026-10-21")).toBe("Il y a 29 jours");
    expect(relativeDay("2026-10-20")).toBe("Il y a 1 mois");
  });
});

describe("queryMatch", () => {
  const callOfDuty = jeu({ title: "Call of Duty®: Black Ops III" });
  const football = jeu({ title: "Football Manager 2020" });
  const rocket = jeu({ title: "Rocket League" });
  const farCry = jeu({ title: "Far Cry 5", details: fiche({ publishers: ["Ubisoft"], tags: ["Monde ouvert"] }) });

  it("« cod » trouve Call of Duty par ses initiales", () => {
    expect(queryMatch(callOfDuty, "cod")).toBe(0);
  });
  it("« fm20 » trouve Football Manager 2020", () => {
    expect(queryMatch(football, "fm20")).toBe(0);
  });
  it("« rocet » trouve Rocket League malgré la faute", () => {
    expect(queryMatch(rocket, "rocet")).toBe(0);
  });
  it("« fifa » ne trouve pas Far Cry", () => {
    expect(queryMatch(farCry, "fifa")).toBeNull();
  });
  it("éditeur ou tag : 1", () => {
    expect(queryMatch(farCry, "ubisoft")).toBe(1);
    expect(queryMatch(farCry, "monde")).toBe(1);
  });
  it("rien : null", () => {
    expect(queryMatch(rocket, "zelda")).toBeNull();
  });
});
