import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  availableUpdate,
  addGame,
  changeExecutable,
  createCollection,
  deleteCollection,
  chooseCover,
  gameSessions,
  getCollections,
  installUpdate,
  launchGame,
  onGameArt,
  onGameDetails,
  onGameSize,
  onGameHltb,
  onGameSession,
  onLibraryChanged,
  onUpdateAvailable,
  onTrayHint,
  onTrayLaunch,
  openInstallDir,
  MOOD_LABEL,
  PLATFORM_LABEL,
  PLATFORMS,
  MAX_COLLECTION_NAME,
  removeGame,
  renameCollection,
  renameGame,
  resetCover,
  scanLibrary,
  setFavorite,
  setHidden,
  setInCollection,
  setNote,
  setStatus,
  STATUS_LABEL,
  type Collection,
  type CollectionsInfo,
  type ArtKind,
  type Game,
  type LibraryEvent,
  type LibraryUpdate,
  type Mood,
  type ScanResult,
  type SessionEvent,
  type Status,
  type UpdateInfo,
  type WelcomeInfo,
  welcome as getWelcome,
  welcomeDone,
} from "./lib/api";
import { ambientFromImage, titleColor } from "./lib/color";
import { useGamepad, type PadAction } from "./lib/gamepad";
import {
  hasFacets,
  LAYOUTS,
  libraryGenres,
  libraryMoods,
  libraryTags,
  matchesFacets,
  matchesFilter,
  matchesStatus,
  NO_FACETS,
  recentGames,
  remember,
  rememberFacets,
  remembered,
  rememberedFacets,
  rememberedText,
  selectGames,
  SORT_LABEL,
  sortMeta,
  SORTS,
  STATUS_FILTER_LABEL,
  STATUS_FILTERS,
  type Facets,
  type Filter,
  type Layout,
  type Sort,
  type StatusFilter,
} from "./lib/library";
import { collectionSections, SECTIONS, type Section, type SectionContext } from "./lib/sections";
import { gameMenu, type MenuAction, type Point } from "./lib/contextMenu";
import { ContextMenu } from "./components/ContextMenu";
import { ficheNext, GameStrip, type FicheEdit } from "./components/GameStrip";
import { Sidebar } from "./components/Sidebar";
import { LibraryGrid } from "./components/LibraryGrid";
import { LibraryList } from "./components/LibraryList";
import { SHIFT_EVENT } from "./components/ProgressBoard";
import { spatialNext } from "./lib/spatial";
import { InlineInput } from "./components/InlineInput";
import { DownloadIcon, GridIcon, ListIcon, LudexMark, PencilIcon, PlusIcon, RefreshIcon, SearchIcon, SlidersIcon, TrashIcon } from "./components/Icons";
import { SettingsView } from "./components/SettingsView";
import { EmptyState, LibrarySkeleton } from "./components/States";
import WelcomeScreen from "./components/WelcomeScreen";
import { WindowControls } from "./components/WindowControls";

interface Toast {
  title: string;
  detail?: string;
  error?: boolean;
  spin?: boolean;
  action?: { label: string; run: () => void };
}

/** Durée du « Annuler » des actions réversibles (masquer, favori, statut, collection). */
const UNDO_MS = 5000;

/** Menu contextuel ouvert sur un jeu ; `origin` reprend le focus à la fermeture. */
interface MenuState {
  gameId: string;
  at: Point;
  keyboard: boolean;
  origin: HTMLElement | null;
}

/** Manette pendant le menu contextuel : mêmes touches que le clavier. */
const PAD_MENU_KEYS: Partial<Record<PadAction, string>> = {
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
  confirm: "Enter",
  details: "Enter",
  back: "Escape",
  menu: "Escape",
};

/** Élément d'un jeu qui reçoit le focus : sa jaquette, sa tuile ou sa ligne. */
const focusTarget = (holder: HTMLElement) => holder.querySelector<HTMLElement>("button") ?? holder;

/** Section des Paramètres : hors de la liste (ni Ctrl+chiffre ni LB/RB), ouverte par Ctrl+, ou le bas de la barre. */
const SETTINGS_ID = "settings";

const FILTERS: readonly Filter[] = ["all", "favorites", ...PLATFORMS, "hidden"];
const FILTER_LABEL: Record<Filter, string> = { all: "Tous", favorites: "Favoris", hidden: "Masqués", ...PLATFORM_LABEL };

export default function App() {
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>(() => remembered("filter", FILTERS, "all"));
  const [sort, setSort] = useState<Sort>(() => remembered("sort", SORTS, "recent"));
  // Grille de jaquettes ou liste dense.
  const [layout, setLayout] = useState<Layout>(() => remembered("layout", LAYOUTS, "grid"));
  // Tag Steam, ambiance et genre choisis, en plus du filtre par launcher.
  const [facets, setFacets] = useState<Facets>(rememberedFacets);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>(() => remembered("status", STATUS_FILTERS, "any"));
  // Barre latérale : section affichée et barre repliée (icônes seules), gardées entre deux lancements.
  // Une section de collection est vérifiée à l'affichage (collection supprimée : première section).
  const [sectionId, setSectionId] = useState(() => rememberedText("section") ?? SECTIONS[0].id);
  /** Collections de l'utilisateur et de Steam, relues avec la bibliothèque. */
  const [collections, setCollections] = useState<CollectionsInfo | null>(null);
  const [collapsed, setCollapsed] = useState(() => remembered("sidebar", ["open", "collapsed"], "open") === "collapsed");
  /** Journal restreint à un jeu (« Voir son journal » de la fiche). Quitte la restriction en changeant de section. */
  const [journalGameId, setJournalGameId] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  // Menu contextuel : l'état pour l'affichage, une copie dans une ref pour les écouteurs globaux.
  const [menu, setMenu] = useState<MenuState | null>(null);
  const menuRef = useRef<MenuState | null>(null);
  /** Heure de la dernière ouverture au clavier : le `contextmenu` que la WebView envoie ensuite est ignoré. */
  const keyMenuAt = useRef(0);
  /** Champ de la fiche à ouvrir dès qu'elle est affichée (Renommer, Nouvelle collection… du menu). */
  const [ficheEdit, setFicheEdit] = useState<{ id: string; kind: FicheEdit } | null>(null);
  const [ambient, setAmbient] = useState<string | null>(null);
  /** Jeux lancés depuis Ludex dont un processus tourne. */
  const [playing, setPlaying] = useState<ReadonlySet<string>>(() => new Set());
  const toastTimer = useRef<number>(undefined);
  const searchRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLElement>(null);

  // Les applis masquées (Wallpaper Engine…) restent dans le scan mais n'apparaissent que dans « Masqués ».
  const games = scan?.games ?? [];
  // Sections fixes, puis une par collection (après : les raccourcis Ctrl+1… des fixes ne bougent pas).
  const sections = useMemo<Section[]>(() => [...SECTIONS, ...collectionSections(collections, games)], [collections, games]);
  // Rendu appelé plus bas, une fois l'état (mise à jour, écran d'accueil) déclaré.
  const settingsSection: Section = {
    id: SETTINGS_ID,
    label: "Paramètres",
    icon: SlidersIcon,
    render: () => (
      <SettingsView
        scan={scan}
        onScan={(result) => {
          setScan(result);
          loadCollections();
        }}
        onShown={(ids) => {
          const shown = new Set(ids);
          setScan((s) => s && { ...s, games: s.games.map((g) => (shown.has(g.id) ? { ...g, hidden: false } : g)) });
          if (ids.length) showToast({ title: ids.length > 1 ? `${ids.length} jeux sont de retour dans la bibliothèque` : "1 jeu est de retour dans la bibliothèque" });
        }}
        onWelcome={setWelcomeInfo}
        update={update}
        onInstallUpdate={runUpdate}
      />
    ),
  };
  // Section : un écran à elle (`render`, sauf pendant une recherche), sinon la grille restreinte à `scope`.
  const view = sectionId === SETTINGS_ID ? settingsSection : sections.find((s) => s.id === sectionId) ?? sections[0];
  const custom = !query && !!view.render;
  /** Paramètres affichés : toujours accessibles, même bibliothèque vide (tous les launchers désactivés). */
  const inSettings = custom && view.id === SETTINGS_ID;
  const scoped = useMemo(
    () => (view.scope ? games.filter(view.scope) : games).filter((g) => matchesStatus(g, statusFilter)),
    [games, view, statusFilter],
  );
  const activeFilter: Filter = view.filters && !view.filters(filter) ? "all" : filter;
  const results = useMemo(() => selectGames(scoped, activeFilter, sort, query, facets), [scoped, activeFilter, sort, query, facets]);
  const recent = useMemo(() => recentGames(games, 4), [games]);
  const selected =
    results.find((g) => g.id === selectedId) ??
    (query ? results[0] : results.find((g) => g.id === recent[0]?.id) ?? results[0]) ??
    null;

  const showToast = useCallback((t: Toast, ms = 3600) => {
    setToast(t);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), ms);
  }, []);

  /**
   * Toast d'une action réversible, d'où qu'elle vienne (fiche, menu contextuel, Ctrl+D, manette) :
   * « Annuler » pendant 5 s rejoue l'action en sens inverse, qui propose à son tour « Annuler ».
   */
  const showUndo = useCallback((title: string, undo: () => void) => {
    showToast({ title, action: { label: "Annuler", run: undo } }, UNDO_MS);
  }, [showToast]);

  const goTo = useCallback((id: string) => {
    setSectionId(id);
    setJournalGameId(null);
    setOpen(false);
    // Les Paramètres ne sont pas rouverts au lancement suivant.
    if (id !== SETTINGS_ID) remember("section", id);
    scrollRef.current?.scrollTo({ top: 0 });
  }, []);

  const toggleSidebar = useCallback(() => {
    setCollapsed((c) => {
      remember("sidebar", c ? "open" : "collapsed");
      return !c;
    });
  }, []);

  const changeFacets = useCallback((f: Facets) => {
    setFacets(f);
    setOpen(false);
    rememberFacets(f);
  }, []);

  /** Ouvre la fiche d'un jeu dans « Tous les jeux », avec un filtre qui le montre. */
  const openGame = useCallback((game: Game) => {
    goTo("all");
    setQuery("");
    setFilter((f) => (game.hidden ? "hidden" : f !== "favorites" && matchesFilter(game, f) ? f : "all"));
    setFacets((f) => {
      if (matchesFacets(game, f)) return f;
      rememberFacets(NO_FACETS);
      return NO_FACETS;
    });
    setStatusFilter((f) => (matchesStatus(game, f) ? f : "any"));
    setSelectedId(game.id);
    setOpen(true);
  }, [goTo]);

  /** Jeux d'un tag ou d'une ambiance (page Tags, fiche) : « Tous les jeux », tous launchers confondus. */
  const showFacets = useCallback((f: Facets) => {
    goTo("all");
    setQuery("");
    setFilter("all");
    remember("filter", "all");
    changeFacets(f);
    setSelectedId(null);
  }, [goTo, changeFacets]);
  const showTag = useCallback((tag: string) => showFacets({ ...NO_FACETS, tag }), [showFacets]);
  const showMood = useCallback((mood: Mood) => showFacets({ ...NO_FACETS, mood }), [showFacets]);
  /** Collections relues (celles de Steam ont pu changer) ; sans elles, la barre n'a que les sections fixes. */
  const loadCollections = useCallback(() => {
    getCollections()
      .then(setCollections)
      .catch(() => {});
  }, []);
  /** Journal d'un seul jeu, depuis sa fiche. */
  const openJournal = useCallback((game: Game) => {
    goTo("journal");
    setQuery("");
    setJournalGameId(game.id);
  }, [goTo]);

  /** `manual` : rescan demandé par l'utilisateur, confirmé par un toast. */
  const refresh = useCallback(async (manual = false) => {
    setScanning(true);
    setScanError(null);
    loadCollections();
    try {
      const result = await scanLibrary();
      setScan(result);
      if (manual) {
        const n = result.games.filter((g) => !g.hidden).length;
        showToast({ title: "Bibliothèque à jour", detail: `${n} jeu${n > 1 ? "x" : ""} détecté${n > 1 ? "s" : ""}` });
      }
    } catch (e) {
      setScanError(String(e));
    } finally {
      setScanning(false);
    }
  }, [showToast, loadCollections]);

  useEffect(() => void refresh(), [refresh]);

  // Couleur ambiante : teinte de la jaquette, sinon couleur dérivée du titre.
  useEffect(() => {
    if (!selected) return;
    let alive = true;
    setAmbient(titleColor(selected.title));
    const image = selected.coverUrl ?? selected.iconUrl;
    if (image) {
      ambientFromImage(image).then((c) => alive && c && setAmbient(c));
    }
    return () => {
      alive = false;
    };
  }, [selected?.id, selected?.coverUrl]);

  // Garde la fiche dépliée (ou la jaquette sélectionnée) visible.
  useEffect(() => {
    const target = scrollRef.current?.querySelector(open ? ".strip" : ".card.is-sel, .lrow.is-sel");
    target?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [selected?.id, open]);

  /** Met à jour un jeu dans le scan courant sans rescanner. */
  const patchGame = useCallback((id: string, patch: Partial<Game>) => {
    setScan((s) => s && { ...s, games: s.games.map((g) => (g.id === id ? { ...g, ...patch } : g)) });
  }, []);

  // Sessions suivies par Ludex : état « En jeu » et temps de jeu mis à jour pendant la partie.
  useEffect(() => {
    let alive = true;
    let rescan: number | undefined;
    const apply = (e: SessionEvent) => {
      setPlaying((current) => {
        if ((e.phase === "running") === current.has(e.id)) return current;
        const next = new Set(current);
        if (e.phase === "running") next.add(e.id);
        else next.delete(e.id);
        return next;
      });
      if (e.playtimeMinutes !== null) patchGame(e.id, { playtimeMinutes: e.playtimeMinutes, playtimeSource: e.playtimeSource });
      // Partie terminée : relit les launchers (temps de Steam, dernière session) sans prévenir.
      if (e.phase === "ended") {
        window.clearTimeout(rescan);
        rescan = window.setTimeout(() => alive && refresh(), 10_000);
      }
    };
    const unlisten = onGameSession(apply);
    gameSessions()
      .then((sessions) => alive && sessions.forEach(apply))
      .catch(() => {}); // sans état « En jeu », le reste de l'interface fonctionne
    return () => {
      alive = false;
      window.clearTimeout(rescan);
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [patchGame, refresh]);

  // Jaquettes retrouvées en arrière-plan après le scan : elles remplacent la jaquette générée.
  useEffect(() => {
    const unlisten = onGameArt(({ id, coverUrl, heroUrl, logoUrl, customArt }) => patchGame(id, { coverUrl, heroUrl, logoUrl, customArt }));
    return () => void unlisten.then((stop) => stop()).catch(() => {});
  }, [patchGame]);

  // Rescan fait tout seul par Ludex (jeu installé ou désinstallé) : bibliothèque remplacée sans
  // spinner, toast seulement si des jeux sont apparus ou ont disparu.
  useEffect(() => {
    const unlisten = onLibraryChanged((e) => {
      setScan(e.scan);
      setScanError(null);
      loadCollections();
      const t = libraryToast(e);
      if (t) showToast(t, 5000);
    });
    return () => void unlisten.then((stop) => stop()).catch(() => {});
  }, [showToast, loadCollections]);

  // Zone de notification : jeu lancé depuis son menu, et explication à la première fermeture
  // (la fenêtre se cache quelques secondes après).
  useEffect(() => {
    const launched = onTrayLaunch(({ id, title, lastPlayed, error }) => {
      if (lastPlayed !== null) patchGame(id, { lastPlayed });
      if (error) showToast({ title: `Impossible de lancer ${title}`, detail: error, error: true }, 6000);
    });
    const hint = onTrayHint(() =>
      showToast({ title: "Ludex reste ouvert dans la zone de notification", detail: "Le temps de jeu continue d'être mesuré. Clic droit sur l'icône, puis Quitter, pour le fermer." }, 3600),
    );
    return () => {
      launched.then((stop) => stop()).catch(() => {});
      hint.then((stop) => stop()).catch(() => {});
    };
  }, [patchGame, showToast]);

  // Fiches (genres, studio, sortie) reçues en arrière-plan après le scan.
  useEffect(() => {
    const unlisten = onGameDetails(({ id, details }) => patchGame(id, { details }));
    return () => void unlisten.then((stop) => stop()).catch(() => {});
  }, [patchGame]);

  // Tailles mesurées par Ludex (jeux sans taille du launcher) : tri « Taille » et section Stockage.
  useEffect(() => {
    const unlisten = onGameSize(({ id, sizeOnDisk }) => patchGame(id, { sizeOnDisk }));
    return () => void unlisten.then((stop) => stop()).catch(() => {});
  }, [patchGame]);

  // Durées de vie (HowLongToBeat) reçues en arrière-plan après le scan.
  useEffect(() => {
    const unlisten = onGameHltb(({ id, hltb }) => patchGame(id, { hltb }));
    return () => void unlisten.then((stop) => stop()).catch(() => {});
  }, [patchGame]);

  // Mise à jour de Ludex : toast à l'arrivée, puis bouton dans la barre du haut tant qu'elle attend.
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updating, setUpdating] = useState(false);
  const runUpdate = useCallback(async (info: UpdateInfo) => {
    setUpdating(true);
    showToast({ spin: true, title: `Mise à jour vers Ludex ${info.version}`, detail: "Téléchargement, puis Ludex se relance" }, 10 * 60_000);
    try {
      await installUpdate(); // sous Windows, Ludex se ferme ici
    } catch (e) {
      showToast({ title: "Mise à jour impossible", detail: String(e), error: true }, 6000);
    } finally {
      setUpdating(false);
    }
  }, [showToast]);
  useEffect(() => {
    let alive = true;
    const announce = (info: UpdateInfo) => {
      if (!alive) return;
      setUpdate(info);
      showToast({
        title: `Ludex ${info.version} est disponible`,
        detail: `Version actuelle : ${info.currentVersion}`,
        action: { label: "Installer et relancer", run: () => runUpdate(info) },
      }, 12_000);
    };
    const unlisten = onUpdateAvailable(announce);
    availableUpdate()
      .then((info) => alive && info && setUpdate(info))
      .catch(() => {}); // sans mise à jour connue, rien à afficher
    return () => {
      alive = false;
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [runUpdate, showToast]);

  // Premier lancement : écran montré une fois, après le premier scan. `null` : déjà vu.
  const [welcomeInfo, setWelcomeInfo] = useState<WelcomeInfo | null>(null);
  const welcomeOpen = useRef(false);
  useEffect(() => {
    getWelcome()
      .then(setWelcomeInfo)
      .catch(() => {}); // sans réponse, pas d'écran d'accueil : la bibliothèque reste utilisable
  }, []);

  const play = useCallback(async (game: Game) => {
    setSelectedId(game.id);
    showToast({ spin: true, title: `Lancement de ${game.title}`, detail: `via ${PLATFORM_LABEL[game.platform]} · ${game.launchLabel}` });
    try {
      patchGame(game.id, { lastPlayed: await launchGame(game.id) });
    } catch (e) {
      showToast({ title: `Impossible de lancer ${game.title}`, detail: String(e), error: true }, 6000);
    }
  }, [patchGame, showToast]);

  // Cliquer le jeu déjà ouvert replie sa fiche.
  const current = useRef({ id: selected?.id, open });
  current.current = { id: selected?.id, open };
  const select = useCallback((game: Game) => {
    const { id, open } = current.current;
    setSelectedId(game.id);
    setOpen(!(open && id === game.id));
  }, []);

  const toggleFavorite = useCallback(async (game: Game) => {
    const value = !game.favorite;
    patchGame(game.id, { favorite: value });
    try {
      await setFavorite(game.id, value);
      showUndo(value ? `${game.title} ajouté aux favoris` : `${game.title} retiré des favoris`, () => toggleFavorite({ ...game, favorite: value }));
    } catch (e) {
      patchGame(game.id, { favorite: !value });
      showToast({ title: "Favori non enregistré", detail: String(e), error: true }, 5000);
    }
  }, [patchGame, showToast, showUndo]);

  const toggleHidden = useCallback(async (game: Game) => {
    const value = !game.hidden;
    patchGame(game.id, { hidden: value });
    try {
      await setHidden(game.id, value);
      if (value) setOpen(false);
      showUndo(value ? `${game.title} est masqué` : `${game.title} est de retour dans la bibliothèque`, () => toggleHidden({ ...game, hidden: value }));
    } catch (e) {
      patchGame(game.id, { hidden: !value });
      showToast({ title: "Changement non enregistré", detail: String(e), error: true }, 5000);
    }
  }, [patchGame, showToast, showUndo]);

  /** Statut de progression, affiché tout de suite, puis « Annuler » pendant 5 s. */
  const changeStatus = useCallback(async (game: Game, status: Status | null) => {
    const before = game.status;
    if (before === status) return;
    patchGame(game.id, { status });
    try {
      await setStatus(game.id, status);
      showUndo(status ? `${game.title} : ${STATUS_LABEL[status]}` : `${game.title} : statut retiré`, () => changeStatus({ ...game, status }, before));
    } catch (e) {
      patchGame(game.id, { status: before });
      showToast({ title: "Statut non enregistré", detail: String(e), error: true }, 5000);
    }
  }, [patchGame, showToast, showUndo]);

  const changeNote = useCallback(async (game: Game, note: string) => {
    patchGame(game.id, { note: note.trim() || null });
    try {
      patchGame(game.id, { note: await setNote(game.id, note) });
    } catch (e) {
      patchGame(game.id, { note: game.note });
      showToast({ title: "Note non enregistrée", detail: String(e), error: true }, 5000);
    }
  }, [patchGame, showToast]);

  const setMine = (mine: Collection[]) => setCollections((c) => ({ steam: c?.steam ?? [], mine }));

  /** Ajoute un jeu à une collection ou l'en retire, puis « Annuler » pendant 5 s. */
  const toggleCollection = useCallback(async (game: Game, collection: Collection, value: boolean) => {
    try {
      setMine(await setInCollection(collection.id, game.id, value));
      showUndo(
        value ? `${game.title} ajouté à « ${collection.name} »` : `${game.title} retiré de « ${collection.name} »`,
        () => toggleCollection(game, collection, !value),
      );
    } catch (e) {
      showToast({ title: "Collection non modifiée", detail: String(e), error: true }, 5000);
    }
  }, [showToast, showUndo]);

  /** Nouvelle collection : depuis une fiche (le jeu y entre), ou depuis la barre (on l'ouvre). */
  const newCollection = useCallback(async (name: string, game: Game | null) => {
    try {
      const { id, mine } = await createCollection(name, game?.id ?? null);
      setMine(mine);
      const created = mine.find((c) => c.id === id);
      if (game && created) {
        showUndo(`${game.title} ajouté à « ${created.name} »`, () => toggleCollection(game, created, false));
      } else if (game) {
        showToast({ title: `${game.title} ajouté à « ${name} »` });
      } else {
        showToast({ title: `Collection « ${created?.name ?? name} » créée`, detail: "Ajoute des jeux depuis leur fiche, bouton Collections" });
        goTo(`col:${id}`);
      }
    } catch (e) {
      showToast({ title: "Collection non créée", detail: String(e), error: true }, 5000);
    }
  }, [goTo, showToast, showUndo, toggleCollection]);

  const [renamingCollection, setRenamingCollection] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => {
    setRenamingCollection(false);
    setConfirmDelete(false);
  }, [sectionId]);

  const renameCol = useCallback(async (id: string, name: string) => {
    try {
      setMine(await renameCollection(id, name));
    } catch (e) {
      showToast({ title: "Collection non renommée", detail: String(e), error: true }, 5000);
    }
  }, [showToast]);

  const deleteCol = useCallback(async (id: string, name: string) => {
    try {
      setMine(await deleteCollection(id));
      goTo(SECTIONS[0].id);
      showToast({ title: `Collection « ${name} » supprimée`, detail: "Ses jeux restent dans la bibliothèque" });
    } catch (e) {
      showToast({ title: "Collection non supprimée", detail: String(e), error: true }, 5000);
    }
  }, [goTo, showToast]);

  const openFolder = useCallback(async (game: Game) => {
    try {
      await openInstallDir(game.id);
    } catch (e) {
      showToast({ title: "Dossier introuvable", detail: String(e), error: true }, 5000);
    }
  }, [showToast]);

  /** Bibliothèque relue par une commande (ajout, correction) : remplace le scan et sélectionne le jeu. */
  const applyUpdate = useCallback((u: LibraryUpdate) => {
    setScan(u.scan);
    setScanError(null);
    setSelectedId(u.id);
  }, []);

  // Le sélecteur de fichier est ouvert côté Rust : un second Ctrl+N pendant ce temps ne fait rien.
  const adding = useRef(false);
  const addManualGame = useCallback(async () => {
    if (adding.current) return;
    adding.current = true;
    try {
      const u = await addGame();
      if (!u) return; // sélection annulée
      applyUpdate(u);
      // Le nouveau jeu doit être visible : ni filtre qui l'exclut, ni recherche (ni tag : il n'a pas encore de fiche).
      setFilter((f) => (f === "all" || f === "other" ? f : "all"));
      changeFacets(NO_FACETS);
      setStatusFilter((f) => (f === "none" ? f : "any"));
      goTo(SECTIONS[0].id);
      setQuery("");
      setOpen(true);
      showToast({ title: `${u.title} ajouté`, detail: "Rangé dans « Autres »" });
    } catch (e) {
      showToast({ title: "Jeu non ajouté", detail: String(e), error: true }, 6000);
    } finally {
      adding.current = false;
    }
  }, [applyUpdate, goTo, changeFacets, showToast]);

  const changeExe = useCallback(async (game: Game) => {
    try {
      const u = await changeExecutable(game.id);
      if (!u) return;
      applyUpdate(u);
      const label = u.scan.games.find((g) => g.id === u.id)?.launchLabel;
      showToast({ title: `Exécutable de ${u.title} changé`, detail: label });
    } catch (e) {
      showToast({ title: "Exécutable non changé", detail: String(e), error: true }, 6000);
    }
  }, [applyUpdate, showToast]);

  const rename = useCallback(async (game: Game, title: string) => {
    // Affiché tout de suite ; le titre nettoyé par Ludex arrive avec la bibliothèque relue.
    if (title.trim()) patchGame(game.id, { title: title.trim() });
    try {
      const u = await renameGame(game.id, title);
      applyUpdate(u);
      showToast({ title: `Renommé en ${u.title}` });
    } catch (e) {
      patchGame(game.id, { title: game.title });
      showToast({ title: "Titre non enregistré", detail: String(e), error: true }, 5000);
    }
  }, [applyUpdate, patchGame, showToast]);

  // Le sélecteur d'image est ouvert côté Rust : un second clic pendant ce temps ne fait rien.
  const choosingArt = useRef(false);
  const chooseArt = useCallback(async (game: Game, kind: ArtKind) => {
    if (choosingArt.current) return;
    choosingArt.current = true;
    const what = kind === "cover" ? "Jaquette" : "Image de fond";
    try {
      const e = await chooseCover(game.id, kind);
      if (!e) return; // sélection annulée
      patchGame(e.id, { coverUrl: e.coverUrl, heroUrl: e.heroUrl, logoUrl: e.logoUrl, customArt: e.customArt });
      showToast({ title: `${what} de ${game.title} changée`, detail: "Copiée dans Ludex : l'image d'origine peut être déplacée" });
    } catch (err) {
      showToast({ title: `${what} non changée`, detail: String(err), error: true }, 6000);
    } finally {
      choosingArt.current = false;
    }
  }, [patchGame, showToast]);

  const resetArt = useCallback(async (game: Game) => {
    try {
      for (const kind of game.customArt) {
        const e = await resetCover(game.id, kind);
        patchGame(e.id, { coverUrl: e.coverUrl, heroUrl: e.heroUrl, logoUrl: e.logoUrl, customArt: e.customArt });
      }
      showToast({ title: `Images d'origine de ${game.title} rétablies` });
    } catch (err) {
      showToast({ title: "Images d'origine non rétablies", detail: String(err), error: true }, 5000);
    }
  }, [patchGame, showToast]);

  const remove = useCallback(async (game: Game) => {
    try {
      setScan(await removeGame(game.id));
      setOpen(false);
      showToast({ title: `${game.title} retiré`, detail: "Ajouté de nouveau, il retrouvera son temps de jeu" });
    } catch (e) {
      showToast({ title: "Jeu non retiré", detail: String(e), error: true }, 5000);
    }
  }, [showToast]);

  /** Focus sur la jaquette (grille) ou la ligne (liste) d'un jeu de la bibliothèque. */
  const focusCard = (id: string | undefined) =>
    requestAnimationFrame(() =>
      scrollRef.current?.querySelector<HTMLElement>(`[data-game="${id}"] .card__hit, [data-game="${id}"] .lrow__hit`)?.focus({ preventScroll: true }),
    );

  // Raccourcis globaux. L'état courant passe par une ref pour n'abonner l'écouteur qu'une fois.
  const sectionIds = sections.map((s) => s.id);
  const live = useRef({ results, selected, open, query, facets, changeFacets, play, toggleFavorite, refresh, addManualGame, custom, sectionId: view.id, sectionIds, goTo, toggleSidebar, games, layout });
  live.current = { results, selected, open, query, facets, changeFacets, play, toggleFavorite, refresh, addManualGame, custom, sectionId: view.id, sectionIds, goTo, toggleSidebar, games, layout };

  /** Ouvre la fiche d'un jeu : sur place s'il est dans la grille affichée, sinon dans « Tous les jeux ». */
  const showFiche = (game: Game, edit?: FicheEdit) => {
    const s = live.current;
    if (!s.custom && s.results.some((g) => g.id === game.id)) {
      setSelectedId(game.id);
      setOpen(true);
    } else {
      openGame(game);
    }
    setFicheEdit(edit ? { id: game.id, kind: edit } : null);
  };

  const openMenu = (m: MenuState) => {
    if (!live.current.games.some((g) => g.id === m.gameId)) return;
    menuRef.current = m;
    setMenu(m);
  };
  /** `restore` : le focus revient à la jaquette, la tuile ou la ligne d'où le menu est parti. */
  const closeMenu = useCallback((restore: boolean) => {
    const m = menuRef.current;
    menuRef.current = null;
    setMenu(null);
    if (restore && m?.origin?.isConnected) m.origin.focus({ preventScroll: true });
  }, []);

  /**
   * Touche Menu, Maj+F10 ou bouton Menu de la manette : menu du jeu qui a le focus (jaquette, tuile,
   * ligne de l'historique), sinon du jeu sélectionné dans la grille, ouvert au milieu de son élément.
   */
  const menuFromFocus = () => {
    const s = live.current;
    const focused = document.activeElement?.closest<HTMLElement>("[data-game]");
    const holder = focused && scrollRef.current?.contains(focused) ? focused : null;
    const id = holder?.dataset.game ?? (s.custom ? undefined : s.selected?.id);
    if (!id) return;
    const el = holder ?? scrollRef.current?.querySelector<HTMLElement>(`[data-game="${CSS.escape(id)}"]`) ?? null;
    const r = el?.getBoundingClientRect();
    const at = r ? { x: r.left + r.width / 2, y: r.top + Math.min(r.height / 2, 120) } : { x: window.innerWidth / 2, y: window.innerHeight / 2 };
    keyMenuAt.current = Date.now();
    openMenu({ gameId: id, at, keyboard: true, origin: el && focusTarget(el) });
  };

  const runMenu = (game: Game, a: MenuAction) => {
    switch (a.kind) {
      case "play":
        return void play(game);
      case "open":
        return showFiche(game);
      case "favorite":
        return void toggleFavorite(game);
      case "status":
        return void changeStatus(game, a.status);
      case "collection": {
        const c = collections?.mine.find((c) => c.id === a.id);
        return void (c && toggleCollection(game, c, a.value));
      }
      case "newCollection":
        return showFiche(game, "collection");
      case "folder":
        return void openFolder(game);
      case "rename":
        return showFiche(game, "rename");
      case "hide":
        return void toggleHidden(game);
    }
  };
  const menuGame = menu ? games.find((g) => g.id === menu.gameId) ?? null : null;
  const menuEntries = useMemo(() => (menuGame ? gameMenu(menuGame, collections?.mine ?? []) : []), [menuGame, collections]);

  // Clic droit : menu maison sur une jaquette, une tuile ou une ligne de l'historique ; ailleurs, le
  // menu natif de la WebView (Recharger, Inspecter…) est coupé, sauf dans les champs de saisie
  // (copier, coller, correcteur) et, en développement, avec Maj pour garder « Inspecter ».
  useEffect(() => {
    const onContextMenu = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (target.closest("input, textarea, [contenteditable]:not([contenteditable='false'])")) return;
      if (import.meta.env.DEV && e.shiftKey) return;
      e.preventDefault();
      if (Date.now() - keyMenuAt.current < 1000) return; // déjà ouvert par la touche Menu ou Maj+F10
      const holder = target.closest<HTMLElement>("[data-game]");
      if (!holder?.dataset.game || !scrollRef.current?.contains(holder)) return;
      openMenu({ gameId: holder.dataset.game, at: { x: e.clientX, y: e.clientY }, keyboard: false, origin: focusTarget(holder) });
    };
    document.addEventListener("contextmenu", onContextMenu);
    return () => document.removeEventListener("contextmenu", onContextMenu);
  }, []);

  // Navigation par zones, au clavier comme à la manette : la barre latérale à gauche, le contenu à
  // droite. Gauche depuis la première colonne (ou depuis une section à elle) entre dans la barre ;
  // dans la barre, haut et bas changent de section, droite ou Entrée (A) reviennent au contenu.
  const sideItems = () => [...document.querySelectorAll<HTMLElement>(".side__item")];
  const inSidebar = () => !!document.activeElement?.closest(".side");
  const focusSidebar = () => sideItems().find((b) => b.getAttribute("aria-current") === "page")?.focus();
  const focusContent = () => {
    const s = live.current;
    if (s.custom) scrollRef.current?.querySelector<HTMLElement>("[data-nav]")?.focus();
    else if (s.selected) focusCard(s.selected.id);
    else (document.activeElement as HTMLElement | null)?.blur(); // grille vide : la barre rend la main
  };
  /** Section voisine (barre, LB/RB), en boucle ; le focus suit si la barre l'avait. */
  const stepSection = (delta: number) => {
    const s = live.current;
    // Les Paramètres ferment la boucle : dernier élément de la barre, joignables au clavier et à la manette.
    const ids = [...s.sectionIds, SETTINGS_ID];
    const i = Math.max(0, ids.indexOf(s.sectionId));
    const next = (i + delta + ids.length) % ids.length;
    const follow = inSidebar();
    s.goTo(ids[next]);
    if (follow) sideItems()[next]?.focus();
  };

  /** Flèche ou croix : barre latérale, élément `[data-nav]` d'une section à elle, ou grille. */
  const navigate = (dir: "up" | "down" | "left" | "right") => {
    const current = document.activeElement as HTMLElement | null;
    // Dans la fiche à onglets : onglet voisin, élément voisin, retour à la jaquette ou rangée suivante.
    const fiche = live.current.custom ? null : current?.closest<HTMLElement>(".strip");
    if (fiche && current) {
      const next = ficheNext(fiche, current, dir);
      if (next === "card") focusCard(live.current.selected?.id);
      else if (next === "out") moveSelection("down");
      else next?.focus();
      return;
    }
    if (inSidebar()) {
      if (dir === "right") focusContent();
      else if (dir !== "left") stepSection(dir === "down" ? 1 : -1);
    } else if (live.current.custom) {
      const items = [...(scrollRef.current?.querySelectorAll<HTMLElement>("[data-nav]") ?? [])];
      const current = document.activeElement as HTMLElement;
      const i = items.indexOf(current);
      // Écran en rangées (« Pour toi ») : l'élément voisin à l'écran ; à gauche du premier, la barre.
      if (i >= 0 && current.closest("[data-nav-2d]")) {
        const next = spatialNext(items, current, dir);
        if (next) next.focus();
        else if (dir === "left") focusSidebar();
      } else if (dir === "left") focusSidebar();
      else items[i < 0 ? 0 : Math.min(items.length - 1, Math.max(0, i + (dir === "up" ? -1 : 1)))]?.focus();
    } else {
      moveSelection(dir);
    }
  };

  /** Déplace la sélection dans la grille, comme les flèches du clavier. */
  const moveSelection = (dir: "up" | "down" | "left" | "right") => {
    const s = live.current;
    if (!s.results.length) return void (dir === "left" && focusSidebar());
    // Liste : une seule colonne, droite ne fait rien.
    if (s.layout === "list" && dir === "right") return;
    const cards = [...(scrollRef.current?.querySelectorAll<HTMLElement>(".grid .card") ?? [])];
    const cols = s.layout === "list" ? 1 : Math.max(1, cards.filter((c) => c.offsetTop === cards[0]?.offsetTop).length);
    const i = Math.max(0, s.results.findIndex((g) => g.id === s.selected?.id));
    if (dir === "left" && i % cols === 0) return focusSidebar();
    // Fiche ouverte sous la rangée : bas y entre (Jouer) avant la rangée suivante.
    const fromFiche = !!document.activeElement?.closest(".strip");
    const ficheStart = s.open && !fromFiche ? scrollRef.current?.querySelector<HTMLElement>(".strip [data-nav]") : null;
    if (dir === "down" && ficheStart) return void ficheStart.focus();
    const step = { left: -1, right: 1, up: -cols, down: cols }[dir];
    const next = s.results[Math.min(s.results.length - 1, Math.max(0, i + step))];
    setSelectedId(next.id);
    focusCard(next.id);
  };

  /** Échap ou B : replie la fiche, sinon efface la recherche, sinon retire le tag et l'ambiance. */
  const back = () => {
    const s = live.current;
    if (s.open) {
      // Le focus était dans la fiche : il revient sur la jaquette au lieu de se perdre.
      if (document.activeElement?.closest(".strip")) focusCard(s.selected?.id);
      setOpen(false);
    } else if (s.query) setQuery("");
    else if (!s.custom && hasFacets(s.facets)) s.changeFacets(NO_FACETS);
  };

  // Indices de la barre du bas : ceux de la manette après une action à la manette, ceux du clavier sinon.
  const [padHints, setPadHints] = useState(false);
  useEffect(() => {
    const toKeyboard = () => setPadHints(false);
    window.addEventListener("keydown", toKeyboard);
    window.addEventListener("pointerdown", toKeyboard);
    return () => {
      window.removeEventListener("keydown", toKeyboard);
      window.removeEventListener("pointerdown", toKeyboard);
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = live.current;
      const target = e.target as HTMLElement;
      const inInput = target instanceof HTMLInputElement;
      // Une section à elle (Historique…) n'a pas de jeu sélectionné visible : Entrée, Espace et Ctrl+D n'y agissent pas.
      const onGame = !s.custom && (target === document.body || target.matches(".card__hit, .tile__hit, .lrow__hit"));
      const onNav = target === document.body || target.matches(".card__hit, .tile__hit, .lrow__hit, .side__item, [data-nav]");
      const ctrl = e.ctrlKey || e.metaKey;
      const key = e.key;
      // Liste déroulante (tag, ambiance, genre, tri) : ses touches lui reviennent, sauf les raccourcis avec Ctrl et F5.
      if (target instanceof HTMLSelectElement && !ctrl && key !== "F5") return;

      if ((ctrl && key.toLowerCase() === "k") || (key === "/" && !inInput)) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      } else if (key === "F5") {
        e.preventDefault(); // sinon la WebView recharge la page
        s.refresh(true);
      } else if (ctrl && key >= "1" && key <= "9") {
        e.preventDefault(); // Ctrl+1, Ctrl+2… : section de la barre latérale
        const id = s.sectionIds[Number(key) - 1];
        if (id) s.goTo(id);
      } else if (ctrl && key.toLowerCase() === "b") {
        e.preventDefault();
        s.toggleSidebar();
      } else if (ctrl && key.toLowerCase() === "d") {
        e.preventDefault();
        if (s.selected && !s.custom) s.toggleFavorite(s.selected);
      } else if (ctrl && key.toLowerCase() === "n") {
        e.preventDefault(); // sinon la WebView ouvre une nouvelle fenêtre
        s.addManualGame();
      } else if (ctrl && key === ",") {
        e.preventDefault();
        s.goTo(SETTINGS_ID);
      } else if (inInput) {
        if (key === "ArrowDown" && s.selected) {
          e.preventDefault();
          setSelectedId(s.selected.id);
          focusCard(s.selected.id);
        }
      } else if (key === "ContextMenu" || (e.shiftKey && key === "F10")) {
        e.preventDefault(); // sinon le menu natif de la WebView
        menuFromFocus();
      } else if (key === "Escape") {
        back();
      } else if (key.length === 1 && key !== " " && !ctrl && !e.altKey) {
        // Taper n'importe où commence une recherche : le caractère arrive dans le champ.
        searchRef.current?.focus();
      } else if (key === "Enter" && target.matches(".side__item")) {
        // Entrée dans la barre : ouvre la section et passe à son contenu.
        e.preventDefault();
        target.click();
        requestAnimationFrame(focusContent);
      } else if (onGame && key === "Enter") {
        e.preventDefault();
        if (s.selected) s.play(s.selected);
      } else if (onGame && key === " ") {
        e.preventDefault();
        setOpen((o) => !o);
      } else if (onNav && key.startsWith("Arrow")) {
        e.preventDefault();
        navigate(key.slice(5).toLowerCase() as "up" | "down" | "left" | "right");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const onSearchKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" && selected) {
      e.preventDefault();
      play(selected);
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (query) setQuery("");
      else e.currentTarget.blur();
    }
  };

  const errors = scan?.sources.filter((s) => s.error) ?? [];
  const emptyLibrary = scan !== null && games.every((g) => g.isTool);
  const meta = useCallback((g: Game) => sortMeta(g, sort), [sort]);
  const count = (f: Filter) => selectGames(scoped, f, sort, "", facets).length;
  // Choix des listes Tag, Ambiance et Genre : jeux de la section et du filtre par launcher.
  const facetPool = useMemo(() => scoped.filter((g) => matchesFilter(g, activeFilter)), [scoped, activeFilter]);
  const tagChoices = useMemo(() => libraryTags(facetPool), [facetPool]);
  const genreChoices = useMemo(() => libraryGenres(facetPool), [facetPool]);
  const moodChoices = useMemo(() => libraryMoods(facetPool).filter((m) => m.count > 0 || m.mood === facets.mood), [facetPool, facets.mood]);
  // Un filtre par launcher qui a au moins un jeu (un launcher installé mais vide n'encombre pas la barre),
  // parmi ceux que la section propose.
  const filters = FILTERS.filter(
    (f) => (view.filters?.(f) ?? true) && (f === "all" || f === "favorites" || f === "hidden" || f === activeFilter || scoped.some((g) => g.platform === f)),
  );
  const changeFilter = (f: Filter) => {
    setFilter(f);
    setOpen(false);
    remember("filter", f);
  };
  const changeSort = (s: Sort) => {
    setSort(s);
    remember("sort", s);
  };
  const changeLayout = (l: Layout) => {
    setLayout(l);
    remember("layout", l);
  };
  /** Passe au filtre ou au tri voisin, en boucle. */
  const cycle = <T,>(list: readonly T[], current: T, delta: number) =>
    list[(Math.max(0, list.indexOf(current)) + delta + list.length) % list.length];

  const padConnected = useGamepad((action) => {
    setPadHints(true);
    const s = live.current;
    // Écran de premier lancement ou menu contextuel ouvert : la manette les pilote comme le clavier.
    if (welcomeOpen.current) {
      const key = PAD_MENU_KEYS[action];
      if (key) (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
      return;
    }
    if (menuRef.current) {
      const key = PAD_MENU_KEYS[action];
      const target = document.activeElement?.closest(".cmenu-layer") ? document.activeElement : document.querySelector(".cmenu");
      if (key) target?.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
      return;
    }
    switch (action) {
      case "menu":
        menuFromFocus();
        break;
      case "up":
      case "down":
      case "left":
      case "right":
        // La sélection quitte le champ de recherche pour la grille (ou la barre, ou la section).
        navigate(action);
        break;
      case "confirm":
      case "details":
        // Dans la barre : entrer dans la section. Dans une section à elle : l'élément choisi (fiche du jeu).
        if (inSidebar()) focusContent();
        else if (s.custom) (document.activeElement?.closest<HTMLElement>("[data-nav]") ?? null)?.click();
        // Dans la fiche, A agit sur l'élément choisi (onglet, statut, action) ; X la referme toujours.
        else if (action === "confirm" && document.activeElement?.closest(".strip [data-nav]")) (document.activeElement as HTMLElement).click();
        else if (action === "confirm" && s.selected) s.play(s.selected);
        else if (s.selected) setOpen((o) => !o);
        break;
      case "back":
        back();
        break;
      case "favorite":
        if (s.selected && !s.custom) s.toggleFavorite(s.selected);
        break;
      case "prevSection":
      case "nextSection":
        stepSection(action === "nextSection" ? 1 : -1);
        break;
      case "prevFilter":
      case "nextFilter":
        // Dans une section à elle (Progression…), LT/RT agissent sur l'élément qui a le focus.
        if (s.custom) document.activeElement?.dispatchEvent(new CustomEvent(SHIFT_EVENT, { detail: action === "nextFilter" ? 1 : -1, bubbles: true }));
        else changeFilter(cycle(filters, activeFilter, action === "nextFilter" ? 1 : -1));
        break;
      case "nextSort":
        if (!s.custom) changeSort(cycle(SORTS, sort, 1));
        break;
    }
  });
  const showPadHints = padConnected && padHints;
  const journalGame = (journalGameId && games.find((g) => g.id === journalGameId)) || null;
  const sectionContext: SectionContext = { games, playing, openGame, play, showTag, showMood, setStatus: changeStatus, journalGame, showWholeJournal: () => setJournalGameId(null) };
  const changeStatusFilter = (f: StatusFilter) => {
    setStatusFilter(f);
    setOpen(false);
    remember("status", f);
  };
  const mineCollection = view.collection?.kind === "mine" ? collections?.mine.find((c) => c.id === view.collection?.id) : undefined;

  /** Fiche dépliée sous la rangée (grille) ou la ligne (liste) du jeu sélectionné. */
  const renderStrip = (g: Game, notch: number) => (
    <GameStrip
      key={`strip-${g.id}`}
      game={g}
      notch={notch}
      playing={playing.has(g.id)}
      onPlay={play}
      onFavorite={toggleFavorite}
      onHide={toggleHidden}
      onOpenFolder={openFolder}
      onChangeExe={changeExe}
      onRename={rename}
      onChooseArt={chooseArt}
      onResetArt={resetArt}
      onRemove={remove}
      onTag={showTag}
      onMood={showMood}
      onJournal={openJournal}
      onClose={() => setOpen(false)}
      collections={collections?.mine ?? []}
      onStatus={changeStatus}
      onNote={changeNote}
      onToggleCollection={toggleCollection}
      onCreateCollection={(name, game) => newCollection(name, game)}
      edit={ficheEdit?.id === g.id ? ficheEdit.kind : null}
      onEditStarted={() => setFicheEdit(null)}
    />
  );

  const showWelcome = !!(scan && welcomeInfo);
  welcomeOpen.current = showWelcome;
  /** Écran de premier lancement fermé : il ne reviendra plus, puis `then` (section à ouvrir). */
  const closeWelcome = (then: () => void) => {
    setWelcomeInfo(null);
    welcomeDone().catch((e) => showToast({ title: "Premier lancement non enregistré", detail: String(e), error: true }, 5000));
    then();
    requestAnimationFrame(focusContent);
  };

  return (
    <div
      className={`app ${collapsed ? "is-side-collapsed" : ""}`}
      style={ambient ? ({ ["--lx-ambient" as string]: ambient } as React.CSSProperties) : undefined}
    >
      <Sidebar
        sections={sections}
        active={view.id}
        onSelect={goTo}
        collapsed={collapsed}
        onToggle={toggleSidebar}
        lastGame={recent[0] ?? null}
        onOpenGame={openGame}
        playing={playing}
        onCreateCollection={(name) => newCollection(name, null)}
        settingsActive={view.id === SETTINGS_ID}
        onSettings={() => goTo(SETTINGS_ID)}
      />

      {/* Barre de titre : le fond de la rangée du haut et de l'indication déplace la fenêtre,
          double clic = agrandir ou restaurer ; boutons de la fenêtre dans le coin (WindowControls). */}
      <WindowControls />
      <header className="top" data-tauri-drag-region>
        <span className="brand" data-tauri-drag-region="deep"><LudexMark />Ludex</span>
        <label className="search">
          <SearchIcon />
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedId(null);
              setOpen(false);
            }}
            onKeyDown={onSearchKey}
            placeholder="Rechercher un jeu, un studio, un tag"
            aria-label="Rechercher un jeu"
            spellCheck={false}
            autoComplete="off"
          />
          {query ? <kbd>Entrée</kbd> : <><kbd>Ctrl</kbd><kbd>K</kbd></>}
        </label>
        <div className="top__tools">
          {update && (
            <button
              type="button"
              className="top__update"
              aria-label={`Installer Ludex ${update.version} et relancer`}
              title={`Ludex ${update.version} est disponible : installer et relancer`}
              onClick={() => runUpdate(update)}
              disabled={updating}
            >
              <DownloadIcon />
            </button>
          )}
          <button type="button" className="top__add" title="Ajouter un jeu (Ctrl+N)" onClick={addManualGame}>
            <PlusIcon />Ajouter un jeu
          </button>
          <button
            type="button"
            aria-label="Rescanner la bibliothèque"
            title="Rescanner la bibliothèque"
            onClick={() => refresh(true)}
            disabled={scanning}
            className={scanning ? "is-spinning" : ""}
          >
            <RefreshIcon />
          </button>
        </div>
      </header>
      <p className="hint" aria-live="polite" data-tauri-drag-region="deep">
        {!scan
          ? "Recherche de tes jeux dans tous tes launchers"
          : query && selected
            ? <><kbd>Entrée</kbd> lance <b>{selected.title}</b></>
            : query
              ? "Aucun jeu ne correspond"
              : "Tape n'importe où pour chercher un jeu"}
      </p>

      <main className="scroll" ref={scrollRef}>
        {scanError && <p className="notice notice--error">Le scan a échoué : {scanError}</p>}
        {errors.map((s) => (
          <p key={s.platform} className="notice notice--error">{PLATFORM_LABEL[s.platform]} : {s.error}</p>
        ))}

        {!scan && !scanError && !inSettings && <LibrarySkeleton />}

        {scan && emptyLibrary && !inSettings && (
          <EmptyState
            title="Aucun jeu trouvé sur ce PC"
            action={
              <span className="empty__acts">
                <button type="button" className="btn-ghost" onClick={() => refresh(true)}><RefreshIcon /> Rescanner</button>
                <button type="button" className="btn-ghost" onClick={addManualGame}><PlusIcon /> Ajouter un jeu</button>
              </span>
            }
          >
            Ludex lit les jeux installés par Steam, Epic Games, GOG, Ubisoft, EA, Battle.net, Rockstar, Xbox et Riot, ainsi que les dossiers « Jeux » et « Games » de tes disques, sans connexion à tes comptes.
            Installe un jeu avec l'un d'eux puis relance un scan, ou ajoute un jeu rangé ailleurs en choisissant son exécutable.
            <span className="empty__sources">
              {scan.sources.map((s) => (
                <span key={s.platform}>{PLATFORM_LABEL[s.platform]} : {s.disabled ? "désactivé dans les Paramètres" : s.installed ? `${s.count} jeu${s.count > 1 ? "x" : ""}` : "non détecté"}</span>
              ))}
            </span>
          </EmptyState>
        )}

        {custom && (inSettings || (scan && !emptyLibrary)) && view.render?.(sectionContext)}

        {scan && !emptyLibrary && !custom && (
          <section aria-labelledby="lib-title">
            <div className="bar">
              {renamingCollection && mineCollection && !query ? (
                <InlineInput
                  className="bar__rename"
                  initial={mineCollection.name}
                  maxLength={MAX_COLLECTION_NAME}
                  label={`Nouveau nom de « ${mineCollection.name} »`}
                  onSubmit={(name) => renameCol(mineCollection.id, name)}
                  onDone={() => setRenamingCollection(false)}
                />
              ) : (
                <h2 id="lib-title" title={view.collection?.kind === "steam" ? "Collection lue dans Steam, modifiable seulement dans Steam" : undefined}>
                  {query ? "Résultats" : view.label}<span>{results.length}</span>
                </h2>
              )}
              {mineCollection && !query && (
                confirmDelete ? (
                  <span className="bar__confirm" role="group" aria-label="Confirmer la suppression">
                    Supprimer « {mineCollection.name} » ? Ses jeux restent dans la bibliothèque.
                    <button type="button" className="btn-ghost btn-ghost--danger" onClick={() => deleteCol(mineCollection.id, mineCollection.name)}>Supprimer</button>
                    <button type="button" className="btn-ghost" onClick={() => setConfirmDelete(false)}>Garder</button>
                  </span>
                ) : (
                  <span className="bar__tools">
                    <button
                      type="button"
                      className="btn-icon btn-icon--small"
                      aria-label="Renommer la collection"
                      title="Renommer la collection"
                      onMouseDown={(e) => renamingCollection && e.preventDefault()}
                      onClick={() => setRenamingCollection((r) => !r)}
                    >
                      <PencilIcon />
                    </button>
                    <button type="button" className="btn-icon btn-icon--small" aria-label="Supprimer la collection" title="Supprimer la collection" onClick={() => setConfirmDelete(true)}>
                      <TrashIcon />
                    </button>
                  </span>
                )
              )}
              <div className="seg" role="group" aria-label="Filtrer">
                {filters.map((f) => (
                  <button key={f} type="button" aria-pressed={activeFilter === f} onClick={() => changeFilter(f)}>
                    {FILTER_LABEL[f]} <em>{count(f)}</em>
                  </button>
                ))}
              </div>
              {/* Listes, tri et affichage : un seul bloc qui passe à la ligne d'un coup quand la place manque. */}
              <div className="bar__opts">
                <select
                  className="pick"
                  aria-label="Filtrer par ambiance"
                  value={facets.mood ?? ""}
                  onChange={(e) => changeFacets({ ...facets, mood: (e.target.value || null) as Mood | null })}
                >
                  <option value="">Ambiance</option>
                  {moodChoices.map(({ mood, count }) => (
                    <option key={mood} value={mood}>{MOOD_LABEL[mood]} ({count})</option>
                  ))}
                </select>
                <select
                  className="pick"
                  aria-label="Filtrer par tag Steam"
                  value={facets.tag ?? ""}
                  onChange={(e) => changeFacets({ ...facets, tag: e.target.value || null })}
                >
                  <option value="">Tag</option>
                  {/* Tag retenu absent de la liste (autre section, jeu disparu) : gardé pour rester affiché. */}
                  {facets.tag && !tagChoices.some((t) => t.name === facets.tag) && <option value={facets.tag}>{facets.tag} (0)</option>}
                  {tagChoices.map(({ name, count }) => (
                    <option key={name} value={name}>{name} ({count})</option>
                  ))}
                </select>
                <select
                  className="pick"
                  aria-label="Filtrer par genre"
                  value={facets.genre ?? ""}
                  onChange={(e) => changeFacets({ ...facets, genre: e.target.value || null })}
                >
                  <option value="">Genre</option>
                  {facets.genre && !genreChoices.some((t) => t.name === facets.genre) && <option value={facets.genre}>{facets.genre} (0)</option>}
                  {genreChoices.map(({ name, count }) => (
                    <option key={name} value={name}>{name} ({count})</option>
                  ))}
                </select>
                <select
                  className="bar__select"
                  value={statusFilter}
                  aria-label="Filtrer par statut"
                  onChange={(e) => changeStatusFilter(e.target.value as StatusFilter)}
                  onKeyDown={(e) => e.stopPropagation()} // les flèches et lettres restent au menu
                >
                  {STATUS_FILTERS.map((f) => (
                    <option key={f} value={f}>{STATUS_FILTER_LABEL[f]}</option>
                  ))}
                </select>
                <select
                  className="bar__select"
                  value={sort}
                  aria-label="Trier"
                  onChange={(e) => changeSort(e.target.value as Sort)}
                  onKeyDown={(e) => e.stopPropagation()}
                >
                  {SORTS.map((s) => (
                    <option key={s} value={s}>{SORT_LABEL[s]}</option>
                  ))}
                </select>
                <div className="seg seg--icons" role="group" aria-label="Affichage">
                  <button type="button" aria-pressed={layout === "grid"} aria-label="Grille" title="Grille" onClick={() => changeLayout("grid")}><GridIcon /></button>
                  <button type="button" aria-pressed={layout === "list"} aria-label="Liste" title="Liste" onClick={() => changeLayout("list")}><ListIcon /></button>
                </div>
              </div>
            </div>
            {hasFacets(facets) && (
              <ul className="pills" aria-label="Filtres actifs">
                {facets.mood && (
                  <li className="pill">
                    Ambiance : {MOOD_LABEL[facets.mood]}
                    <button type="button" aria-label={`Retirer le filtre ${MOOD_LABEL[facets.mood]}`} title="Retirer ce filtre" onClick={() => changeFacets({ ...facets, mood: null })}>×</button>
                  </li>
                )}
                {facets.tag && (
                  <li className="pill">
                    Tag : {facets.tag}
                    <button type="button" aria-label={`Retirer le filtre ${facets.tag}`} title="Retirer ce filtre" onClick={() => changeFacets({ ...facets, tag: null })}>×</button>
                  </li>
                )}
                {facets.genre && (
                  <li className="pill">
                    Genre : {facets.genre}
                    <button type="button" aria-label={`Retirer le filtre ${facets.genre}`} title="Retirer ce filtre" onClick={() => changeFacets({ ...facets, genre: null })}>×</button>
                  </li>
                )}
              </ul>
            )}
            {results.length === 0 && (
              <NoResults
                filter={activeFilter}
                status={statusFilter}
                section={view.render ? null : view.label}
                empty={view.empty}
                query={query}
                facets={facets}
                onClear={() => setQuery("")}
                onClearStatus={() => changeStatusFilter("any")}
                onClearFacets={() => changeFacets(NO_FACETS)}
              />
            )}
            {layout === "list" ? (
              <LibraryList
                games={results}
                selectedId={selected?.id ?? null}
                open={open}
                sort={sort}
                playing={playing}
                onSort={changeSort}
                onSelect={select}
                onPlay={play}
                renderStrip={renderStrip}
              />
            ) : (
              <LibraryGrid
                games={results}
                selectedId={selected?.id ?? null}
                open={open}
                meta={meta}
                playing={playing}
                onSelect={select}
                onPlay={play}
                renderStrip={renderStrip}
              />
            )}
          </section>
        )}
      </main>

      {showPadHints ? (
        <footer className="keys keys--pad" aria-hidden>
          <span><kbd>Croix</kbd> naviguer</span>
          <span><kbd className="pad">A</kbd> jouer</span>
          <span><kbd className="pad">X</kbd> fiche</span>
          <span><kbd className="pad">Y</kbd> favori</span>
          <span><kbd>LB</kbd><kbd>RB</kbd> section</span>
          <span><kbd>LT</kbd><kbd>RT</kbd> filtre</span>
          <span><kbd>Vue</kbd> tri</span>
          <span><kbd>Menu</kbd> actions</span>
          <span><kbd className="pad">B</kbd> fermer</span>
        </footer>
      ) : (
        <footer className="keys" aria-hidden>
          <span><kbd>↑</kbd><kbd>↓</kbd><kbd>←</kbd><kbd>→</kbd> naviguer</span>
          <span><kbd>Entrée</kbd> jouer</span>
          <span><kbd>Espace</kbd> fiche</span>
          <span><kbd>Ctrl</kbd><kbd>D</kbd> favori</span>
          <span><kbd>Maj</kbd><kbd>F10</kbd> actions</span>
          <span><kbd>Ctrl</kbd><kbd>1</kbd>…<kbd>{Math.min(9, sections.length)}</kbd> section</span>
          <span><kbd>Ctrl</kbd><kbd>N</kbd> ajouter</span>
          <span><kbd>F5</kbd> rescanner</span>
          <span><kbd>Échap</kbd> fermer</span>
        </footer>
      )}

      {menu && menuGame && (
        <ContextMenu
          key={`${menu.gameId}-${menu.at.x}-${menu.at.y}`}
          entries={menuEntries}
          at={menu.at}
          label={`Actions pour ${menuGame.title}`}
          keyboard={menu.keyboard}
          onAction={(a) => runMenu(menuGame, a)}
          onClose={closeMenu}
        />
      )}

      {showWelcome && (
        <WelcomeScreen
          scan={scan!}
          info={welcomeInfo!}
          pad={padConnected}
          onDone={() => closeWelcome(() => goTo("for-you"))}
          onShowHidden={() =>
            closeWelcome(() => {
              goTo("all");
              setQuery("");
              changeFilter("hidden");
            })
          }
        />
      )}

      <div className={`toast ${toast ? "is-on" : ""} ${toast?.error ? "toast--error" : ""}`} role="status" aria-live="polite">
        {toast && (
          <>
            {toast.spin && <span className="toast__spin" aria-hidden />}
            <div>
              <b>{toast.title}</b>
              {toast.detail && <code>{toast.detail}</code>}
            </div>
            {toast.action && (
              <button
                type="button"
                className="toast__action"
                onClick={() => {
                  // Fermé avant l'action : un toast ouvert par l'action reste affiché.
                  const run = toast.action!.run;
                  setToast(null);
                  run();
                }}
              >
                {toast.action.label}
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}

/** « Nouveau jeu : Hades », « 3 nouveaux jeux », « Jeu retiré : Portal »… `null` si rien n'a changé. */
function libraryToast({ added, removed }: LibraryEvent): Toast | null {
  const list = (titles: string[]) => (titles.length > 1 ? titles.join(", ") : undefined);
  const gone = removed.length === 1 ? `Jeu retiré : ${removed[0]}` : `${removed.length} jeux retirés`;
  if (added.length) {
    const title = added.length === 1 ? `Nouveau jeu : ${added[0]}` : `${added.length} nouveaux jeux`;
    const detail = [list(added), removed.length ? gone : undefined].filter(Boolean).join(" · ");
    return { title, detail: detail || undefined };
  }
  return removed.length ? { title: gone, detail: list(removed) } : null;
}

interface NoResultsProps {
  filter: Filter;
  status: StatusFilter;
  section: string | null;
  /** Texte de la section vide (sans recherche ni filtre). */
  empty?: string;
  query: string;
  facets: Facets;
  onClear: () => void;
  onClearStatus: () => void;
  onClearFacets: () => void;
}

function NoResults({ filter, status, section, empty, query, facets, onClear, onClearStatus, onClearFacets }: NoResultsProps) {
  if (!query && hasFacets(facets)) {
    const what = [facets.mood && `l'ambiance « ${MOOD_LABEL[facets.mood]} »`, facets.tag && `le tag « ${facets.tag} »`, facets.genre && `le genre « ${facets.genre} »`]
      .filter(Boolean)
      .join(" et ");
    return (
      <EmptyState
        title={`Aucun jeu avec ${what}`}
        action={<button type="button" className="btn-ghost" onClick={onClearFacets}>Retirer les filtres</button>}
      >
        {filter !== "all" ? `Dans « ${FILTER_LABEL[filter]} ». ` : ""}Les tags et les ambiances viennent de la boutique Steam : les jeux sans fiche Steam n'en ont pas
      </EmptyState>
    );
  }
  if (query) {
    return (
      <EmptyState
        icon="search"
        title={`Aucun jeu ne correspond à « ${query} »`}
        action={<button type="button" className="btn-ghost" onClick={onClear}>Effacer la recherche</button>}
      >
        La recherche porte sur le titre, le studio et les tags{section ? ` dans « ${section} »` : ""}{filter !== "all" ? ` dans « ${FILTER_LABEL[filter]} »` : ""}{status !== "any" ? `, statut « ${STATUS_FILTER_LABEL[status]} »` : ""}. Vérifie l'orthographe ou élargis le filtre
      </EmptyState>
    );
  }
  if (status !== "any") {
    return (
      <EmptyState
        title={`Aucun jeu « ${STATUS_FILTER_LABEL[status]} »${section ? ` dans « ${section} »` : ""}`}
        action={<button type="button" className="btn-ghost" onClick={onClearStatus}>Tous les statuts</button>}
      >
        Le statut se choisit dans la fiche d'un jeu : À faire, En cours, Terminé ou Abandonné
      </EmptyState>
    );
  }
  if (section && filter === "all") {
    return <EmptyState title={`Aucun jeu dans « ${section} »`}>{empty}</EmptyState>;
  }
  if (filter === "favorites") {
    return <EmptyState title="Pas encore de favoris">Ouvre la fiche d'un jeu et clique l'étoile, ou appuie sur Ctrl+D quand il est sélectionné</EmptyState>;
  }
  if (filter === "hidden") {
    return <EmptyState title="Aucun jeu masqué">Les applis qui ne sont pas des jeux, et ce que tu masques toi-même, arrivent ici et restent restaurables</EmptyState>;
  }
  return <EmptyState title={`Aucun jeu ${FILTER_LABEL[filter]}`}>Aucun jeu installé via ce launcher n'a été trouvé</EmptyState>;
}
