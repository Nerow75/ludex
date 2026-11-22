// Écran de lancement « cinéma » (`LaunchScreen.tsx`) : états suivis par l'événement `game-session`,
// délais de fermeture et textes. Fonctions pures, sans DOM ni horloge : l'heure est passée en argument.
import { PLATFORM_LABEL, type Game, type SessionEvent } from "./api";
import { t } from "./i18n";

/** Lancement demandé, jeu vu par Ludex, ou erreur de `launch_game`. */
export type LaunchPhase = "launching" | "running" | "error";

export interface LaunchView {
  id: string;
  phase: LaunchPhase;
  /** Début de la phase en cours (millisecondes) : les délais de fermeture partent de là. */
  since: number;
  /** Message de `launch_game` (phase `error`). */
  error: string | null;
}

/** « Bonne partie » reste affiché ce temps, puis l'écran s'efface. */
export const RUNNING_HOLD_MS = 2000;
/** Sans processus du jeu vu (Steam ou Galaxy qui ne remontent pas toujours, jeu lent) : fermeture douce. */
export const SILENT_MS = 20_000;
/** Après ce temps sans jeu vu, une ligne dit qui prend le relais (« Steam prend le relais… »). */
export const RELAY_HINT_MS = 6000;
/** Durée du fondu de sortie (même valeur que `.launch.is-leaving` dans `app.css`). */
export const FADE_MS = 240;
/** Fondu plus lent quand l'écran se ferme seul sans avoir vu le jeu. */
export const SOFT_FADE_MS = 700;

/** Écran ouvert au clic sur Jouer. Jeu déjà en cours : « Bonne partie » tout de suite (le launcher le ramène). */
export function startLaunch(game: Game, alreadyPlaying: boolean, now: number): LaunchView {
  return { id: game.id, phase: alreadyPlaying ? "running" : "launching", since: now, error: null };
}

/**
 * Événement `game-session` : `running` = jeu vu, `ended` = fermeture (partie finie ou lanceur
 * intermédiaire refermé sans jeu), `detecting` = rien de nouveau. Un autre jeu ne change rien.
 */
export function afterSession(view: LaunchView, e: SessionEvent, now: number): LaunchView | null {
  if (e.id !== view.id || view.phase === "error") return view;
  if (e.phase === "ended") return null;
  if (e.phase === "running" && view.phase !== "running") return { ...view, phase: "running", since: now };
  return view;
}

/** Erreur de `launch_game` pour ce jeu : message et bouton Fermer, plus de fermeture automatique. */
export function afterError(view: LaunchView, id: string, message: string, now: number): LaunchView {
  if (view.id !== id) return view;
  return { ...view, phase: "error", since: now, error: message.trim() || t.extras.launch.noAnswer };
}

/** Fermeture automatique : dans combien de temps et avec quel fondu ; `null` = jamais (erreur). */
export function autoClose(view: LaunchView, now: number): { ms: number; soft: boolean } | null {
  const elapsed = Math.max(0, now - view.since);
  if (view.phase === "running") return { ms: Math.max(0, RUNNING_HOLD_MS - elapsed), soft: false };
  if (view.phase === "launching") return { ms: Math.max(0, SILENT_MS - elapsed), soft: true };
  return null;
}

/** Programmes qui lancent le jeu à la place de Ludex, d'après l'exécutable lancé. */
const RELAY_EXES: Record<string, string> = {
  "galaxyclient.exe": "GOG Galaxy",
  "riotclientservices.exe": "Riot Client",
  "explorer.exe": "Xbox",
  "zaap.exe": "Ankama Launcher",
  "ankama launcher.exe": "Ankama Launcher",
  "battle.net.exe": "Battle.net",
  "eadesktop.exe": "EA app",
};
/** Schémas d'URI des launchers. */
const RELAY_SCHEMES: Record<string, string> = {
  steam: "Steam",
  "com.epicgames.launcher": "Epic Games",
  uplay: "Ubisoft Connect",
  battlenet: "Battle.net",
};

/**
 * Launcher qui prend le relais (`launchLabel` : URI ou « exe arguments »), `null` si Ludex lance
 * l'exécutable du jeu lui-même.
 */
export function relayName(game: Game): string | null {
  const label = game.launchLabel.trim();
  const scheme = /^([a-z][a-z0-9+.-]*):\/\//i.exec(label)?.[1].toLowerCase();
  if (scheme) return RELAY_SCHEMES[scheme] ?? null;
  const lower = label.toLowerCase();
  const exe = Object.keys(RELAY_EXES).find((name) => lower === name || lower.startsWith(`${name} `));
  return exe ? RELAY_EXES[exe] : null;
}

/** Texte principal et ligne secondaire de l'écran. */
export function launchText(view: LaunchView, game: Game, now: number): { status: string; detail: string | null } {
  const text = t.extras.launch;
  if (view.phase === "error") return { status: text.failed, detail: view.error };
  if (view.phase === "running") return { status: text.running, detail: null };
  const relay = relayName(game);
  const late = now - view.since >= RELAY_HINT_MS;
  return { status: text.launching, detail: late ? (relay ? text.relay(relay) : text.starting) : null };
}

/** Fond de l'écran : image large du jeu, sinon sa jaquette (plus floutée), sinon la teinte du titre. */
export function launchBackdrop(game: Game): { url: string | null; kind: "hero" | "cover" | "tint" } {
  if (game.heroUrl) return { url: game.heroUrl, kind: "hero" };
  if (game.coverUrl) return { url: game.coverUrl, kind: "cover" };
  return { url: null, kind: "tint" };
}

/** Ligne du launcher sous le logo (« Steam », « Dossiers de jeux (Autres) »). */
export const launcherLine = (game: Game) => PLATFORM_LABEL[game.platform];
