// Textes du statut Discord : aide des Paramètres et ligne du Diagnostic (langue actuelle).
import type { DiscordStatus } from "./api";
import type { Tone } from "./diagnostic";
import { t } from "./i18n";

/** Ce que montre le statut, sous l'interrupteur. */
export function discordHelp(available: boolean, hideName: boolean): string {
  const d = t.settings.discord;
  if (!available) return d.unavailable;
  return hideName ? d.helpHidden : d.help;
}

/** État du statut Discord dans le Diagnostic. */
export function discordState(s: DiscordStatus): { text: string; tone: Tone } {
  const d = t.settings.discord;
  switch (s.state) {
    case "unconfigured":
      return { text: d.unconfigured, tone: "off" };
    case "off":
      return { text: d.off, tone: "off" };
    case "waiting":
      return { text: d.waiting, tone: "ok" };
    case "connected":
      return { text: d.connected, tone: "ok" };
    case "absent":
      return { text: d.absent, tone: "warn" };
    case "refused":
      return { text: d.refused(s.error ?? d.unknownReason), tone: "error" };
  }
}
