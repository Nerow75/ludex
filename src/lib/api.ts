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

/**
 * État de l'installation chez le launcher (Steam, Epic). Le jeu reste lançable :
 * le launcher termine la mise à jour au lancement. `progress` en %, `null` si inconnue.
 */
export type InstallState =
  | { kind: "ready" }
  | { kind: "update_required" }
  | { kind: "queued" }
  | { kind: "updating"; progress: number | null }
  | { kind: "paused"; progress: number | null }
  | { kind: "incomplete" };

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
  /** Mise à jour ou téléchargement en attente, `ready` sinon. */
  installState: InstallState;
  /** Application plutôt qu'un jeu (masquée par défaut). */
  isTool: boolean;
  /** Ajouté à la main (« Autres ») : il peut être retiré. */
  manual: boolean;
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
/** Jeu ajouté ou modifié, avec la bibliothèque relue après l'opération. */
export interface LibraryUpdate {
  id: string;
  title: string;
  scan: ScanResult;
}
/** Titre libre : 80 caractères au plus (nettoyé côté Rust). */
export const MAX_TITLE = 80;
/**
 * Ouvre le sélecteur de fichier natif (côté Rust) et ajoute l'exécutable choisi.
 * `null` : sélection annulée. Aucun chemin ne passe par le front.
 */
export const addGame = () => invoke<LibraryUpdate | null>("add_game");
/** Rouvre le sélecteur dans le dossier du jeu (« Autres » seulement). `null` : annulé. */
export const changeExecutable = (id: string) => invoke<LibraryUpdate | null>("change_executable", { id });
/** Titre vide : retour au titre d'origine. */
export const renameGame = (id: string, title: string) => invoke<LibraryUpdate>("rename_game", { id, title });
/** Jeux ajoutés à la main seulement (les autres se masquent). */
export const removeGame = (id: string) => invoke<ScanResult>("remove_game", { id });
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
  /** Plusieurs sessions d'un même jour regroupées (historique très long) : heure de début inconnue. */
  merged?: boolean;
}
/** Historique d'un jeu, la session la plus récente d'abord. */
export const gameHistory = (id: string) => invoke<PastSession[]>("game_history", { id });
/** Dernière session de chaque jeu de l'historique, la plus récente d'abord. */
export const lastSessions = () => invoke<PastSession[]>("last_sessions");
/** Temps de jeu des derniers jours, tiré de l'historique (sessions terminées, chacune comptée une fois). */
export interface RecentPlaytime {
  days: number;
  secs: number;
  sessions: number;
  games: number;
}
export const recentPlaytime = () => invoke<RecentPlaytime>("recent_playtime");
/** Période du récap : valeur fermée, seule chose (avec un recul entier) que le front envoie. */
export type RecapPeriod = "week" | "month" | "year";
export interface RecapGame {
  gameId: string;
  secs: number;
  sessions: number;
}
/** Récap d'une période, calculé côté Rust depuis l'historique (fuseau local, sessions comptées une fois). */
export interface Recap {
  period: RecapPeriod;
  /** Périodes en arrière (0 = en cours). */
  back: number;
  /** Premier et dernier jour, dates locales `AAAA-MM-JJ` comprises. */
  start: string;
  end: string;
  elapsedDays: number;
  totalSecs: number;
  previousSecs: number;
  /** Période précédente, sur autant de jours que ceux déjà écoulés ici. */
  previousToDateSecs: number;
  sessions: number;
  games: number;
  top: RecapGame[];
  longest: { gameId: string; start: number; secs: number } | null;
  bestStreak: number;
  currentStreak: number;
  /** Lundi d'abord. */
  weekdaySecs: number[];
  favoriteWeekday: number | null;
  /** Nuit, matin, après-midi, soir. */
  momentSecs: number[];
  favoriteMoment: number | null;
  /** Temps de chaque jour de la période. */
  days: number[];
  newGames: { gameId: string; firstPlayed: number }[];
  achievements: number;
  achievementGames: { gameId: string; count: number }[];
  historySessions: number;
  historySince: number | null;
}
export const recap = (period: RecapPeriod, back: number) => invoke<Recap>("recap", { period, back });
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
/** Bibliothèque relue sans demande du front : jeu installé ou désinstallé, ou retour sur la fenêtre
 * après un long moment. `added` et `removed` : titres des jeux apparus et disparus (hors masqués). */
export interface LibraryEvent {
  added: string[];
  removed: string[];
  scan: ScanResult;
}
export const onLibraryChanged = (handler: (event: LibraryEvent) => void): Promise<UnlistenFn> =>
  listen<LibraryEvent>("library-changed", (e) => handler(e.payload));
/** Nouvelle version de Ludex publiée (vérifiée côté Rust, jamais en dev). */
export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
}
/** Mise à jour déjà trouvée, pour la retrouver après un rechargement de l'interface. */
export const availableUpdate = () => invoke<UpdateInfo | null>("available_update");
/** Télécharge et installe la mise à jour trouvée : Ludex se ferme puis se relance. */
export const installUpdate = () => invoke<void>("install_update");
export const onUpdateAvailable = (handler: (event: UpdateInfo) => void): Promise<UnlistenFn> =>
  listen<UpdateInfo>("update-available", (e) => handler(e.payload));
/** Jeu lancé depuis le menu de la zone de notification : date du lancement, ou erreur. */
export interface TrayLaunchEvent {
  id: string;
  title: string;
  lastPlayed: number | null;
  error: string | null;
}
export const onTrayLaunch = (handler: (event: TrayLaunchEvent) => void): Promise<UnlistenFn> =>
  listen<TrayLaunchEvent>("tray-launch", (e) => handler(e.payload));
/** Première fermeture de la fenêtre : Ludex se cache dans la zone de notification. */
export const onTrayHint = (handler: () => void): Promise<UnlistenFn> => listen("tray-hint", () => handler());
