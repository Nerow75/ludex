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
export const launchGame = (id: string) => invoke<void>("launch_game", { id });
export const openInstallDir = (id: string) => invoke<void>("open_install_dir", { id });
