import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  gameSessions,
  launchGame,
  onGameArt,
  onGameDetails,
  onGameSession,
  openInstallDir,
  PLATFORM_LABEL,
  PLATFORMS,
  scanLibrary,
  setFavorite,
  setHidden,
  type Game,
  type ScanResult,
  type SessionEvent,
} from "./lib/api";
import { ambientFromImage, titleColor } from "./lib/color";
import { formatPlaytime, formatSize, relativeTime } from "./lib/format";
import { useGamepad } from "./lib/gamepad";
import { recentGames, remember, remembered, selectGames, SORT_LABEL, type Filter, type Sort } from "./lib/library";
import { GameStrip } from "./components/GameStrip";
import { LibraryGrid } from "./components/LibraryGrid";
import { ResumeRow } from "./components/ResumeRow";
import { LudexMark, RefreshIcon, SearchIcon } from "./components/Icons";
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
  const [toast, setToast] = useState<Toast | null>(null);
  const [ambient, setAmbient] = useState<string | null>(null);
  /** Jeux lancés depuis Ludex dont un processus tourne. */
  const [playing, setPlaying] = useState<ReadonlySet<string>>(() => new Set());
  const toastTimer = useRef<number>(undefined);
  const searchRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLElement>(null);

  // Les applis masquées (Wallpaper Engine…) restent dans le scan mais n'apparaissent que dans « Masqués ».
  const games = scan?.games ?? [];
  const results = useMemo(() => selectGames(games, filter, sort, query), [games, filter, sort, query]);
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

  /** `manual` : rescan demandé par l'utilisateur, confirmé par un toast. */
  const refresh = useCallback(async (manual = false) => {
    setScanning(true);
    setScanError(null);
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
  }, [showToast]);

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

  // Fiches (genres, studio, sortie) reçues en arrière-plan après le scan.
  useEffect(() => {
    const unlisten = onGameDetails(({ id, details }) => patchGame(id, { details }));
    return () => void unlisten.then((stop) => stop()).catch(() => {});
  }, [patchGame]);

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

  const openFolder = useCallback(async (game: Game) => {
    try {
      await openInstallDir(game.id);
    } catch (e) {
      showToast({ title: "Dossier introuvable", detail: String(e), error: true }, 5000);
    }
  }, [showToast]);

  const focusCard = (id: string | undefined) =>
    requestAnimationFrame(() => scrollRef.current?.querySelector<HTMLElement>(`[data-game="${id}"] .card__hit`)?.focus({ preventScroll: true }));

  // Raccourcis globaux. L'état courant passe par une ref pour n'abonner l'écouteur qu'une fois.
  const live = useRef({ results, selected, open, query, play, toggleFavorite, refresh });
  live.current = { results, selected, open, query, play, toggleFavorite, refresh };

  /** Déplace la sélection dans la grille, comme les flèches du clavier. */
  const moveSelection = (dir: "up" | "down" | "left" | "right") => {
    const s = live.current;
    if (!s.results.length) return;
    const cards = [...(scrollRef.current?.querySelectorAll<HTMLElement>(".grid .card") ?? [])];
    const cols = Math.max(1, cards.filter((c) => c.offsetTop === cards[0]?.offsetTop).length);
    const i = Math.max(0, s.results.findIndex((g) => g.id === s.selected?.id));
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
      const onGame = target === document.body || target.matches(".card__hit, .tile__hit");
      const ctrl = e.ctrlKey || e.metaKey;
      const key = e.key;

      if ((ctrl && key.toLowerCase() === "k") || (key === "/" && !inInput)) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      } else if (key === "F5") {
        e.preventDefault(); // sinon la WebView recharge la page
        s.refresh(true);
      } else if (ctrl && key.toLowerCase() === "d") {
        e.preventDefault();
        if (s.selected) s.toggleFavorite(s.selected);
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
      } else if (onGame && key === "Enter") {
        e.preventDefault();
        if (s.selected) s.play(s.selected);
      } else if (onGame && key === " ") {
        e.preventDefault();
        setOpen((o) => !o);
      } else if (onGame && key.startsWith("Arrow") && s.results.length) {
        e.preventDefault();
        moveSelection(key.slice(5).toLowerCase() as "up" | "down" | "left" | "right");
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
  const count = (f: Filter) => selectGames(games, f, sort, "").length;
  // Un filtre par launcher qui a au moins un jeu (un launcher installé mais vide n'encombre pas la barre).
  const filters = FILTERS.filter((f) => f === "all" || f === "favorites" || f === "hidden" || f === filter || games.some((g) => g.platform === f));
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
        // La sélection quitte le champ de recherche pour la grille.
        moveSelection(action);
        break;
      case "confirm":
        if (s.selected) s.play(s.selected);
        break;
      case "back":
        back();
        break;
      case "details":
        if (s.selected) setOpen((o) => !o);
        break;
      case "favorite":
        if (s.selected) s.toggleFavorite(s.selected);
        break;
      case "prevFilter":
      case "nextFilter":
        changeFilter(cycle(filters, filter, action === "nextFilter" ? 1 : -1));
        break;
      case "prevSort":
      case "nextSort":
        changeSort(cycle(SORTS, sort, action === "nextSort" ? 1 : -1));
        break;
    }
  });
  const showPadHints = padConnected && padHints;

  return (
    <div className="app" style={ambient ? ({ ["--lx-ambient" as string]: ambient } as React.CSSProperties) : undefined}>

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
            action={<button type="button" className="btn-ghost" onClick={() => refresh(true)}><RefreshIcon /> Rescanner</button>}
          >
            Ludex lit les jeux installés par Steam, Epic Games, GOG, Ubisoft, EA, Battle.net, Rockstar, Xbox et Riot, ainsi que les dossiers « Jeux » et « Games » de tes disques, sans connexion à tes comptes.
            Installe un jeu avec l'un d'eux, puis relance un scan.
            <span className="empty__sources">
              {scan.sources.map((s) => (
                <span key={s.platform}>{PLATFORM_LABEL[s.platform]} : {s.installed ? `${s.count} jeu${s.count > 1 ? "x" : ""}` : "non détecté"}</span>
              ))}
            </span>
          </EmptyState>
        )}

        {scan && !emptyLibrary && !query && filter === "all" && <ResumeRow games={recent} playing={playing} onSelect={select} onPlay={play} />}

        {scan && !emptyLibrary && (
          <section aria-labelledby="lib-title">
            <div className="bar">
              <h2 id="lib-title">{query ? "Résultats" : "Bibliothèque"}<span>{results.length}</span></h2>
              <div className="seg" role="group" aria-label="Filtrer">
                {filters.map((f) => (
                  <button key={f} type="button" aria-pressed={filter === f} onClick={() => changeFilter(f)}>
                    {FILTER_LABEL[f]} <em>{count(f)}</em>
                  </button>
                ))}
              </div>
              <div className="seg" role="group" aria-label="Trier">
                {SORTS.map((s) => (
                  <button key={s} type="button" aria-pressed={sort === s} onClick={() => changeSort(s)}>{SORT_LABEL[s]}</button>
                ))}
              </div>
            </div>
            {results.length === 0 && <NoResults filter={filter} query={query} onClear={() => setQuery("")} />}
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
                  onClose={() => setOpen(false)}
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
          <span><kbd>LB</kbd><kbd>RB</kbd> filtre</span>
          <span><kbd>LT</kbd><kbd>RT</kbd> tri</span>
          <span><kbd className="pad">B</kbd> fermer</span>
        </footer>
      ) : (
        <footer className="keys" aria-hidden>
          <span><kbd>↑</kbd><kbd>↓</kbd><kbd>←</kbd><kbd>→</kbd> naviguer</span>
          <span><kbd>Entrée</kbd> jouer</span>
          <span><kbd>Espace</kbd> fiche</span>
          <span><kbd>Ctrl</kbd><kbd>D</kbd> favori</span>
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
                  toast.action!.run();
                  setToast(null);
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

function NoResults({ filter, query, onClear }: { filter: Filter; query: string; onClear: () => void }) {
  if (query) {
    return (
      <EmptyState
        icon="search"
        title={`Aucun jeu ne correspond à « ${query} »`}
        action={<button type="button" className="btn-ghost" onClick={onClear}>Effacer la recherche</button>}
      >
        La recherche porte sur le titre{filter !== "all" ? ` dans « ${FILTER_LABEL[filter]} »` : ""}. Vérifie l'orthographe ou élargis le filtre
      </EmptyState>
    );
  }
  if (filter === "favorites") {
    return <EmptyState title="Pas encore de favoris">Ouvre la fiche d'un jeu et clique l'étoile, ou appuie sur Ctrl+D quand il est sélectionné</EmptyState>;
  }
  if (filter === "hidden") {
    return <EmptyState title="Aucun jeu masqué">Les applis qui ne sont pas des jeux, et ce que tu masques toi-même, arrivent ici et restent restaurables</EmptyState>;
  }
  return <EmptyState title={`Aucun jeu ${FILTER_LABEL[filter]}`}>Aucun jeu installé via ce launcher n'a été trouvé</EmptyState>;
}
