import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { launchGame, PLATFORM_LABEL, scanLibrary, type Game, type ScanResult } from "./lib/api";
import { ambientFromImage, titleColor } from "./lib/color";
import { cleanTitle, norm, relativeTime } from "./lib/format";
import { GameCard } from "./components/GameCard";
import { ResumeRow } from "./components/ResumeRow";
import { RefreshIcon, SearchIcon } from "./components/Icons";

interface Toast { title: string; detail: string; error?: boolean }

const byLastPlayed = (a: Game, b: Game) => (b.lastPlayed ?? 0) - (a.lastPlayed ?? 0) || cleanTitle(a.title).localeCompare(cleanTitle(b.title), "fr");

export default function App() {
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [toast, setToast] = useState<Toast | null>(null);
  const [ambient, setAmbient] = useState<string | null>(null);
  const toastTimer = useRef<number>(undefined);
  const searchRef = useRef<HTMLInputElement>(null);

  // Les applis masquées (Wallpaper Engine…) restent dans le scan mais sortent de la bibliothèque.
  const library = useMemo(() => (scan?.games ?? []).filter((g) => !g.hidden).sort(byLastPlayed), [scan]);
  const results = useMemo(() => {
    const q = norm(query.trim());
    return q ? library.filter((g) => norm(cleanTitle(g.title)).includes(q)) : library;
  }, [library, query]);
  const recent = useMemo(() => library.filter((g) => g.lastPlayed).slice(0, 4), [library]);
  const selected = library.find((g) => g.id === selectedId) ?? (query ? results[0] : library[0]) ?? null;

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
    showToast({ title: `Lancement de ${game.title}`, detail: `via ${PLATFORM_LABEL[game.platform]} · ${game.launchLabel}` });
    try {
      patchGame(game.id, { lastPlayed: await launchGame(game.id) });
    } catch (e) {
      showToast({ title: `Impossible de lancer ${game.title}`, detail: String(e), error: true }, 6000);
    }
  }, [patchGame, showToast]);

  const select = useCallback((game: Game) => setSelectedId(game.id), []);

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
    if (e.key === "Enter" && results[0]) {
      e.preventDefault();
      play(query ? results[0] : selected ?? results[0]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (query) setQuery("");
      else e.currentTarget.blur();
    }
  };

  const errors = scan?.sources.filter((s) => s.error) ?? [];
  const topResult = query ? results[0] : null;

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
        {topResult ? <><kbd>Entrée</kbd> lance <b>{topResult.title}</b></> : "Tape le nom d'un jeu, Entrée pour le lancer"}
      </p>

      <main className="scroll">
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
            <div className="grid">
              {results.map((g) => (
                <GameCard key={g.id} game={g} selected={g.id === selected?.id} meta={relativeTime(g.lastPlayed)} onSelect={select} onPlay={play} />
              ))}
            </div>
          </section>
        )}
      </main>

      <div className={`toast ${toast ? "is-on" : ""} ${toast?.error ? "toast--error" : ""}`} role="status" aria-live="polite">
        {toast && (
          <>
            {!toast.error && <span className="toast__spin" aria-hidden />}
            <div>
              <b>{toast.title}</b>
              <code>{toast.detail}</code>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
