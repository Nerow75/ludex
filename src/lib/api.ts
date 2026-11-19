import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Platform = "steam" | "epic" | "gog" | "ubisoft" | "rockstar" | "xbox" | "riot" | "battlenet" | "ea" | "other";
export type PlaytimeSource = "launcher" | "ludex";
/** `detecting` : lancé, le jeu n'est pas encore visible ; `running` : en jeu ; `ended` : terminé. */
export type SessionPhase = "detecting" | "running" | "ended";

export const PLATFORMS: readonly Platform[] = ["steam", "epic", "gog", "ubisoft", "ea", "battlenet", "rockstar", "xbox", "riot", "other"];

/** Ambiance tirée des tags Steam et des catégories de la boutique (`moods.rs`). */
export type Mood = "chill" | "intense" | "story" | "friends";
export const MOODS: readonly Mood[] = ["chill", "intense", "story", "friends"];
export const MOOD_LABEL: Record<Mood, string> = { chill: "Chill", intense: "Intense", story: "Histoire", friends: "Entre amis" };

/** Prise en charge de la manette annoncée par la boutique Steam. */
export type ControllerSupport = "full" | "partial";

/** Fiche lue sur la boutique Steam pour les jeux dont on connaît l'appid. */
export interface GameDetails {
  /** Du plus voté au moins voté, trois au plus. */
  genres: string[];
  /** Tags Steam en français, du plus voté au moins voté, dix au plus (genres compris). */
  tags: string[];
  moods: Mood[];
  /** `null` : la boutique n'en dit rien. */
  controller: ControllerSupport | null;
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
  /** Progression choisie par l'utilisateur, `null` : aucun statut. */
  status: Status | null;
  /** Note perso, `null` : aucune. */
  note: string | null;
}

/** Progression d'un jeu : valeur fermée, vérifiée côté Rust. */
export type Status = "todo" | "playing" | "done" | "dropped";
/** Ordre des colonnes de « Progression » et des boutons de la fiche. */
export const STATUSES: readonly Status[] = ["todo", "playing", "done", "dropped"];
export const STATUS_LABEL: Record<Status, string> = { todo: "À faire", playing: "En cours", done: "Terminé", dropped: "Abandonné" };

/** Collection : nom et ids des jeux, dans l'ordre d'ajout. */
export interface Collection {
  id: string;
  name: string;
  games: string[];
}
/** Collections de l'utilisateur, et celles du client Steam (lecture seule). */
export interface CollectionsInfo {
  mine: Collection[];
  steam: Collection[];
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
/** Note perso : 500 caractères au plus, nom de collection : 40 (nettoyés côté Rust). */
export const MAX_NOTE = 500;
export const MAX_COLLECTION_NAME = 40;
export const setStatus = (id: string, status: Status | null) => invoke<void>("set_status", { id, status });
/** Renvoie la note enregistrée (nettoyée), `null` : effacée. */
export const setNote = (id: string, note: string) => invoke<string | null>("set_note", { id, note });
export const getCollections = () => invoke<CollectionsInfo>("collections");
/** `game` : jeu à mettre tout de suite dans la nouvelle collection. */
export const createCollection = (name: string, game: string | null) =>
  invoke<{ id: string; mine: Collection[] }>("create_collection", { name, game });
export const renameCollection = (id: string, name: string) => invoke<Collection[]>("rename_collection", { id, name });
export const deleteCollection = (id: string) => invoke<Collection[]>("delete_collection", { id });
export const setInCollection = (collection: string, id: string, value: boolean) =>
  invoke<Collection[]>("set_in_collection", { collection, id, value });
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
/** Un succès Steam avec la progression du compte (liste lue à la demande, pas pendant le scan). */
export interface Achievement {
  /** Nom interne, clé des pourcentages mondiaux. */
  apiName: string;
  /** Vide pour un succès caché pas encore débloqué (rien ne le dévoile). */
  name: string;
  description: string | null;
  /** Icône en couleur et grisée, sur le CDN de Steam. */
  icon: string | null;
  iconGray: string | null;
  hidden: boolean;
  unlocked: boolean;
  /** Date du déblocage (secondes Unix). */
  unlockedAt: number | null;
  /** Part des joueurs Steam qui l'ont débloqué, en %. */
  globalPercent: number | null;
}
/** Succès d'un jeu : non débloqués les plus faciles d'abord, puis débloqués, le plus récent d'abord. */
export const gameAchievements = (id: string) => invoke<Achievement[]>("game_achievements", { id });
/** Succès débloqué montré par la vitrine. */
export interface ShowcaseUnlock {
  gameId: string;
  gameTitle: string;
  apiName: string;
  name: string;
  description: string | null;
  icon: string | null;
  percent: number | null;
  unlockedAt: number | null;
}
export interface PerfectGame {
  gameId: string;
  gameTitle: string;
  total: number;
  /** Dernier déblocage, donc date du 100 %. */
  completedAt: number | null;
}
/** Section « Vitrine » (jeux Steam non masqués). */
export interface Showcase {
  unlocked: number;
  total: number;
  /** Jeux avec des succès. */
  games: number;
  /** Le plus récemment terminé d'abord. */
  perfect: PerfectGame[];
  /** Les dix plus rares, le plus rare d'abord ; vide sans pourcentages mondiaux (hors ligne). */
  rarest: ShowcaseUnlock[];
  /** Les dix derniers déblocages. */
  recent: ShowcaseUnlock[];
}
export const showcase = () => invoke<Showcase>("showcase");
/** Place prise par les jeux d'un launcher sur un disque. */
export interface PlatformShare {
  platform: Platform;
  bytes: number;
  games: number;
}
/** Disque qui porte au moins un jeu de Ludex. Octets partout. */
export interface DiskUsage {
  /** « C: », ou le point de montage. */
  letter: string;
  /** Nom du volume, vide s'il n'en a pas. */
  name: string;
  total: number;
  free: number;
  /** Place des jeux de taille connue. */
  gamesBytes: number;
  games: number;
  /** Jeux dont la taille n'est pas encore connue. */
  unknown: number;
  /** La plus grosse part d'abord. */
  platforms: PlatformShare[];
  /** Moins de 10 % d'espace libre. */
  low: boolean;
}
/** Jeu pas lancé depuis six mois, ou jamais lancé. */
export interface UninstallCandidate {
  id: string;
  title: string;
  platform: Platform;
  bytes: number;
  lastPlayed: number | null;
  /** Le launcher note les lancements (Steam, GOG Galaxy) ; sinon seuls ceux de Ludex sont connus. */
  launcherDates: boolean;
  drive: string | null;
  /** Le launcher a une désinstallation que Ludex sait ouvrir (Steam) ; sinon, le dossier. */
  uninstall: boolean;
}
export interface StorageReport {
  disks: DiskUsage[];
  /** Le plus gros d'abord. */
  candidates: UninstallCandidate[];
  /** Des dossiers sont en cours de mesure : le relevé va encore bouger. */
  measuring: boolean;
}
export const storageReport = () => invoke<StorageReport>("storage");
/** Ouvre la désinstallation chez le launcher (Steam, qui demande confirmation). Ludex ne supprime rien. */
export const uninstallGame = (id: string) => invoke<void>("uninstall_game", { id });
/** Taille d'un jeu mesurée par Ludex (dossier d'installation), après le scan. */
export interface SizeEvent {
  id: string;
  sizeOnDisk: number;
}
export const onGameSize = (handler: (event: SizeEvent) => void): Promise<UnlistenFn> =>
  listen<SizeEvent>("game-size", (e) => handler(e.payload));
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
/** Base des sauvegardes (Ludusavi) : `loading` au premier téléchargement, `unavailable` hors ligne sans copie locale. */
export type SavesStatus = "loading" | "ready" | "unavailable";
/** `auto` : fin d'une partie ; `manual` : « Sauvegarder maintenant » ; `before_restore` : état remplacé par une restauration. */
export type BackupReason = "auto" | "manual" | "before_restore";
/** Copie des sauvegardes d'un jeu. `id` : seul identifiant renvoyé à Rust. */
export interface SavesCopy {
  id: string;
  /** Date de la copie (secondes Unix). */
  created: number;
  reason: BackupReason;
  size: number;
  files: number;
}
/** Emplacement de sauvegarde trouvé sur ce PC (affichage seulement). */
export interface SavesLocation {
  path: string;
  size: number;
  files: number;
}
export interface GameSaves {
  status: SavesStatus;
  /** Nom du jeu dans la base, `null` : jeu inconnu de la base. */
  name: string | null;
  /** Le jeu garde aussi des données dans le registre, non copiées. */
  registry: boolean;
  locations: SavesLocation[];
  /** La plus récente d'abord. */
  copies: SavesCopy[];
  /** Dossier des copies (affichage seulement). */
  dir: string;
  auto: boolean;
}
export interface BackupOutcome {
  copy: SavesCopy;
  /** Rien n'a changé depuis la copie précédente : aucune nouvelle copie. */
  unchanged: boolean;
}
export interface RestoreOutcome {
  restored: number;
  /** Copie de l'état remplacé, `null` s'il n'y avait rien à copier. */
  safety: SavesCopy | null;
}
export interface SavesSummary {
  id: string;
  title: string;
  locations: number;
  size: number;
  copies: number;
  lastCopy: number | null;
  registry: boolean;
}
export interface SavesOverview {
  status: SavesStatus;
  dir: string;
  auto: boolean;
  games: SavesSummary[];
}
/** Emplacements trouvés, copies et réglages d'un jeu. */
export const gameSaves = (id: string) => invoke<GameSaves>("game_saves", { id });
export const backupSaves = (id: string) => invoke<BackupOutcome>("backup_saves", { id });
/** Refusée si le jeu tourne. L'état actuel est copié avant d'être remplacé. */
export const restoreSaves = (id: string, backupId: string) => invoke<RestoreOutcome>("restore_saves", { id, backupId });
/** Sélecteur de dossier natif (côté Rust). Renvoie le dossier choisi à afficher, `null` : annulé. */
export const chooseSavesDir = () => invoke<string | null>("choose_saves_dir");
export const setAutoSaves = (value: boolean) => invoke<void>("set_auto_saves", { value });
/** Jeux avec des sauvegardes trouvées ou des copies, par titre. */
export const savesOverview = () => invoke<SavesOverview>("saves_overview");
/** Copie automatique faite (ou ratée) à la fin d'une partie. */
export interface SavesEvent {
  id: string;
  title: string;
  copy: SavesCopy | null;
  unchanged: boolean;
  error: string | null;
}
export const onSavesBackup = (handler: (event: SavesEvent) => void): Promise<UnlistenFn> =>
  listen<SavesEvent>("saves-backup", (e) => handler(e.payload));
/** Première fermeture de la fenêtre : Ludex se cache dans la zone de notification. */
export const onTrayHint = (handler: () => void): Promise<UnlistenFn> => listen("tray-hint", () => handler());
/** Capture d'écran d'un jeu (Steam ou Xbox Game Bar). Aucun chemin : une empreinte et des URL `ludex`. */
export interface Screenshot {
  /** Empreinte de la capture : seule référence à renvoyer à Rust. */
  key: string;
  /** `null` : rattachement impossible (« Autres captures »). */
  gameId: string | null;
  source: "steam" | "gamebar";
  /** Dossier Steam de la capture (jeu désinstallé compris). */
  steamAppid: number | null;
  /** Date de la capture (secondes Unix). */
  taken: number;
  /** Taille du fichier, en octets. */
  size: number;
  width: number | null;
  height: number | null;
  /** Vidéo de la Game Bar (`.mp4`) : pas de miniature. */
  video: boolean;
  /** Miniature pour les listes. */
  thumbUrl: string;
  /** Image (ou vidéo) pleine taille, pour la visionneuse. */
  url: string;
}
/** Captures d'un jeu, ou « Autres captures » (`gameId` à `null`). */
export interface ScreenshotGroup {
  gameId: string | null;
  title: string;
  shots: Screenshot[];
}
/** Captures d'un jeu, la plus récente d'abord. */
export const gameScreenshots = (id: string) => invoke<Screenshot[]>("game_screenshots", { id });
/** Toutes les captures, groupées par jeu : le jeu à la capture la plus récente d'abord, « Autres captures » en dernier. */
export const allScreenshots = () => invoke<ScreenshotGroup[]>("all_screenshots");
/** Montre la capture dans l'Explorateur (le chemin est retrouvé côté Rust). */
export const showScreenshot = (shot: Screenshot) => invoke<void>("show_screenshot", { id: shot.gameId, key: shot.key });
