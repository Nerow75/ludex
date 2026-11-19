import type { ReactNode } from "react";
import type { Game, Mood } from "./api";
import type { Filter } from "./library";
import { HistoryList } from "../components/HistoryList";
import { ChartIcon, ClockIcon, DriveIcon, GridIcon, HomeIcon, ImageIcon, SaveIcon, StarIcon, StorageIcon, TagIcon, TrophyIcon } from "../components/Icons";
import { SavesPage } from "../components/SavesPage";
import { ScreenshotsPage } from "../components/ScreenshotsPage";
import { RecapView } from "../components/RecapView";
import { ShowcaseView } from "../components/ShowcaseView";
import { TagsView } from "../components/TagsView";
import { StorageView } from "../components/StorageView";

/** Ce que l'écran d'une section reçoit de l'application. À compléter quand une section en a besoin. */
export interface SectionContext {
  games: Game[];
  /** Jeux dont une session est en cours. */
  playing: ReadonlySet<string>;
  /** Ouvre la fiche d'un jeu dans « Tous les jeux ». */
  openGame: (game: Game) => void;
  play: (game: Game) => void;
  /** Montre les jeux d'un tag Steam, ou d'une ambiance, dans « Tous les jeux ». */
  showTag: (tag: string) => void;
  showMood: (mood: Mood) => void;
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
  { id: "tags", label: "Tags", icon: TagIcon, render: (ctx) => <TagsView {...ctx} /> },
  { id: "history", label: "Historique", icon: ClockIcon, render: (ctx) => <HistoryList {...ctx} /> },
  { id: "recap", label: "Récap", icon: ChartIcon, render: (ctx) => <RecapView {...ctx} /> },
  // Succès Steam : total, jeux à 100 %, les plus rares, derniers déblocages.
  { id: "showcase", label: "Vitrine", icon: TrophyIcon, render: (ctx) => <ShowcaseView {...ctx} /> },
  // Captures Steam et Xbox Game Bar, jeu par jeu.
  {
    id: "screenshots",
    label: "Captures",
    icon: ImageIcon,
    render: (ctx) => (
      <ScreenshotsPage
        onOpenGame={(id) => {
          const game = ctx.games.find((g) => g.id === id);
          if (game) ctx.openGame(game);
        }}
      />
    ),
  },
  // Copies de sécurité des sauvegardes (base Ludusavi).
  { id: "saves", label: "Sauvegardes", icon: SaveIcon, render: () => <SavesPage /> },
  { id: "storage", label: "Stockage", icon: StorageIcon, render: (ctx) => <StorageView {...ctx} /> },
];

export const SECTION_IDS = SECTIONS.map((s) => s.id);
