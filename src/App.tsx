import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { launchGame, openInstallDir, PLATFORM_LABEL, scanLibrary, setFavorite, setHidden, type Game, type ScanResult } from "./lib/api";
import { ambientFromImage, titleColor } from "./lib/color";
import { cleanTitle, norm, relativeTime } from "./lib/format";
import { GameStrip } from "./components/GameStrip";
import { LibraryGrid } from "./components/LibraryGrid";
import { ResumeRow } from "./components/ResumeRow";
import { RefreshIcon, SearchIcon } from "./components/Icons";

interface Toast {
  title: string;
  detail?: string;
  error?: boolean;
  spin?: boolean;
  action?: { label: string; run: () => void };
}

const byLastPlayed = (a: Game, b: Game) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || cleanTitle(a.title).localeCompare(cleanTitle(b.title), "fr");

export default function App() {
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [toast, setToast] = useState<Toast | null>(null);
  const [ambient, setAmbient] = useState<string | null>(null);
  const toastTimer = useRef<number>(undefined);
  const searchRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLElement>(null);

  // Les applis masquées (Wallpaper Engine…) restent dans le scan mais sortent de la bibliothèque.
  const library = useMemo(() => (scan?.games ?? []).filter((g) => !g.hidden).sort(byLastPlayed), [scan]);
  const results = useMemo(() => {
    const q = norm(query.trim());
    return q ? library.filter((g) => norm(cleanTitle(g.title)).includes(q)) : library;
  }, [library, query]);
  const recent = useMemo(() => library.filter((g) => g.lastPlayed).slice(0, 4), [library]);
  const selected = results.find((g) => g.id === selectedId) ?? (query ? results[0] : library[0]) ?? null;

  const refresh = useCallback(async () => {
    setScanning(true);
    setScanError(null);
    try {
      setScan(await scanLibrary());
    } catch (e) {
      setScanError(String(e));
    } finally {
      setScanning(false);
    }
  }, []);

  useEffect(() => void refresh(), [refresh]);

  // Couleur ambiante : teinte de la jaquette, sinon couleur dérivée du titre.
  useEffect(() => {
    if (!selected) return;
    let alive = true;
    setAmbient(titleColor(selected.title));
    if (selected.coverUrl) {
      ambientFromImage(selected.coverUrl).then((c) => alive && c && setAmbient(c));
    }
    return () => {
      alive = false;
    };
  }, [selected?.id]);

  // Garde la fiche dépliée (ou la jaquette sélectionnée) visible.
  useEffect(() => {
    const target = scrollRef.current?.querySelector(open ? ".strip" : ".card.is-sel");
    target?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [selected?.id, open]);

  const showToast = useCallback((t: Toast, ms = 3600) => {
    setToast(t);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), ms);
  }, []);

  /** Met à jour un jeu dans le scan courant sans rescanner. */
  const patchGame = useCallback((id: string, patch: Partial<Game>) => {
    setScan((s) => s && { ...s, games: s.games.map((g) => (g.id === id ? { ...g, ...patch } : g)) });
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

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
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
  const meta = useCallback((g: Game) => relativeTime(g.lastPlayed), []);

  return (
    <div className="app" style={ambient ? ({ ["--lx-ambient" as string]: ambient } as React.CSSProperties) : undefined}>
      <div className="wash" aria-hidden />

      <header className="top">
        <span className="brand"><span className="logo" aria-hidden />Ludex</span>
        <label className="search">
          <SearchIcon size={20} />
          <input
            ref={searchRef}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedId(null);
              setOpen(false);
            }}
            onKeyDown={onSearchKey}
            placeholder="Lancer un jeu…"
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
            onClick={refresh}
            disabled={scanning}
            className={scanning ? "is-spinning" : ""}
          >
            <RefreshIcon />
          </button>
        </div>
      </header>
      <p className="hint" aria-live="polite">
        {query && selected ? <><kbd>Entrée</kbd> lance <b>{selected.title}</b></> : "Tape le nom d'un jeu, Entrée pour le lancer"}
      </p>

      <main className="scroll" ref={scrollRef}>
        {scanError && <p className="notice notice--error">Le scan a échoué : {scanError}</p>}
        {errors.map((s) => (
          <p key={s.platform} className="notice notice--error">{PLATFORM_LABEL[s.platform]} : {s.error}</p>
        ))}

        {!scan && !scanError && <div className="loading">Recherche de tes jeux…</div>}

        {scan && !query && <ResumeRow games={recent} onSelect={select} onPlay={play} />}

        {scan && (
          <section aria-labelledby="lib-title">
            <div className="bar">
              <h2 id="lib-title">{query ? "Résultats" : "Bibliothèque"}<span>{results.length}</span></h2>
            </div>
            <LibraryGrid
              games={results}
              selectedId={selected?.id ?? null}
              open={open}
              meta={meta}
              onSelect={select}
              onPlay={play}
              renderStrip={(g, notch) => (
                <GameStrip
                  key={`strip-${g.id}`}
                  game={g}
                  notch={notch}
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
