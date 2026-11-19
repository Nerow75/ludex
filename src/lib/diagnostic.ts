// Textes de la page de diagnostic (Paramètres), tirés du diagnostic envoyé par Rust.
import type { DiagService, DiagSource, Diagnostic } from "./api";
import { relativeTime } from "./format";

const integer = new Intl.NumberFormat("fr-FR");
const plural = (n: number, one: string, many: string) => `${integer.format(n)} ${n > 1 ? many : one}`;

/** État d'une source en une ligne, comme dans le rapport. */
export function sourceState(s: DiagSource): string {
  switch (s.state) {
    case "unknown":
      return "Pas encore lu";
    case "absent":
      return "Pas sur ce PC";
    case "empty":
      return s.id === "manual" ? "Aucun" : "Installé, aucun jeu";
    case "games":
      return plural(s.count, "jeu", "jeux");
    case "disabled":
      return s.count ? `Désactivé, ${plural(s.count, "jeu écarté", "jeux écartés")}` : "Désactivé";
    case "error":
      return `Erreur : ${s.error ?? "inconnue"}`;
  }
}

/** Ton de la ligne : erreur, quelque chose à regarder (écarts, emplacement attendu absent), ou rien. */
export type Tone = "error" | "warn" | "ok" | "off";

export function sourceTone(s: DiagSource): Tone {
  if (s.state === "error") return "error";
  if (s.state === "absent" || s.state === "disabled" || s.state === "unknown") return "off";
  if (s.id === "manual") return s.skipped.length ? "warn" : s.state === "games" ? "ok" : "off";
  return s.skipped.length ? "warn" : "ok";
}

/** Détails courts sous le nom : écarts, applis masquées, durée de lecture. */
export function sourceDetail(s: DiagSource): string {
  const parts: string[] = [];
  if (s.skipped.length) parts.push(s.id === "manual" ? plural(s.skipped.length, "exécutable introuvable", "exécutables introuvables") : plural(s.skipped.length, "écarté", "écartés"));
  if (s.tools.length) parts.push(plural(s.tools.length, "appli masquée", "applis masquées"));
  const ms = durationText(s.millis);
  if (ms) parts.push(`lu en ${ms}`);
  return parts.join(" · ");
}

/** « 12 ms », « 1,6 s » ; vide si inconnue. */
export function durationText(ms: number | null): string {
  if (ms === null) return "";
  if (ms < 1000) return `${integer.format(ms)} ms`;
  return `${new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 }).format(ms / 1000)} s`;
}

/** État d'un service en ligne depuis le démarrage. Une erreur plus récente que la dernière réussite compte. */
export function serviceState(s: DiagService): { text: string; tone: Tone } {
  const failing = s.errorAt !== null && (s.lastOk === null || s.errorAt >= s.lastOk);
  if (failing) return { text: `Erreur ${relativeTime(s.errorAt)} : ${s.lastError ?? "inconnue"}`, tone: "error" };
  if (s.busy) return { text: "Lecture en cours", tone: "ok" };
  if (s.lastOk !== null) return { text: `Joint ${relativeTime(s.lastOk)}`, tone: "ok" };
  return { text: "Rien demandé depuis le démarrage", tone: "off" };
}

/** Nombre de problèmes à signaler en tête : sources en erreur, services en erreur. */
export function problems(d: Diagnostic): number {
  return d.sources.filter((s) => s.state === "error").length + d.services.filter((s) => serviceState(s).tone === "error").length;
}
