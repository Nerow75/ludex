// Textes du statut Discord : aide des Paramètres et ligne du Diagnostic.
import type { DiscordStatus } from "./api";
import type { Tone } from "./diagnostic";

/** Ce que montre le statut, sous l'interrupteur. */
export function discordHelp(available: boolean, hideName: boolean): string {
  if (!available) return "Pas encore disponible dans cette version de Ludex.";
  if (hideName) return "Discord montre « En jeu » et le temps écoulé, sans le nom du jeu, pour les parties lancées depuis Ludex. Discord doit être ouvert sur ce PC.";
  return "Discord montre le jeu, son launcher, son image (jeux Steam) et le temps écoulé, pour les parties lancées depuis Ludex. Discord doit être ouvert sur ce PC.";
}

/** État du statut Discord dans le Diagnostic. */
export function discordState(s: DiscordStatus): { text: string; tone: Tone } {
  switch (s.state) {
    case "unconfigured":
      return { text: "Pas configuré dans cette version", tone: "off" };
    case "off":
      return { text: "Désactivé dans les Paramètres", tone: "off" };
    case "waiting":
      return { text: "Activé, aucune partie en cours", tone: "ok" };
    case "connected":
      return { text: "Connecté, partie affichée", tone: "ok" };
    case "absent":
      return { text: "Discord n'est pas ouvert (nouvel essai toutes les 30 s)", tone: "warn" };
    case "refused":
      return { text: `Refusé par Discord : ${s.error ?? "raison inconnue"}`, tone: "error" };
  }
}
