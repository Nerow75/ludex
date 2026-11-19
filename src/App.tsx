import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  availableUpdate,
  addGame,
  changeExecutable,
  createCollection,
  deleteCollection,
  gameSessions,
  getCollections,
  installUpdate,
  launchGame,
  onGameArt,
  onGameDetails,
  onGameSession,
  onLibraryChanged,
  onUpdateAvailable,
  onTrayHint,
  onTrayLaunch,
  openInstallDir,
  PLATFORM_LABEL,
  PLATFORMS,
  MAX_COLLECTION_NAME,
  removeGame,
  renameCollection,
  renameGame,
  scanLibrary,
  setFavorite,
  setHidden,
  setInCollection,
  setNote,
  setStatus,
  STATUS_LABEL,
  type Collection,
  type CollectionsInfo,
  type Game,
  type LibraryEvent,
  type LibraryUpdate,
  type ScanResult,
  type SessionEvent,
  type Status,
  type UpdateInfo,
} from "./lib/api";
import { ambientFromImage, titleColor } from "./lib/color";
import { formatPlaytime, formatSize, relativeTime } from "./lib/format";
import { useGamepad } from "./lib/gamepad";
import {
  matchesFilter,
  matchesStatus,
  recentGames,
  remember,
  remembered,
  rememberedText,
  selectGames,
  SORT_LABEL,
  STATUS_FILTER_LABEL,
  STATUS_FILTERS,
  type Filter,
  type Sort,
  type StatusFilter,
} from "./lib/library";
import { collectionSections, SECTIONS, type Section, type SectionContext } from "./lib/sections";
import { GameStrip } from "./components/GameStrip";
import { Sidebar } from "./components/Sidebar";
import { LibraryGrid } from "./components/LibraryGrid";
import { ResumeRow } from "./components/ResumeRow";
import { SHIFT_EVENT } from "./components/ProgressBoard";
import { InlineInput } from "./components/InlineInput";
import { DownloadIcon, LudexMark, PencilIcon, PlusIcon, RefreshIcon, SearchIcon, TrashIcon } from "./components/Icons";
import { EmptyState, LibrarySkeleton } from "./components/States";

interface Toast {
  title: string;
  detail?: string;
  error?: boolean;
  spin?: boolean;
  action?: { label: string; run: () => void };
}

const FILTERS: readonly Filter[] = ["all", "favorites", ...PLATFORMS, "hidden"];
const SORTS = Object.keys(SORT_LABEL) as Sort[];
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
  const [statusFilter, setStatusFilter] = useState<StatusFilter>(() => remembered("status", STATUS_FILTERS, "any"));
  // Barre latérale : section affichée et barre repliée (icônes seules), gardées entre deux lancements.
  // Une section de collection est vérifiée à l'affichage (collection supprimée : première section).
  const [sectionId, setSectionId] = useState(() => rememberedText("section") ?? SECTIONS[0].id);
  /** Collections de l'utilisateur et de Steam, relues avec la bibliothèque. */
  const [collections, setCollections] = useState<CollectionsInfo | null>(null);
  const [collapsed, setCollapsed] = useState(() => remembered("sidebar", ["open", "collapsed"], "open") === "collapsed");
  const [toast, setToast] = useState<Toast | null>(null);
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
  // Section : un écran à elle (`render`, sauf pendant une recherche), sinon la grille restreinte à `scope`.
  const view = sections.find((s) => s.id === sectionId) ?? sections[0];
  const custom = !query && !!view.render;
  const scoped = useMemo(
    () => (view.scope ? games.filter(view.scope) : games).filter((g) => matchesStatus(g, statusFilter)),
    [games, view, statusFilter],
  );
  const activeFilter: Filter = view.filters && !view.filters(filter) ? "all" : filter;
  const results = useMemo(() => selectGames(scoped, activeFilter, sort, query), [scoped, activeFilter, sort, query]);
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

  const goTo = useCallback((id: string) => {
    setSectionId(id);
    setOpen(false);
    remember("section", id);
    scrollRef.current?.scrollTo({ top: 0 });
  }, []);

  const toggleSidebar = useCallback(() => {
    setCollapsed((c) => {
      remember("sidebar", c ? "open" : "collapsed");
      return !c;
    });
  }, []);

  /** Ouvre la fiche d'un jeu dans « Tous les jeux », avec un filtre qui le montre. */
  const openGame = useCallback((game: Game) => {
    goTo("all");
    setQuery("");
    setFilter((f) => (game.hidden ? "hidden" : f !== "favorites" && matchesFilter(game, f) ? f : "all"));
    setStatusFilter((f) => (matchesStatus(game, f) ? f : "any"));
    setSelectedId(game.id);
    setOpen(true);
  }, [goTo]);

  /** Collections relues (celles de Steam ont pu changer) ; sans elles, la barre n'a que les sections fixes. */
  const loadCollections = useCallback(() => {
    getCollections()
      .then(setCollections)
      .catch(() => {});
  }, []);

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
    const target = scrollRef.current?.querySelector(open ? ".strip" : ".card.is-sel");
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
    const unlisten = onGameArt(({ id, coverUrl, heroUrl, logoUrl }) => patchGame(id, { coverUrl, heroUrl, logoUrl }));
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
    } catch (e) {
      patchGame(game.id, { favorite: !value });
      showToast({ title: "Favori non enregistré", detail: String(e), error: true }, 5000);
    }
  }, [patchGame, showToast]);

  const toggleHidden = useCallback(async (game: Game) => {
    const value = !game.hidden;
    patchGame(game.id, { hidden: value });
    try {
      await setHidden(game.id, value);
      if (value) {
        setOpen(false);
        showToast({
          title: `${game.title} est masqué`,
          action: { label: "Annuler", run: () => toggleHidden({ ...game, hidden: true }) },
        }, 6000);
      } else {
        showToast({ title: `${game.title} est de retour dans la bibliothèque` });
      }
    } catch (e) {
      patchGame(game.id, { hidden: !value });
      showToast({ title: "Changement non enregistré", detail: String(e), error: true }, 5000);
    }
  }, [patchGame, showToast]);

  /** Statut de progression, affiché tout de suite, puis « Annuler » pendant 5 s. */
  const changeStatus = useCallback(async (game: Game, status: Status | null) => {
    const before = game.status;
    if (before === status) return;
    patchGame(game.id, { status });
    try {
      await setStatus(game.id, status);
      showToast({
        title: status ? `${game.title} : ${STATUS_LABEL[status]}` : `${game.title} : statut retiré`,
        action: { label: "Annuler", run: () => changeStatus({ ...game, status }, before) },
      }, 5000);
    } catch (e) {
      patchGame(game.id, { status: before });
      showToast({ title: "Statut non enregistré", detail: String(e), error: true }, 5000);
    }
  }, [patchGame, showToast]);

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
      showToast({
        title: value ? `${game.title} ajouté à « ${collection.name} »` : `${game.title} retiré de « ${collection.name} »`,
        action: { label: "Annuler", run: () => toggleCollection(game, collection, !value) },
      }, 5000);
    } catch (e) {
      showToast({ title: "Collection non modifiée", detail: String(e), error: true }, 5000);
    }
  }, [showToast]);

  /** Nouvelle collection : depuis une fiche (le jeu y entre), ou depuis la barre (on l'ouvre). */
  const newCollection = useCallback(async (name: string, game: Game | null) => {
    try {
      const { id, mine } = await createCollection(name, game?.id ?? null);
      setMine(mine);
      const created = mine.find((c) => c.id === id);
      if (game) {
        showToast({
          title: `${game.title} ajouté à « ${created?.name ?? name} »`,
          action: created && { label: "Annuler", run: () => toggleCollection(game, created, false) },
        }, 5000);
      } else {
        showToast({ title: `Collection « ${created?.name ?? name} » créée`, detail: "Ajoute des jeux depuis leur fiche, bouton Collections" });
        goTo(`col:${id}`);
      }
    } catch (e) {
      showToast({ title: "Collection non créée", detail: String(e), error: true }, 5000);
    }
  }, [goTo, showToast, toggleCollection]);

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
      // Le nouveau jeu doit être visible : ni filtre qui l'exclut, ni recherche.
      setFilter((f) => (f === "all" || f === "other" ? f : "all"));
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
  }, [applyUpdate, goTo, showToast]);

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

  const remove = useCallback(async (game: Game) => {
    try {
      setScan(await removeGame(game.id));
      setOpen(false);
      showToast({ title: `${game.title} retiré`, detail: "Ajouté de nouveau, il retrouvera son temps de jeu" });
    } catch (e) {
      showToast({ title: "Jeu non retiré", detail: String(e), error: true }, 5000);
    }
  }, [showToast]);

  const focusCard = (id: string | undefined) =>
    requestAnimationFrame(() => scrollRef.current?.querySelector<HTMLElement>(`[data-game="${id}"] .card__hit`)?.focus({ preventScroll: true }));

  // Raccourcis globaux. L'état courant passe par une ref pour n'abonner l'écouteur qu'une fois.
  const sectionIds = sections.map((s) => s.id);
  const live = useRef({ results, selected, open, query, play, toggleFavorite, refresh, addManualGame, custom, sectionId: view.id, sectionIds, goTo, toggleSidebar });
  live.current = { results, selected, open, query, play, toggleFavorite, refresh, addManualGame, custom, sectionId: view.id, sectionIds, goTo, toggleSidebar };

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
    const ids = s.sectionIds;
    const i = Math.max(0, ids.indexOf(s.sectionId));
    const next = (i + delta + ids.length) % ids.length;
    const follow = inSidebar();
    s.goTo(ids[next]);
    if (follow) sideItems()[next]?.focus();
  };

  /** Flèche ou croix : barre latérale, élément `[data-nav]` d'une section à elle, ou grille. */
  const navigate = (dir: "up" | "down" | "left" | "right") => {
    if (inSidebar()) {
      if (dir === "right") focusContent();
      else if (dir !== "left") stepSection(dir === "down" ? 1 : -1);
    } else if (live.current.custom) {
      const items = [...(scrollRef.current?.querySelectorAll<HTMLElement>("[data-nav]") ?? [])];
      const i = items.indexOf(document.activeElement as HTMLElement);
      if (dir === "left") focusSidebar();
      else items[i < 0 ? 0 : Math.min(items.length - 1, Math.max(0, i + (dir === "up" ? -1 : 1)))]?.focus();
    } else {
      moveSelection(dir);
    }
  };

  /** Déplace la sélection dans la grille, comme les flèches du clavier. */
  const moveSelection = (dir: "up" | "down" | "left" | "right") => {
    const s = live.current;
    if (!s.results.length) return void (dir === "left" && focusSidebar());
    const cards = [...(scrollRef.current?.querySelectorAll<HTMLElement>(".grid .card") ?? [])];
    const cols = Math.max(1, cards.filter((c) => c.offsetTop === cards[0]?.offsetTop).length);
    const i = Math.max(0, s.results.findIndex((g) => g.id === s.selected?.id));
    if (dir === "left" && i % cols === 0) return focusSidebar();
    const step = { left: -1, right: 1, up: -cols, down: cols }[dir];
    const next = s.results[Math.min(s.results.length - 1, Math.max(0, i + step))];
    setSelectedId(next.id);
    focusCard(next.id);
  };

  /** Échap ou B : replie la fiche, sinon efface la recherche. */
  const back = () => {
    const s = live.current;
    if (s.open) setOpen(false);
    else if (s.query) setQuery("");
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
      const onGame = !s.custom && (target === document.body || target.matches(".card__hit, .tile__hit"));
      const onNav = target === document.body || target.matches(".card__hit, .tile__hit, .side__item, [data-nav]");
      const ctrl = e.ctrlKey || e.metaKey;
      const key = e.key;

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
      } else if (inInput) {
        if (key === "ArrowDown" && s.selected) {
          e.preventDefault();
          setSelectedId(s.selected.id);
          focusCard(s.selected.id);
        }
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
  const meta = useCallback(
    (g: Game) => (sort === "size" ? formatSize(g.sizeOnDisk) : sort === "playtime" ? formatPlaytime(g.playtimeMinutes) : relativeTime(g.lastPlayed)),
    [sort],
  );
  const count = (f: Filter) => selectGames(scoped, f, sort, "").length;
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
  /** Passe au filtre ou au tri voisin, en boucle. */
  const cycle = <T,>(list: readonly T[], current: T, delta: number) =>
    list[(Math.max(0, list.indexOf(current)) + delta + list.length) % list.length];

  const padConnected = useGamepad((action) => {
    setPadHints(true);
    const s = live.current;
    switch (action) {
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
  const sectionContext: SectionContext = { games, playing, openGame, play, setStatus: changeStatus };
  const changeStatusFilter = (f: StatusFilter) => {
    setStatusFilter(f);
    setOpen(false);
    remember("status", f);
  };
  const mineCollection = view.collection?.kind === "mine" ? collections?.mine.find((c) => c.id === view.collection?.id) : undefined;

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
      />

      <header className="top">
        <span className="brand"><LudexMark />Ludex</span>
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
            placeholder="Rechercher un jeu"
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
      <p className="hint" aria-live="polite">
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

        {!scan && !scanError && <LibrarySkeleton />}

        {scan && emptyLibrary && (
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
                <span key={s.platform}>{PLATFORM_LABEL[s.platform]} : {s.installed ? `${s.count} jeu${s.count > 1 ? "x" : ""}` : "non détecté"}</span>
              ))}
            </span>
          </EmptyState>
        )}

        {scan && !emptyLibrary && custom && view.render?.(sectionContext)}

        {scan && !emptyLibrary && !custom && view.resume && !query && activeFilter === "all" && <ResumeRow games={recent} playing={playing} onSelect={select} onPlay={play} />}

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
                  {query ? "Résultats" : view.resume ? "Bibliothèque" : view.label}<span>{results.length}</span>
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
              <div className="seg" role="group" aria-label="Trier">
                {SORTS.map((s) => (
                  <button key={s} type="button" aria-pressed={sort === s} onClick={() => changeSort(s)}>{SORT_LABEL[s]}</button>
                ))}
              </div>
            </div>
            {results.length === 0 && (
              <NoResults
                filter={activeFilter}
                status={statusFilter}
                section={view.resume ? null : view.label}
                empty={view.empty}
                query={query}
                onClear={() => setQuery("")}
                onClearStatus={() => changeStatusFilter("any")}
              />
            )}
            <LibraryGrid
              games={results}
              selectedId={selected?.id ?? null}
              open={open}
              meta={meta}
              playing={playing}
              onSelect={select}
              onPlay={play}
              renderStrip={(g, notch) => (
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
                  onRemove={remove}
                  onClose={() => setOpen(false)}
                  collections={collections?.mine ?? []}
                  onStatus={changeStatus}
                  onNote={changeNote}
                  onToggleCollection={toggleCollection}
                  onCreateCollection={(name, game) => newCollection(name, game)}
                />
              )}
            />
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
          <span><kbd className="pad">B</kbd> fermer</span>
        </footer>
      ) : (
        <footer className="keys" aria-hidden>
          <span><kbd>↑</kbd><kbd>↓</kbd><kbd>←</kbd><kbd>→</kbd> naviguer</span>
          <span><kbd>Entrée</kbd> jouer</span>
          <span><kbd>Espace</kbd> fiche</span>
          <span><kbd>Ctrl</kbd><kbd>D</kbd> favori</span>
          <span><kbd>Ctrl</kbd><kbd>1</kbd>…<kbd>{Math.min(9, sections.length)}</kbd> section</span>
          <span><kbd>Ctrl</kbd><kbd>N</kbd> ajouter</span>
          <span><kbd>F5</kbd> rescanner</span>
          <span><kbd>Échap</kbd> fermer</span>
        </footer>
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
  onClear: () => void;
  onClearStatus: () => void;
}

function NoResults({ filter, status, section, empty, query, onClear, onClearStatus }: NoResultsProps) {
  if (query) {
    return (
      <EmptyState
        icon="search"
        title={`Aucun jeu ne correspond à « ${query} »`}
        action={<button type="button" className="btn-ghost" onClick={onClear}>Effacer la recherche</button>}
      >
        La recherche porte sur le titre{section ? ` dans « ${section} »` : ""}{filter !== "all" ? ` dans « ${FILTER_LABEL[filter]} »` : ""}{status !== "any" ? `, statut « ${STATUS_FILTER_LABEL[status]} »` : ""}. Vérifie l'orthographe ou élargis le filtre
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
