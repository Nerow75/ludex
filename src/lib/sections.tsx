import type { ReactNode } from "react";
import type { CollectionsInfo, Game, Mood, Status } from "./api";
import { t } from "./i18n";
import { titleOrder, type Filter } from "./library";
import { HistoryList } from "../components/HistoryList";
import { HomeView } from "../components/HomeView";
import { ProgressBoard } from "../components/ProgressBoard";
import { BoardIcon, ChartIcon, ClockIcon, DownloadIcon, GridIcon, HeartIcon, HomeIcon, ImageIcon, JournalIcon, PieIcon, SaveIcon, StackIcon, StarIcon, StorageIcon, TagIcon, TrophyIcon } from "../components/Icons";
import { JournalView } from "../components/JournalView";
import { SavesPage } from "../components/SavesPage";
import { ScreenshotsPage } from "../components/ScreenshotsPage";
import { RecapView } from "../components/RecapView";
import { ShowcaseView } from "../components/ShowcaseView";
import { StatsView } from "../components/StatsView";
import { TagsView } from "../components/TagsView";
import { StorageView } from "../components/StorageView";
import { WishlistView } from "../components/WishlistView";
import { NotInstalledView } from "../components/NotInstalledView";

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
  /** Toast de 5 s avec « Annuler » (le même que pour un favori ou un statut). */
  showUndo: (title: string, undo: () => void) => void;
  /** Ouvre les Paramètres (compte GOG à connecter). */
  openSettings: () => void;
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

/**
 * Catégories repliables de la barre, pour ne pas montrer quinze entrées d'un coup. Ces noms
 * français sont des identifiants (`Section.group`, catégories ouvertes gardées dans
 * `ludex.sideGroups`) : la barre latérale les traduit à l'affichage, ne pas les traduire ici.
 */
export const ACTIVITY_GROUP = "Activité";
export const MEMORIES_GROUP = "Succès et captures";
export const TOOLS_GROUP = "Outils";
export const MINE_GROUP = "Mes collections";
export const STEAM_GROUP = "Collections Steam";
/** Catégories ouvertes au premier lancement : les collections perso, où l'on crée. */
export const OPEN_GROUPS_DEFAULT: readonly string[] = [MINE_GROUP];

/** Libellés des sections fixes, relus à chaque accès (langue actuelle). */
const text = () => t.common.sections;

/**
 * Sections fixes, dans l'ordre de la barre (et des raccourcis Ctrl+1, Ctrl+2…) : quatre entrées
 * toujours visibles, puis les catégories. Nouvelle section : une entrée ici, à côté de sa catégorie.
 * Libellés et textes en accesseurs : relus dans la langue actuelle, ids inchangés.
 */
export const SECTIONS: readonly Section[] = [
  // Accueil : bannière « Ce soir », Reprendre, idées par ambiance, pile des jeux pas commencés.
  {
    id: "for-you",
    get label() { return text().forYou; },
    icon: HomeIcon,
    render: (ctx) => <HomeView {...ctx} />,
  },
  {
    id: "all",
    get label() { return text().all; },
    icon: GridIcon,
    filters: (f) => f !== "favorites",
  },
  {
    id: "favorites",
    get label() { return text().favorites; },
    icon: () => <StarIcon />,
    scope: (g) => g.favorite,
    filters: launchersOnly,
    get empty() { return text().favoritesEmpty; },
  },
  {
    id: "tags",
    get label() { return text().tags; },
    icon: TagIcon,
    render: (ctx) => <TagsView {...ctx} />,
  },
  {
    id: "progress",
    get label() { return text().progress; },
    icon: BoardIcon,
    group: ACTIVITY_GROUP,
    render: (ctx) => <ProgressBoard {...ctx} />,
  },
  {
    id: "history",
    get label() { return text().history; },
    icon: ClockIcon,
    group: ACTIVITY_GROUP,
    render: (ctx) => <HistoryList {...ctx} />,
  },
  // Frise jour par jour : sessions, succès Steam et captures.
  {
    id: "journal",
    get label() { return text().journal; },
    icon: JournalIcon,
    group: ACTIVITY_GROUP,
    render: (ctx) => <JournalView {...ctx} />,
  },
  {
    id: "recap",
    get label() { return text().recap; },
    icon: ChartIcon,
    group: ACTIVITY_GROUP,
    render: (ctx) => <RecapView {...ctx} />,
  },
  // Heures par mois, launchers, genres, top 10, séries et succès par mois.
  {
    id: "stats",
    get label() { return text().stats; },
    icon: PieIcon,
    group: ACTIVITY_GROUP,
    render: (ctx) => <StatsView {...ctx} />,
  },
  // Succès Steam : total, jeux à 100 %, les plus rares, derniers déblocages.
  {
    id: "showcase",
    get label() { return text().showcase; },
    icon: TrophyIcon,
    group: MEMORIES_GROUP,
    render: (ctx) => <ShowcaseView {...ctx} />,
  },
  // Captures Steam et Xbox Game Bar, jeu par jeu.
  {
    id: "screenshots",
    get label() { return text().screenshots; },
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
  {
    id: "saves",
    get label() { return text().saves; },
    icon: SaveIcon,
    group: TOOLS_GROUP,
    render: () => <SavesPage />,
  },
  {
    id: "storage",
    get label() { return text().storage; },
    icon: StorageIcon,
    group: TOOLS_GROUP,
    render: (ctx) => <StorageView {...ctx} />,
  },
  // Liste de souhaits locale : jeux de la boutique Steam suivis, alertes de promo.
  {
    id: "wishlist",
    get label() { return text().wishlist; },
    icon: HeartIcon,
    group: TOOLS_GROUP,
    render: (ctx) => <WishlistView {...ctx} />,
  },
  // Jeux possédés (GOG, Epic) pas installés sur ce PC (comptes facultatifs, voir les Paramètres).
  {
    id: "gog",
    get label() { return text().gog; },
    icon: DownloadIcon,
    group: TOOLS_GROUP,
    render: (ctx) => <NotInstalledView {...ctx} />,
  },
];

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

const byName = (a: { name: string }, b: { name: string }) => titleOrder().compare(a.name, b.name);

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
      get empty() { return text().mineEmpty; },
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
        get empty() { return text().steamEmpty; },
      };
    });
  return [...mine, ...steam];
}
