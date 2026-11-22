// Compte GOG et jeux possédés non installés (`gog_online` côté Rust) : textes et calculs purs.
import type { GogAccount, GogLibrary, GogPhase, GogProgress } from "./api";
import { formatBytes, formatDuration, relativeTime } from "./format";
import { t } from "./i18n";

/** Installation qui tourne encore (le téléchargement peut être en pause). */
export function jobActive(p: GogProgress | null): boolean {
  if (!p) return false;
  const running: GogPhase[] = ["preparing", "downloading", "paused", "verifying", "installing"];
  return running.includes(p.phase);
}

/** Part téléchargée en % entier (vers le bas), `null` si le total est inconnu. */
export function gogPercent(p: GogProgress): number | null {
  if (p.total <= 0) return null;
  return Math.min(100, Math.floor((p.received / p.total) * 100));
}

/** Secondes restantes d'après le débit, `null` sans débit. */
export function gogRemaining(p: GogProgress): number | null {
  if (!p.speed || p.speed <= 0 || p.total <= 0) return null;
  return Math.max(0, Math.ceil((p.total - p.received) / p.speed));
}

/** Ligne d'état d'une installation : « Téléchargement 42 % · 4,3 Go sur 10,2 Go · 12 Mo/s · 8 min restantes ». */
export function gogProgressText(p: GogProgress): string {
  const g = t.library.gog;
  const percent = gogPercent(p);
  switch (p.phase) {
    case "preparing":
      return g.preparing;
    case "downloading": {
      const parts = [g.downloading(percent)];
      if (p.total > 0) parts.push(g.of(formatBytes(p.received), formatBytes(p.total)));
      if (p.speed && p.speed > 0) parts.push(`${formatBytes(p.speed)}/s`);
      const left = gogRemaining(p);
      if (left !== null) parts.push(left < 60 ? t.common.install.underAMinute : t.common.install.remaining(formatDuration(left)));
      return parts.join(" · ");
    }
    case "paused":
      return g.paused(percent);
    case "verifying":
      return g.verifying;
    case "installing":
      return g.installing;
    case "done":
      return p.gameId ? g.done : g.doneNotSeen;
    case "failed":
      return g.failed(p.error ?? g.unknownError);
    case "cancelled":
      return g.cancelled;
  }
}

/** État du compte dans les Paramètres. */
export function accountText(a: GogAccount): string {
  const c = t.settings.accounts;
  switch (a.state) {
    case "off":
      return a.loginOpen ? c.loginOpen : c.off;
    case "connected":
      return a.username ? c.connectedAs(a.username) : c.connected;
    case "expired":
      return c.expired;
  }
}

/** « 3 jeux à installer sur 12 possédés. Liste relue il y a 2 h. » */
export function librarySummary(lib: GogLibrary): string {
  const g = t.library.gog;
  const base = lib.games.length === 0 ? g.allInstalled(lib.owned) : g.summary(lib.games.length, lib.owned);
  return lib.checked ? `${base} ${g.checked(relativeTime(lib.checked))}` : base;
}

/** État du compte GOG dans le Diagnostic. */
export function gogDiagState(state: GogLibrary["account"]["state"]): { text: string; tone: "ok" | "off" | "warn" } {
  const c = t.settings.accounts;
  if (state === "connected") return { text: c.diagConnected, tone: "ok" };
  if (state === "expired") return { text: c.expired, tone: "warn" };
  return { text: c.diagOff, tone: "off" };
}
