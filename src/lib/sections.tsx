import type { ReactNode } from "react";
import type { CollectionsInfo, Game, Mood, Status } from "./api";
import type { Filter } from "./library";
import { HistoryList } from "../components/HistoryList";
import { HomeView } from "../components/HomeView";
import { ProgressBoard } from "../components/ProgressBoard";
import { BoardIcon, ChartIcon, ClockIcon, GridIcon, HomeIcon, ImageIcon, JournalIcon, SaveIcon, StackIcon, StarIcon, StorageIcon, TagIcon, TrophyIcon } from "../components/Icons";
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
  /** « Tous les jeux » restreint à un genre (page Explorer). */
  showGenre: (genre: string) => void;
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

/** Catégories repliables de la barre, pour ne pas montrer quinze entrées d'un coup. */
export const ACTIVITY_GROUP = "Activité";
export const MEMORIES_GROUP = "Succès et captures";
export const TOOLS_GROUP = "Outils";

/**
 * Sections fixes, dans l'ordre de la barre (et des raccourcis Ctrl+1, Ctrl+2…) : quatre entrées
 * toujours visibles, puis les catégories. Nouvelle section : une entrée ici, à côté de sa catégorie.
 */
export const SECTIONS: readonly Section[] = [
  // Accueil : bannière « Ce soir », Reprendre, idées par ambiance, pile des jeux pas commencés.
  { id: "for-you", label: "Pour toi", icon: HomeIcon, render: (ctx) => <HomeView {...ctx} /> },
  { id: "all", label: "Tous les jeux", icon: GridIcon, filters: (f) => f !== "favorites" },
  {
    id: "favorites",
    label: "Favoris",
    icon: () => <StarIcon />,
    scope: (g) => g.favorite,
    filters: launchersOnly,
    empty: "Ouvre la fiche d'un jeu et clique l'étoile, ou appuie sur Ctrl+D quand il est sélectionné",
  },
  { id: "tags", label: "Explorer", icon: TagIcon, render: (ctx) => <TagsView {...ctx} /> },
  { id: "progress", label: "Progression", icon: BoardIcon, group: ACTIVITY_GROUP, render: (ctx) => <ProgressBoard {...ctx} /> },
  { id: "history", label: "Historique", icon: ClockIcon, group: ACTIVITY_GROUP, render: (ctx) => <HistoryList {...ctx} /> },
  // Frise jour par jour : sessions, succès Steam et captures.
  { id: "journal", label: "Journal", icon: JournalIcon, group: ACTIVITY_GROUP, render: (ctx) => <JournalView {...ctx} /> },
  { id: "recap", label: "Récap", icon: ChartIcon, group: ACTIVITY_GROUP, render: (ctx) => <RecapView {...ctx} /> },
  // Succès Steam : total, jeux à 100 %, les plus rares, derniers déblocages.
  { id: "showcase", label: "Vitrine", icon: TrophyIcon, group: MEMORIES_GROUP, render: (ctx) => <ShowcaseView {...ctx} /> },
  // Captures Steam et Xbox Game Bar, jeu par jeu.
  {
    id: "screenshots",
    label: "Captures",
    icon: ImageIcon,
    group: MEMORIES_GROUP,
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
  { id: "saves", label: "Sauvegardes", icon: SaveIcon, group: TOOLS_GROUP, render: () => <SavesPage /> },
  { id: "storage", label: "Stockage", icon: StorageIcon, group: TOOLS_GROUP, render: (ctx) => <StorageView {...ctx} /> },
];

/** Catégories ouvertes au premier lancement : les collections perso, où l'on crée. */
export const OPEN_GROUPS_DEFAULT: readonly string[] = ["Mes collections"];

/** Ordre des catégories dans la barre : celles des sections fixes, puis les collections. */
export function sideGroups(sections: readonly Section[]): string[] {
  const fixed = [...new Set(sections.filter((s) => !s.collection && s.group).map((s) => s.group!))];
  const rest = [...new Set(sections.filter((s) => s.collection).map((s) => s.group!))].filter((g) => g !== MINE_GROUP);
  return [...fixed, MINE_GROUP, ...rest];
}

/**
 * Une catégorie est dépliée si l'utilisateur l'a ouverte, ou si elle contient la section affichée
 * (on voit toujours où l'on est).
 */
export function groupOpen(group: string, opened: ReadonlySet<string>, sections: readonly Section[], active: string): boolean {
  return opened.has(group) || sections.some((s) => s.id === active && s.group === group);
}

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
