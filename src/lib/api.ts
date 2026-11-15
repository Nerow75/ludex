import { invoke } from "@tauri-apps/api/core";

export type Platform = "steam" | "epic" | "riot";

export interface Game {
  id: string;
  title: string;
  platform: Platform;
  platformId: string;
  installDir: string | null;
  coverUrl: string | null;
  heroUrl: string | null;
  logoUrl: string | null;
  /** Dernière session (secondes Unix), launcher ou Ludex. */
  lastPlayed: number | null;
  /** Taille sur le disque, en octets. */
  sizeOnDisk: number | null;
  /** Application plutôt qu'un jeu (masquée par défaut). */
  isTool: boolean;
  favorite: boolean;
  hidden: boolean;
  launchLabel: string;
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

export const PLATFORM_LABEL: Record<Platform, string> = {
  steam: "Steam",
  epic: "Epic Games",
  riot: "Riot",
};

export const scanLibrary = () => invoke<ScanResult>("scan_library");
/** Renvoie l'heure du lancement (secondes Unix). */
export const launchGame = (id: string) => invoke<number>("launch_game", { id });
export const setFavorite = (id: string, value: boolean) => invoke<void>("set_favorite", { id, value });
export const setHidden = (id: string, value: boolean) => invoke<void>("set_hidden", { id, value });
export const openInstallDir = (id: string) => invoke<void>("open_install_dir", { id });
