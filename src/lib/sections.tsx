import type { ReactNode } from "react";
import type { Game } from "./api";
import type { Filter } from "./library";
import { HistoryList } from "../components/HistoryList";
import { ClockIcon, DriveIcon, GridIcon, HomeIcon, StarIcon, TrophyIcon } from "../components/Icons";
import { ShowcaseView } from "../components/ShowcaseView";

/** Ce que l'écran d'une section reçoit de l'application. À compléter quand une section en a besoin. */
export interface SectionContext {
  games: Game[];
  /** Jeux dont une session est en cours. */
  playing: ReadonlySet<string>;
  /** Ouvre la fiche d'un jeu dans « Tous les jeux ». */
  openGame: (game: Game) => void;
  play: (game: Game) => void;
}

/**
 * Section de la barre latérale. Deux sortes :
 * - avec `render` : un écran à elle (Historique…). Ses éléments navigables au clavier et à la
 *   manette portent l'attribut `data-nav` (flèches haut/bas, gauche vers la barre) ;
 * - sans `render` : la bibliothèque (grille, filtres, tri, fiche), restreinte par `scope`.
 * Taper une recherche affiche toujours les résultats, quelle que soit la section.
 */
export interface Section {
  id: string;
  label: string;
  icon: () => ReactNode;
  render?: (ctx: SectionContext) => ReactNode;
  /** Jeux de la section (tous par défaut). */
  scope?: (game: Game) => boolean;
  /** Filtres proposés au-dessus de la grille (tous par défaut). */
  filters?: (filter: Filter) => boolean;
  /** Rangée « Reprendre » au-dessus de la grille. */
  resume?: boolean;
}

/** Filtres par launcher seulement : « Favoris » et « Masqués » n'ont pas de sens dans ces sections. */
const launchersOnly = (f: Filter) => f !== "favorites" && f !== "hidden";

/** Ordre de la barre (et des raccourcis Ctrl+1, Ctrl+2…). Nouvelle section : une entrée ici. */
export const SECTIONS: readonly Section[] = [
  { id: "for-you", label: "Pour toi", icon: HomeIcon, resume: true },
  { id: "all", label: "Tous les jeux", icon: GridIcon, filters: (f) => f !== "favorites" },
  // Ludex ne liste que des jeux installés : ici, ceux qui se lancent sans mise à jour ni installation à finir.
  { id: "installed", label: "Installés", icon: DriveIcon, scope: (g) => g.installState.kind === "ready", filters: launchersOnly },
  { id: "favorites", label: "Favoris", icon: () => <StarIcon />, scope: (g) => g.favorite, filters: launchersOnly },
  { id: "history", label: "Historique", icon: ClockIcon, render: (ctx) => <HistoryList {...ctx} /> },
  // Succès Steam : total, jeux à 100 %, les plus rares, derniers déblocages.
  { id: "showcase", label: "Vitrine", icon: TrophyIcon, render: (ctx) => <ShowcaseView {...ctx} /> },
];

export const SECTION_IDS = SECTIONS.map((s) => s.id);
