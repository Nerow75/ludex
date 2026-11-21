import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Platform = "steam" | "epic" | "gog" | "ubisoft" | "rockstar" | "xbox" | "riot" | "battlenet" | "ea" | "amazon" | "itch" | "other";
export type PlaytimeSource = "launcher" | "ludex";
/** `detecting` : lancé, le jeu n'est pas encore visible ; `running` : en jeu ; `ended` : terminé. */
export type SessionPhase = "detecting" | "running" | "ended";

export const PLATFORMS: readonly Platform[] = ["steam", "epic", "gog", "ubisoft", "ea", "battlenet", "rockstar", "xbox", "riot", "amazon", "itch", "other"];

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

/** Durées moyennes HowLongToBeat, en minutes (`null` : pas assez de joueurs). */
export interface Hltb {
  /** Nom du jeu sur HowLongToBeat. */
  name: string;
  /** Histoire principale. */
  main: number | null;
  /** Histoire et extras. */
  plus: number | null;
  /** Tout à 100 %. */
  full: number | null;
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
  /** Débit du téléchargement en cours, posé par le front depuis `game-install` (jamais par le scan). */
  download?: DownloadRate;
  /** Application plutôt qu'un jeu (masquée par défaut). */
  isTool: boolean;
  /** Ajouté à la main (« Autres ») : il peut être retiré. */
  manual: boolean;
  favorite: boolean;
  hidden: boolean;
  launchLabel: string;
  /** Genres, studio, éditeur, sortie et description ; `null` tant qu'inconnus. Genres corrigés par l'utilisateur compris. */
  details: GameDetails | null;
  /** Les genres de `details` sont ceux choisis par l'utilisateur, pas ceux de Steam. */
  customGenres: boolean;
  /** Alias de recherche choisis par l'utilisateur (nettoyés côté Rust), cherchés comme le titre. */
  aliases: string[];
  /** Images choisies par l'utilisateur, qui priment sur celles du launcher. */
  customArt: ArtKind[];
  /** Progression choisie par l'utilisateur, `null` : aucun statut. */
  status: Status | null;
  /** Note perso, `null` : aucune. */
  note: string | null;
  /** Durée de vie (HowLongToBeat) ; `null` tant qu'inconnue. */
  hltb: Hltb | null;
  /** Options de lancement choisies par l'utilisateur (validées côté Rust), `null` : aucune. */
  launchArgs: string | null;
  /** Pourquoi ce jeu ne peut pas recevoir d'options, `null` s'il le peut. */
  launchArgsRefusal: string | null;
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

/** Image d'un jeu que l'utilisateur peut remplacer : jaquette ou image de fond. */
export type ArtKind = "cover" | "hero";

export interface SourceReport {
  platform: Platform;
  installed: boolean;
  count: number;
  error: string | null;
  /** Désactivé dans les Paramètres : ses jeux (`count`) sont écartés de la bibliothèque. */
  disabled: boolean;
}

export interface ScanResult {
  games: Game[];
  sources: SourceReport[];
  /** Premier affichage du démarrage : EA, Battle.net et « Autres » encore en lecture (absents de
   * `games` et `sources`), complément annoncé par `library-changed`. */
  partial: boolean;
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
  amazon: "Amazon Games",
  itch: "itch.io",
  other: "Autres",
};

export const scanLibrary = () => invoke<ScanResult>("scan_library");
/** Renvoie l'heure du lancement (secondes Unix). */
export const launchGame = (id: string) => invoke<number>("launch_game", { id });
export const setFavorite = (id: string, value: boolean) => invoke<void>("set_favorite", { id, value });
export const setHidden = (id: string, value: boolean) => invoke<void>("set_hidden", { id, value });
export const openInstallDir = (id: string) => invoke<void>("open_install_dir", { id });
/** Raccourci `.url` du jeu sur le Bureau (lien `ludex://launch/<id>`, écrit côté Rust) ; renvoie le nom du fichier. */
export const createShortcut = (id: string) => invoke<string>("create_shortcut", { id });
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
/** Longueur maximale des options de lancement (`launch_args::MAX_LEN`). */
export const MAX_LAUNCH_ARGS = 256;
export const MAX_COLLECTION_NAME = 40;
export const setStatus = (id: string, status: Status | null) => invoke<void>("set_status", { id, status });
/** Renvoie la note enregistrée (nettoyée), `null` : effacée. */
export const setNote = (id: string, note: string) => invoke<string | null>("set_note", { id, note });
/** Options de lancement d'un jeu, validées et normalisées côté Rust ; renvoie le texte enregistré (`null` : effacées). */
export const setLaunchArgs = (id: string, text: string) => invoke<string | null>("set_launch_args", { id, text });
/** Alias de recherche, séparés par des virgules (nettoyés côté Rust) ; renvoie la liste enregistrée (vide : effacés). */
export const setAliases = (id: string, text: string) => invoke<string[]>("set_aliases", { id, text });
/** Longueur d'un alias et nombre d'alias par jeu (`corrections.rs`). */
export const MAX_ALIAS = 40;
export const MAX_ALIASES = 8;
/** Genres corrigés (liste de Ludex, 3 au plus) ; vide : genres de Steam. Renvoie la fiche à jour. */
export const setGenres = (id: string, genres: string[]) =>
  invoke<{ details: GameDetails | null; customGenres: boolean }>("set_genres", { id, genres });
/** Genres que l'on peut choisir dans la fiche (`metadata::GENRES`). */
export const genreChoices = () => invoke<string[]>("genre_choices");
/** Genres corrigés par jeu, trois au plus. */
export const MAX_GENRES = 3;
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
/** Date d'installation de chaque jeu (création de son dossier, secondes Unix) ; jeu absent : inconnue. */
export const installDates = () => invoke<Record<string, number>>("install_dates");
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
/** Succès le plus rare débloqué pendant une période (pourcentage mondial du cache, aucune requête). */
export interface RareUnlock {
  gameId: string;
  name: string;
  description: string | null;
  icon: string | null;
  percent: number;
  unlockedAt: number;
}
export const recapRarest = (period: RecapPeriod, back: number) => invoke<RareUnlock | null>("recap_rarest", { period, back });
/**
 * Enregistre une page de la story : seuls les octets du PNG (1080 × 1920) partent, en corps brut ;
 * Rust les vérifie et ouvre lui-même le sélecteur d'enregistrement. Renvoie le nom du fichier, `null` si annulé.
 */
export const saveStoryImage = (png: Uint8Array) => invoke<string | null>("save_story_image", png);
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

/** Ligne d'une configuration requise (texte de la boutique Steam, sans HTML). */
export type RequirementKind = "system" | "cpu" | "memory" | "gpu" | "directx" | "storage" | "notes";
/** Ce PC face à une ligne : suffit, en dessous, ou comparaison impossible. */
export type RequirementVerdict = "ok" | "below" | "unknown";
export interface RequirementLine {
  kind: RequirementKind;
  text: string;
  verdict: RequirementVerdict;
}
export interface PcGpu {
  name: string;
  /** Mémoire dédiée, en octets. */
  vram: number;
}
/** Matériel de ce PC, pour la colonne « Ce PC ». */
export interface PcSpecs {
  windows: string;
  cpu: string | null;
  cores: number | null;
  threads: number | null;
  /** Mémoire installée, en octets. */
  memory: number | null;
  /** Cartes dédiées d'abord, la plus grosse mémoire d'abord. */
  gpus: PcGpu[];
  /** Version de DirectX la plus haute prise en charge par une carte. */
  directx: number | null;
  /** Disque du jeu : place libre et place prise par le jeu (octets). */
  disk: { letter: string; free: number; game: number | null } | null;
}
export interface Requirements {
  minimum: RequirementLine[];
  recommended: RequirementLine[];
  pc: PcSpecs;
}
/** Configuration requise d'un jeu (appid Steam), comparée à ce PC ; `null` si rien de connu. */
export const gameRequirements = (id: string) => invoke<Requirements | null>("game_requirements", { id });
/** Mod Steam Workshop installé. */
export interface WorkshopMod {
  /** Id Workshop (texte : il dépasse les entiers sûrs de JavaScript). */
  id: string;
  /** Titre donné par Steam, `null` tant qu'il est inconnu. */
  title: string | null;
  /** Vignette réduite (`images.steamusercontent.com`), reconstruite côté Rust. */
  previewUrl: string | null;
  /** Octets sur le disque. */
  size: number;
  /** Date de la version installée (secondes Unix). */
  updated: number | null;
  /** Steam connaît une version plus récente, pas encore téléchargée. */
  pending: boolean;
  /** Le dossier du mod existe. */
  present: boolean;
}
export interface WorkshopMods {
  mods: WorkshopMod[];
  total: number;
  /** Le dossier des mods du jeu existe. */
  folder: boolean;
  /** Des titres restent à demander à Steam. */
  stale: boolean;
}
/** Mods Workshop installés d'un jeu Steam (disque et cache seulement) ; `null` : aucun manifeste. */
export const gameMods = (id: string) => invoke<WorkshopMods | null>("game_mods", { id });
/** Même liste, après avoir demandé à Steam les titres et vignettes manquants. */
export const refreshMods = (id: string) => invoke<WorkshopMods | null>("refresh_mods", { id });
export const openModsDir = (id: string) => invoke<void>("open_mods_dir", { id });
/** Page d'un mod dans Steam ; l'id est revérifié côté Rust contre les mods installés du jeu. */
export const openModPage = (id: string, modId: string) => invoke<void>("open_mod_page", { id, modId });
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
/** Dossier relevé par la vue avancée du Stockage. */
export interface FootprintFolder {
  /** Clé de l'élément, renvoyée pour l'ouvrir (jamais un chemin). */
  item: string;
  label: string;
  kind: "install" | "cache" | "mods" | "saves";
  path: string;
  exists: boolean;
  /** Fichier seul (une sauvegarde). */
  file: boolean;
  /** Octets, `null` : inconnu. */
  bytes: number | null;
  /** Clé du dossier de la liste qui le contient (`install` : part avec le jeu), `null` : à part. */
  inside: string | null;
}
/** Entrée de « Applications installées » de Windows. */
export interface FootprintUninstall {
  item: string;
  /** `HKLM\…\Uninstall\<nom>`. */
  key: string;
  exists: boolean;
  name: string | null;
  publisher: string | null;
  version: string | null;
  /** Taille déclarée à Windows, octets. */
  declaredSize: number | null;
  /** `AAAA-MM-JJ`. */
  installDate: string | null;
  location: string | null;
  command: string | null;
  /** `run` : désinstalleur lancé par Ludex ; `steam` : passe par Steam ; `none` : voir `refusal`. */
  action: "run" | "steam" | "none";
  refusal: string | null;
}
export interface FootprintKey {
  label: string;
  key: string;
  exists: boolean;
}
/** Ce qu'un jeu laisse sur le PC (lecture seule). */
export interface Footprint {
  id: string;
  title: string;
  platform: Platform;
  /** Encore dans la bibliothèque ; sinon tout ce qui existe encore est un reste. */
  installed: boolean;
  folders: FootprintFolder[];
  uninstall: FootprintUninstall[];
  keys: FootprintKey[];
  /** La base des sauvegardes signale aussi des données dans le registre. */
  savesRegistry: boolean;
  /** Pourquoi les sauvegardes sont inconnues (base pas prête, jeu absent de la base). */
  savesNote: string | null;
  /** « Désinstaller avec Steam » possible. */
  steam: boolean;
  /** Paquet du Microsoft Store : se désinstalle depuis les Paramètres de Windows. */
  storeApp: boolean;
  /** Éléments encore présents alors que le jeu a quitté la bibliothèque. */
  leftovers: number;
}
/** `rescan` : relit la bibliothèque d'abord (après un désinstalleur). */
export const gameFootprint = (id: string, rescan = false) => invoke<Footprint>("game_footprint", { id, rescan });
export const openFootprintItem = (id: string, item: string) => invoke<void>("open_footprint_item", { id, item });
/** Lance le désinstalleur officiel (commande relue dans le registre côté Rust). */
export const runUninstaller = (id: string, item: string) => invoke<void>("run_uninstaller", { id, item });
export const openAppsSettings = () => invoke<void>("open_apps_settings");
/** Taille d'un jeu mesurée par Ludex (dossier d'installation), après le scan. */
export interface SizeEvent {
  id: string;
  sizeOnDisk: number;
}
export const onGameSize = (handler: (event: SizeEvent) => void): Promise<UnlistenFn> =>
  listen<SizeEvent>("game-size", (e) => handler(e.payload));
/** Débit estimé d'un téléchargement : octets par seconde, secondes restantes (`null` : inconnus). */
export interface DownloadRate {
  speed: number | null;
  remaining: number | null;
}
/** État d'installation changé pendant un téléchargement ou une mise à jour Steam (manifeste relu, sans rescan).
 * Envoyé seulement quand l'état, le % ou la taille change. */
export interface InstallEvent extends DownloadRate {
  id: string;
  installState: InstallState;
  sizeOnDisk: number | null;
}
export const onGameInstall = (handler: (event: InstallEvent) => void): Promise<UnlistenFn> =>
  listen<InstallEvent>("game-install", (e) => handler(e.payload));
/** Jaquette trouvée après le scan (recherche sur la boutique Steam en arrière-plan), image passée
 * en cache, ou jaquette perso choisie ou retirée. */
export interface ArtEvent {
  id: string;
  coverUrl: string | null;
  heroUrl: string | null;
  logoUrl: string | null;
  customArt: ArtKind[];
}
/**
 * Ouvre le sélecteur d'image natif (côté Rust), copie l'image choisie dans le dossier de Ludex et
 * la pose sur le jeu. `null` : sélection annulée. Aucun chemin ne passe par le front.
 */
export const chooseCover = (id: string, kind: ArtKind) => invoke<ArtEvent | null>("choose_cover", { id, kind });
/** Retire l'image perso : l'image d'origine revient. */
export const resetCover = (id: string, kind: ArtKind) => invoke<ArtEvent>("reset_cover", { id, kind });
export const onGameArt = (handler: (event: ArtEvent) => void): Promise<UnlistenFn> =>
  listen<ArtEvent>("game-art", (e) => handler(e.payload));
/** Fiche reçue après le scan (boutique Steam, en arrière-plan). */
export interface DetailsEvent {
  id: string;
  details: GameDetails;
}
export const onGameDetails = (handler: (event: DetailsEvent) => void): Promise<UnlistenFn> =>
  listen<DetailsEvent>("game-details", (e) => handler(e.payload));
/** Durée de vie reçue après le scan (HowLongToBeat, en arrière-plan). */
export interface HltbEvent {
  id: string;
  hltb: Hltb;
}
export const onGameHltb = (handler: (event: HltbEvent) => void): Promise<UnlistenFn> =>
  listen<HltbEvent>("game-hltb", (e) => handler(e.payload));
export const onGameSession = (handler: (event: SessionEvent) => void): Promise<UnlistenFn> =>
  listen<SessionEvent>("game-session", (e) => handler(e.payload));
/** Bibliothèque relue sans demande du front : jeu installé ou désinstallé, ou retour sur la fenêtre
 * après un long moment. `added` et `removed` : titres des jeux apparus et disparus (hors masqués). */
export interface LibraryEvent {
  added: string[];
  removed: string[];
  scan: ScanResult;
  /** Fin du premier scan (sources lentes) : pas de toast « Nouveau jeu ». */
  complement: boolean;
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
/** Ouvre un emplacement trouvé par Ludex (Rust revérifie qu'il fait partie des emplacements du jeu). */
export const openSaveLocation = (id: string, path: string) => invoke<void>("open_save_location", { id, path });
/** Montre une copie dans l'Explorateur. */
export const openSaveCopy = (id: string, backupId: string) => invoke<void>("open_save_copy", { id, backupId });
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
/** Morceau d'une session dans un jour du journal. */
export interface JournalSession {
  /** Début de la session (secondes Unix), `null` : sessions regroupées d'un ancien jour (heure inconnue). */
  start: number | null;
  /** Temps joué ce jour-là, en secondes. */
  secs: number;
  /** Partie commencée la veille (passée minuit). */
  continued: boolean;
}
/** Ce qu'un jeu a laissé dans un jour. `gameId` à `null` : captures sans jeu (« Autres captures »). */
export interface JournalGame {
  gameId: string | null;
  title: string;
  secs: number;
  sessions: JournalSession[];
  /** Succès débloqués ce jour-là (compte sûr, même sans détail). */
  unlocked: number;
  /** Détail de ces succès, dans l'ordre de la journée. */
  achievements: Achievement[];
  shots: Screenshot[];
}
export interface JournalDay {
  /** Date locale `AAAA-MM-JJ`. */
  date: string;
  secs: number;
  games: JournalGame[];
}
export interface JournalPage {
  /** Du plus récent au plus ancien. */
  days: JournalDay[];
  /** À renvoyer en `before` pour la page suivante, `null` : rien de plus ancien. */
  next: string | null;
}
/** Une page du journal (environ 30 jours actifs) avant `before` ; `game` : un seul jeu. */
export const journal = (before: string | null, game: string | null) => invoke<JournalPage>("journal", { before, game });

/** Écran de premier lancement : ce que le scan ne dit pas. */
export interface WelcomeInfo {
  /** Dossiers « Jeux », « Games » lus à la racine des disques. */
  folders: string[];
  /** Aperçu forcé (`LUDEX_WELCOME=1`) : réglages et déplacement des données simulés. */
  preview: boolean;
}
/** `null` : écran déjà vu (ou Ludex utilisé avant cet écran). */
export const welcome = () => invoke<WelcomeInfo | null>("welcome");
/** « C'est parti » : l'écran ne reviendra plus. */
export const welcomeDone = () => invoke<void>("welcome_done");

/* Barre de titre maison : commandes sans argument, la fenêtre visée est celle qui appelle
   (fenêtre principale seulement, refus côté Rust pour les autres). */
export const minimizeWindow = () => invoke<void>("window_minimize");
export const toggleMaximizeWindow = () => invoke<void>("window_toggle_maximize");
/** Même chemin que Alt+F4 : cachée dans la zone de notification si le réglage le demande. */
export const closeWindow = () => invoke<void>("window_close");

/** « Revoir l'écran de premier lancement » (Paramètres) : l'écran redevient dû. */
export const welcomeAgain = () => invoke<WelcomeInfo>("welcome_again");

/** Thème de l'interface ; `system` suit Windows. */
export type Theme = "dark" | "light" | "system" | "oled" | "contrast";
export const THEMES: readonly Theme[] = ["dark", "light", "system", "oled", "contrast"];
export const THEME_LABEL: Record<Theme, string> = {
  dark: "Sombre",
  light: "Clair",
  system: "Comme Windows",
  oled: "Noir OLED",
  contrast: "Contraste élevé",
};

/** Couleur d'accent de l'interface ; turquoise (celle du logo) par défaut. */
export type Accent = "turquoise" | "blue" | "violet" | "pink" | "green";
export const ACCENTS: readonly Accent[] = ["turquoise", "blue", "violet", "pink", "green"];
export const ACCENT_LABEL: Record<Accent, string> = {
  turquoise: "Turquoise",
  blue: "Bleu",
  violet: "Violet",
  pink: "Rose",
  green: "Vert",
};

/** Réglages de l'écran Paramètres (`settings.rs`). */
export interface SettingsInfo {
  theme: Theme;
  accent: Accent;
  /** Format du plugin (« Alt+Space »), vide si désactivé. */
  quickShortcut: string;
  /** Le même, lisible (« Alt+Espace », « Désactivé »). */
  quickShortcutLabel: string;
  launchAtStartup: boolean;
  keepInTray: boolean;
  /** Dossier des copies de sauvegardes (affichage seulement). */
  savesDir: string;
  autoSaves: boolean;
  /** Écran de lancement « cinéma » au clic sur Jouer (vrai par défaut). */
  launchScreen: boolean;
  disabledSources: Platform[];
  /** Jeux masqués par l'utilisateur (outils masqués d'office non compris). */
  hiddenGames: number;
  version: string;
  /** Notifications Windows activées, par type. */
  notifications: NotificationPrefs;
  /** Windows a coupé les notifications de Ludex (ou toutes) : Ludex ne change pas ce réglage. */
  notificationsBlocked: boolean;
}
/** Types de notifications Windows (`notify.rs`). */
export type NotificationKind = "download" | "news" | "lowDisk" | "update" | "savesError" | "sale";
export type NotificationPrefs = Record<NotificationKind, boolean>;
export const settings = () => invoke<SettingsInfo>("settings");
export const setNotification = (kind: NotificationKind, value: boolean) => invoke<SettingsInfo>("set_notification", { kind, value });
/** Clic sur une notification Windows : la fenêtre est déjà affichée, le front ouvre la cible. */
export type NotificationTarget = { target: "game"; id: string; tab: string | null } | { target: "storage" } | { target: "wishlist" } | { target: "window" };
export const onNotificationOpen = (handler: (target: NotificationTarget) => void): Promise<UnlistenFn> =>
  listen<NotificationTarget>("notification-open", (e) => handler(e.payload));
export const setTheme = (theme: Theme) => invoke<SettingsInfo>("set_theme", { theme });
export const setAccent = (accent: Accent) => invoke<SettingsInfo>("set_accent", { accent });
/** Texte au format du plugin (`Ctrl+Shift+KeyL`), validé et enregistré côté Rust ; vide : désactivé. */
export const setQuickShortcut = (text: string) => invoke<SettingsInfo>("set_quick_shortcut", { text });
/** Coupe le raccourci actuel pendant sa saisie (`true`), puis le remet (`false`). */
export const pauseQuickShortcut = (paused: boolean) => invoke<void>("pause_quick_shortcut", { paused });
export const setLaunchAtStartup = (value: boolean) => invoke<SettingsInfo>("set_launch_at_startup", { value });
export const setKeepInTray = (value: boolean) => invoke<SettingsInfo>("set_keep_in_tray", { value });
export const setLaunchScreen = (value: boolean) => invoke<SettingsInfo>("set_launch_screen", { value });
/** Active ou désactive un launcher, puis rescanne : bibliothèque relue. */
export const setSourceEnabled = (platform: Platform, value: boolean) => invoke<ScanResult>("set_source_enabled", { platform, value });
/** Ré-affiche les jeux masqués par l'utilisateur ; renvoie leurs ids. */
export const showAllHidden = () => invoke<string[]>("show_all_hidden");
/** Recherche une mise à jour tout de suite ; `null` : Ludex est à jour. */
export const checkUpdate = () => invoke<UpdateInfo | null>("check_update");
export const onSettingsChanged = (handler: (event: SettingsInfo) => void): Promise<UnlistenFn> =>
  listen<SettingsInfo>("settings-changed", (e) => handler(e.payload));

/** Plein écran réel de la fenêtre principale (mode canapé), commandes sans argument. */
/** Mode canapé : fenêtre agrandie à l'entrée, rendue à sa taille (et plein écran quitté) à la sortie. */
export const enterCouchWindow = () => invoke<void>("window_enter_couch");
export const leaveCouchWindow = () => invoke<void>("window_leave_couch");
/** F11 dans le mode canapé : nouvel état du plein écran, `null` si le réglage l'interdit. */
export const toggleCouchFullscreen = () => invoke<boolean | null>("window_couch_fullscreen");

/** Page de diagnostic des launchers (`diagnostic.rs`), d'après le dernier scan. */
export type DiagSourceState = "unknown" | "absent" | "empty" | "games" | "disabled" | "error";
export interface DiagLocation {
  label: string;
  /** Chemin, ou clé du registre (`HKLM\…`, `HKCU\…`). */
  path: string;
  found: boolean;
}
export interface DiagSkipped {
  title: string;
  reason: string;
}
export interface DiagSource {
  /** Plateforme, ou `manual` pour les jeux ajoutés à la main. */
  id: Platform | "manual";
  label: string;
  state: DiagSourceState;
  /** Jeux trouvés ; écartés si le launcher est désactivé. */
  count: number;
  error: string | null;
  /** Durée de lecture au dernier scan. */
  millis: number | null;
  locations: DiagLocation[];
  skipped: DiagSkipped[];
  /** Applis masquées d'office. */
  tools: string[];
}
export type DiagServiceId = "images" | "art" | "details" | "hltb" | "saves" | "requirements" | "workshop";
export interface DiagService {
  id: DiagServiceId;
  label: string;
  host: string;
  busy: boolean;
  /** Secondes Unix, depuis le démarrage de Ludex. */
  lastOk: number | null;
  lastError: string | null;
  errorAt: number | null;
}
export interface Diagnostic {
  version: string;
  windows: string;
  webview: string | null;
  dataDir: string;
  scannedAt: number | null;
  created: number;
  sources: DiagSource[];
  services: DiagService[];
  /** Rapport texte anonymisé (nom d'utilisateur, compte Steam), prêt à copier. */
  report: string;
}
export const diagnostic = () => invoke<Diagnostic>("diagnostic");

/** Contenu d'un export de données (`transfer.rs`), en nombres d'éléments. */
export interface TransferSummary {
  favorites: number;
  hidden: number;
  statuses: number;
  notes: number;
  collections: number;
  manualGames: number;
  titles: number;
  aliases: number;
  genres: number;
  launchArgs: number;
  wishes: number;
  playtimeGames: number;
  sessions: number;
  covers: number;
}
/** Export écrit : nom du fichier (sans dossier) et contenu. */
export interface ExportInfo {
  file: string;
  summary: TransferSummary;
}
/** Archive choisie et vérifiée, en attente de confirmation. */
export interface ImportPreview {
  file: string;
  ludexVersion: string;
  /** Date de l'export en secondes Unix, 0 si inconnue. */
  created: number;
  summary: TransferSummary;
}
/** Import fait : bibliothèque relue, contenu importé, copie de sécurité faite avant. */
export interface ImportDone {
  scan: ScanResult;
  summary: TransferSummary;
  backup: string;
}
/** Fusion : les choix de ce PC gardés, ceux de l'archive ajoutés. Remplacement : ceux de l'archive. */
export type ImportMode = "merge" | "replace";
/** Sélecteur natif ouvert côté Rust ; `null` : annulé. */
export const exportData = () => invoke<ExportInfo | null>("export_data");
/** Sélecteur natif, archive lue et vérifiée côté Rust, rien d'écrit ; `null` : annulé. */
export const importData = () => invoke<ImportPreview | null>("import_data");
export const cancelImport = () => invoke<void>("cancel_import");
export const applyImport = (mode: ImportMode) => invoke<ImportDone>("apply_import", { mode });

/** Morceau de texte d'une annonce ; `link` : numéro d'un lien Steam, ouvert par `openNews`. */
export interface NewsSpan {
  text: string;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  link?: number;
}
/** Bloc d'une annonce, tiré du BBCode Steam côté Rust (jamais de HTML). */
export type NewsBlock =
  | { kind: "heading"; level: 1 | 2 | 3; spans: NewsSpan[] }
  | { kind: "paragraph"; spans: NewsSpan[] }
  | { kind: "list"; ordered: boolean; items: NewsSpan[][] }
  /** Image du CDN des annonces Steam, adresse vérifiée côté Rust. */
  | { kind: "image"; url: string }
  | { kind: "rule" };
/** Annonce officielle d'un jeu (Steam), dans la langue du studio. */
export interface News {
  gid: string;
  title: string;
  /** Publication, secondes Unix. */
  date: number;
  author: string | null;
  /** Notes de mise à jour. */
  update: boolean;
  image: string | null;
  excerpt: string;
  blocks: NewsBlock[];
}
/** Annonce récente d'un jeu « En cours », signalée une seule fois. */
export interface NewsAlert {
  id: string;
  game: string;
  title: string;
  gid: string;
  update: boolean;
}
/** Annonces du jeu (relues au plus toutes les 6 h) ; `seen` : l'onglet est affiché, plus de notification. */
export const gameNews = (id: string, seen: boolean) => invoke<News[]>("game_news", { id, seen });
/** Ouvre l'annonce sur Steam, ou son lien numéro `link` : l'adresse est reprise côté Rust. */
export const openNews = (id: string, gid: string, link: number | null) => invoke<void>("open_news", { id, gid, link });
export const onGameNews = (handler: (alerts: NewsAlert[]) => void): Promise<UnlistenFn> =>
  listen<NewsAlert[]>("game-news", (e) => handler(e.payload));

/** Disque proposé pour les données lourdes de Ludex (`relocate.rs`). */
export interface DiskChoice {
  /** Point de montage (« D:\ »), renvoyé tel quel pour choisir ce disque. */
  mount: string;
  /** « D: ». */
  letter: string;
  /** Nom du volume, vide s'il n'en a pas. */
  name: string;
  total: number;
  free: number;
  removable: boolean;
  /** Disque de Windows. */
  system: boolean;
  /** Disque des données lourdes aujourd'hui. */
  current: boolean;
  /** Dossier qui serait utilisé : « D:\Ludex ». */
  target: string;
}
/** Où sont les données de Ludex et combien elles pèsent (chemins en texte, affichage seulement). */
export interface DataLocation {
  /** Dossier choisi, vide = emplacement d'origine. */
  root: string;
  /** Dossier choisi introuvable (disque débranché ?). */
  missing: boolean;
  /** Cache des images et index de la base des sauvegardes. */
  cacheDir: string;
  cacheBytes: number;
  copiesDir: string;
  copiesBytes: number;
  /** Dossier des copies choisi à part : il ne bouge pas avec le reste. */
  copiesSeparate: boolean;
  /** Ce qui reste dans %APPDATA% (préférences, historique, petits caches). */
  keptDir: string;
  keptBytes: number;
  defaultCache: string;
  defaultCopies: string;
  disks: DiskChoice[];
  /** Dossier choisi avec le sélecteur, en attente de confirmation. */
  picked: string | null;
  /** Aperçu de l'écran d'accueil : rien n'est déplacé. */
  preview: boolean;
}
/** Destination d'un déplacement : jamais un chemin libre. */
export type DataTarget = { kind: "default" } | { kind: "drive"; mount: string } | { kind: "picked" };
export interface MoveDone {
  scan: ScanResult;
  location: DataLocation;
  /** Octets déplacés. */
  moved: number;
  /** Anciens dossiers pas entièrement supprimés. */
  warnings: string[];
}
export const dataLocation = () => invoke<DataLocation>("data_location");
/** Sélecteur de dossier natif côté Rust ; renvoie le dossier qui serait utilisé (`<choisi>\Ludex`), `null` si annulé. */
export const chooseDataDir = () => invoke<string | null>("choose_data_dir");
/** Copie, vérifie, puis supprime l'ancien ; refusé pendant une partie. Bibliothèque relue. */
export const moveData = (target: DataTarget) => invoke<MoveDone>("move_data", { target });

/** Prix de la boutique Steam, en centimes (`wishlist.rs`). */
export interface WishPrice {
  currency: string;
  initial: number;
  final: number;
  /** Remise en %, 0 sans promo. */
  discount: number;
}
/** Jeu de la liste de souhaits. */
export interface WishItem {
  appid: number;
  title: string;
  /** Date d'ajout, secondes Unix. */
  added: number;
  /** Prix vu à l'ajout. */
  addedPrice: WishPrice | null;
  /** Dernier prix connu. */
  price: WishPrice | null;
  /** `price` : en vente ; `noPrice` : gratuit ou pas encore en vente ; `unavailable` : retiré ou pas vendu en France ; `unknown` : jamais relu. */
  offer: "price" | "noPrice" | "unavailable" | "unknown";
  /** Plus bas prix vu, seulement s'il est sous le prix actuel. */
  lowest: WishPrice | null;
  checked: number | null;
  imageUrl: string;
  /** Id du jeu dans la bibliothèque : « Déjà à toi ». */
  owned: string | null;
}
export interface Wishlist {
  items: WishItem[];
  /** Dernière relecture complète des prix. */
  checked: number | null;
}
/** Résultat de la recherche de la boutique Steam. */
export interface WishHit {
  appid: number;
  title: string;
  price: WishPrice | null;
  imageUrl: string;
  owned: string | null;
  /** Déjà dans la liste. */
  wished: boolean;
}
/** Promo nouvelle d'un jeu de la liste. */
export interface WishSale {
  appid: number;
  title: string;
  price: WishPrice;
}
export const wishlist = () => invoke<Wishlist>("wishlist");
/** Recherche dans la boutique Steam : seul le texte part, les résultats restent connus de Rust. */
export const wishlistSearch = (text: string) => invoke<WishHit[]>("wishlist_search", { text });
/** Ajoute un résultat de la dernière recherche, ou remet un jeu retiré (« Annuler »). */
export const wishlistAdd = (appid: number) => invoke<Wishlist>("wishlist_add", { appid });
export const wishlistRemove = (appid: number) => invoke<Wishlist>("wishlist_remove", { appid });
/** Page du jeu dans Steam (`steam://store/<appid>` reconstruit côté Rust). */
export const openStorePage = (appid: number) => invoke<void>("open_store_page", { appid });
export const onWishlistChanged = (handler: (list: Wishlist) => void): Promise<UnlistenFn> =>
  listen<Wishlist>("wishlist-changed", (e) => handler(e.payload));
export const onWishlistSale = (handler: (sales: WishSale[]) => void): Promise<UnlistenFn> =>
  listen<WishSale[]>("wishlist-sale", (e) => handler(e.payload));
