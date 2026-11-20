import type { ReactNode } from "react";
import type { CollectionsInfo, Game, Mood, Status } from "./api";
import type { Filter } from "./library";
import { HistoryList } from "../components/HistoryList";
import { HomeView } from "../components/HomeView";
import { ProgressBoard } from "../components/ProgressBoard";
import { BoardIcon, ChartIcon, ClockIcon, DriveIcon, GridIcon, HomeIcon, ImageIcon, JournalIcon, SaveIcon, StackIcon, StarIcon, StorageIcon, TagIcon, TrophyIcon } from "../components/Icons";
import { JournalView } from "../components/JournalView";
import { SavesPage } from "../components/SavesPage";
import { ScreenshotsPage } from "../components/ScreenshotsPage";
import { RecapView } from "../components/RecapView";
import { ShowcaseView } from "../components/ShowcaseView";
import { TagsView } from "../components/TagsView";
import { StorageView } from "../components/StorageView";

/** Ce que l'écran d'une section reçoit de l'application. À compléter quand une section en a besoin. */
export interface SectionContext {
  games: Game[];
  /** Premier affichage du démarrage : EA, Battle.net et « Autres » encore en lecture. */
  partial: boolean;
  /** Jeux dont une session est en cours. */
  playing: ReadonlySet<string>;
  /** Ouvre la fiche d'un jeu dans « Tous les jeux ». */
  openGame: (game: Game) => void;
  play: (game: Game) => void;
  /** Montre les jeux d'un tag Steam, ou d'une ambiance, dans « Tous les jeux ». */
  showTag: (tag: string) => void;
  showMood: (mood: Mood) => void;
  /** Change le statut de progression (`null` : aucun), avec « Annuler » dans le toast. */
  setStatus: (game: Game, status: Status | null) => void;
  /** Journal restreint à un jeu (« Voir son journal » de la fiche), `null` : tout le journal. */
  journalGame: Game | null;
  /** Revient au journal complet. */
  showWholeJournal: () => void;
}

/**
 * Section de la barre latérale. Deux sortes :
 * - avec `render` : un écran à elle (Historique…). Ses éléments navigables au clavier et à la
 *   manette portent l'attribut `data-nav` (flèches haut/bas, gauche vers la barre) ; dans un
 *   conteneur `data-nav-2d` (« Pour toi »), les flèches suivent l'écran : rangées à gauche et à droite ;
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
  /** Titre sous lequel la barre range la section (« Mes collections »…), aucun pour les fixes. */
  group?: string;
  /** Section d'une collection : `mine` se renomme et se supprime, `steam` est en lecture seule. */
  collection?: { id: string; kind: "mine" | "steam" };
  /** Texte de la grille vide (sans recherche ni filtre). */
  empty?: string;
}

/** Filtres par launcher seulement : « Favoris » et « Masqués » n'ont pas de sens dans ces sections. */
const launchersOnly = (f: Filter) => f !== "favorites" && f !== "hidden";

/** Sections fixes, dans l'ordre de la barre (et des raccourcis Ctrl+1, Ctrl+2…). Nouvelle section : une entrée ici. */
export const SECTIONS: readonly Section[] = [
  // Accueil : bannière « Ce soir », Reprendre, idées par ambiance, pile des jeux pas commencés.
  { id: "for-you", label: "Pour toi", icon: HomeIcon, render: (ctx) => <HomeView {...ctx} /> },
  { id: "all", label: "Tous les jeux", icon: GridIcon, filters: (f) => f !== "favorites" },
  // Ludex ne liste que des jeux installés : ici, ceux qui se lancent sans mise à jour ni installation à finir.
  {
    id: "installed",
    label: "Installés",
    icon: DriveIcon,
    scope: (g) => g.installState.kind === "ready",
    filters: launchersOnly,
    empty: "Les jeux qui attendent une mise à jour ou une fin d'installation sont dans « Tous les jeux »",
  },
  {
    id: "favorites",
    label: "Favoris",
    icon: () => <StarIcon />,
    scope: (g) => g.favorite,
    filters: launchersOnly,
    empty: "Ouvre la fiche d'un jeu et clique l'étoile, ou appuie sur Ctrl+D quand il est sélectionné",
  },
  { id: "tags", label: "Tags", icon: TagIcon, render: (ctx) => <TagsView {...ctx} /> },
  { id: "history", label: "Historique", icon: ClockIcon, render: (ctx) => <HistoryList {...ctx} /> },
  { id: "progress", label: "Progression", icon: BoardIcon, render: (ctx) => <ProgressBoard {...ctx} /> },
  // Frise jour par jour : sessions, succès Steam et captures.
  { id: "journal", label: "Journal", icon: JournalIcon, render: (ctx) => <JournalView {...ctx} /> },
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

export const MINE_GROUP = "Mes collections";
export const STEAM_GROUP = "Collections Steam";

const byName = (a: { name: string }, b: { name: string }) => a.name.localeCompare(b.name, "fr", { sensitivity: "base" });

/**
 * Sections des collections, après les sections fixes (leurs raccourcis Ctrl+1… ne bougent donc
 * pas) : celles de l'utilisateur dans l'ordre de création, puis celles de Steam par nom, seulement
 * si au moins un de leurs jeux est dans la bibliothèque.
 */
export function collectionSections(info: CollectionsInfo | null, games: Game[]): Section[] {
  if (!info) return [];
  const mine = info.mine.map((c): Section => {
    const ids = new Set(c.games);
    return {
      id: `col:${c.id}`,
      label: c.name,
      icon: StackIcon,
      scope: (g) => ids.has(g.id),
      filters: launchersOnly,
      group: MINE_GROUP,
      collection: { id: c.id, kind: "mine" },
      empty: "Ouvre la fiche d'un jeu et choisis « Collections » pour l'ajouter ici",
    };
  });
  const present = new Set(games.filter((g) => !g.hidden).map((g) => g.id));
  const steam = [...info.steam]
    .filter((c) => c.games.some((id) => present.has(id)))
    .sort(byName)
    .map((c): Section => {
      const ids = new Set(c.games);
      return {
        id: `steam-col:${c.id}`,
        label: c.name,
        icon: TagIcon,
        scope: (g) => ids.has(g.id),
        filters: launchersOnly,
        group: STEAM_GROUP,
        collection: { id: c.id, kind: "steam" },
        empty: "Collection lue dans Steam : elle se modifie dans Steam",
      };
    });
  return [...mine, ...steam];
}
