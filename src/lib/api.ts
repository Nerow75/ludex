import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Platform = "steam" | "epic" | "gog" | "ubisoft" | "rockstar" | "xbox" | "riot" | "battlenet" | "ea" | "other";
export type PlaytimeSource = "launcher" | "ludex";
/** `detecting` : lancé, le jeu n'est pas encore visible ; `running` : en jeu ; `ended` : terminé. */
export type SessionPhase = "detecting" | "running" | "ended";

export const PLATFORMS: readonly Platform[] = ["steam", "epic", "gog", "ubisoft", "ea", "battlenet", "rockstar", "xbox", "riot", "other"];

/** Fiche lue sur la boutique Steam pour les jeux dont on connaît l'appid. */
export interface GameDetails {
  /** Du plus voté au moins voté, trois au plus. */
  genres: string[];
  developers: string[];
  publishers: string[];
  /** Première sortie (secondes Unix). */
  releaseDate: number | null;
  description: string | null;
}

/** Succès débloqués sur le total, lus dans le cache local de Steam. */
export interface Achievements {
  unlocked: number;
  total: number;
  /** Date du dernier succès débloqué (secondes Unix). */
  lastUnlocked: number | null;
}

export interface Game {
  id: string;
  title: string;
  platform: Platform;
  platformId: string;
  installDir: string | null;
  coverUrl: string | null;
  /** Icône de l'exécutable, pour la jaquette générée. */
  iconUrl: string | null;
  heroUrl: string | null;
  logoUrl: string | null;
  /** Dernière session (secondes Unix), launcher ou Ludex. */
  lastPlayed: number | null;
  /** Taille sur le disque, en octets. */
  sizeOnDisk: number | null;
  /** Temps de jeu en minutes, `null` si personne ne l'a mesuré. */
  playtimeMinutes: number | null;
  /** Chiffre du launcher (Steam, GOG Galaxy) ou sessions mesurées par Ludex : jamais additionnés. */
  playtimeSource: PlaytimeSource | null;
  /** `null` : pas de succès, ou progression inconnue (Steam seulement pour l'instant). */
  achievements: Achievements | null;
  /** Application plutôt qu'un jeu (masquée par défaut). */
  isTool: boolean;
  favorite: boolean;
  hidden: boolean;
  launchLabel: string;
  /** Genres, studio, éditeur, sortie et description ; `null` tant qu'inconnus. */
  details: GameDetails | null;
}

export interface SourceReport {
  platform: Platform;
  installed: boolean;
  count: number;
  error: string | null;
}

export interface ScanResult {
  games: Game[];
  sources: SourceReport[];
}

/** Session d'un jeu lancé depuis Ludex. Le temps de jeu n'accompagne que les mises à jour du cumul. */
export interface SessionEvent {
  id: string;
  phase: SessionPhase;
  playtimeMinutes: number | null;
  playtimeSource: PlaytimeSource | null;
}

export const PLATFORM_LABEL: Record<Platform, string> = {
  steam: "Steam",
  epic: "Epic Games",
  gog: "GOG",
  ubisoft: "Ubisoft",
  rockstar: "Rockstar",
  xbox: "Xbox",
  riot: "Riot",
  battlenet: "Battle.net",
  ea: "EA",
  other: "Autres",
};

export const scanLibrary = () => invoke<ScanResult>("scan_library");
/** Renvoie l'heure du lancement (secondes Unix). */
export const launchGame = (id: string) => invoke<number>("launch_game", { id });
export const setFavorite = (id: string, value: boolean) => invoke<void>("set_favorite", { id, value });
export const setHidden = (id: string, value: boolean) => invoke<void>("set_hidden", { id, value });
export const openInstallDir = (id: string) => invoke<void>("open_install_dir", { id });
/** Sessions en cours, pour retrouver l'état « En jeu » au chargement. */
export const gameSessions = () => invoke<SessionEvent[]>("game_sessions");
/** Session terminée, mesurée par Ludex (parties lancées depuis Ludex, une minute au moins). */
export interface PastSession {
  gameId: string;
  /** Début (secondes Unix). */
  start: number;
  /** Durée de jeu, en secondes. */
  secs: number;
  /** Steam ou GOG Galaxy : le temps de jeu de référence reste celui du launcher. */
  launcher: boolean;
}
/** Historique d'un jeu, la session la plus récente d'abord. */
export const gameHistory = (id: string) => invoke<PastSession[]>("game_history", { id });
/** Jaquette trouvée après le scan (recherche sur la boutique Steam en arrière-plan). */
export interface ArtEvent {
  id: string;
  coverUrl: string | null;
  heroUrl: string | null;
  logoUrl: string | null;
}
export const onGameArt = (handler: (event: ArtEvent) => void): Promise<UnlistenFn> =>
  listen<ArtEvent>("game-art", (e) => handler(e.payload));
/** Fiche reçue après le scan (boutique Steam, en arrière-plan). */
export interface DetailsEvent {
  id: string;
  details: GameDetails;
}
export const onGameDetails = (handler: (event: DetailsEvent) => void): Promise<UnlistenFn> =>
  listen<DetailsEvent>("game-details", (e) => handler(e.payload));
export const onGameSession = (handler: (event: SessionEvent) => void): Promise<UnlistenFn> =>
  listen<SessionEvent>("game-session", (e) => handler(e.payload));
