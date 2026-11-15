import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { launchGame, openInstallDir, PLATFORM_LABEL, scanLibrary, type Game, type Platform, type ScanResult } from "./lib/api";
import { ambientFromImage, titleColor } from "./lib/color";
import { Cover } from "./components/Cover";
import { Hero } from "./components/Hero";
import { CommandPalette } from "./components/CommandPalette";
import { FriendsIcon, HomeIcon, RefreshIcon, SearchIcon, TrophyIcon } from "./components/Icons";

type Filter = "all" | Platform;
interface Toast { title: string; detail: string; error?: boolean }

export default function App() {
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const [ambient, setAmbient] = useState<string | null>(null);
  const toastTimer = useRef<number>(undefined);

  const games = scan?.games ?? [];
  const visible = useMemo(() => (filter === "all" ? games : games.filter((g) => g.platform === filter)), [games, filter]);
  const selected = games.find((g) => g.id === selectedId) ?? visible[0] ?? null;

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

  const showToast = (t: Toast, ms = 3600) => {
    setToast(t);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), ms);
  };

  const play = async (game: Game) => {
    setSelectedId(game.id);
    setPaletteOpen(false);
    showToast({ title: `Lancement de ${game.title}`, detail: `via ${PLATFORM_LABEL[game.platform]} · ${game.launchLabel}` });
    try {
      await launchGame(game.id);
    } catch (e) {
      showToast({ title: `Impossible de lancer ${game.title}`, detail: String(e), error: true }, 6000);
    }
  };

  const openFolder = async (game: Game) => {
    try {
      await openInstallDir(game.id);
    } catch (e) {
      showToast({ title: "Dossier introuvable", detail: String(e), error: true }, 5000);
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const installedSources = scan?.sources.filter((s) => s.installed) ?? [];
  const missing = scan?.sources.filter((s) => !s.installed) ?? [];
  const errors = scan?.sources.filter((s) => s.error) ?? [];

  return (
    <div className="app" style={ambient ? ({ ["--lx-ambient" as string]: ambient } as React.CSSProperties) : undefined}>
      <nav className="rail" aria-label="Navigation">
        <span className="logo" aria-label="Ludex" />
        <button type="button" aria-label="Accueil" aria-current="page"><HomeIcon /></button>
        <button type="button" aria-label="Amis (bientôt)" title="Bientôt" disabled><FriendsIcon /></button>
        <button type="button" aria-label="Succès (bientôt)" title="Bientôt" disabled><TrophyIcon /></button>
        <span className="rail__spacer" />
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
      </nav>

      <main className="main">
        <header className="topbar">
          <button className="search" type="button" onClick={() => setPaletteOpen(true)}>
            <SearchIcon />
            <span>Rechercher et lancer un jeu</span>
            <kbd>Ctrl</kbd>
            <kbd>K</kbd>
          </button>
          {scan && (
            <span className="counts">
              {games.length} {games.length > 1 ? "jeux" : "jeu"} · {installedSources.length} launcher{installedSources.length > 1 ? "s" : ""}
            </span>
          )}
        </header>

        {scanError && <p className="notice notice--error">Le scan a échoué : {scanError}</p>}
        {errors.map((s) => (
          <p key={s.platform} className="notice notice--error">{PLATFORM_LABEL[s.platform]} : {s.error}</p>
        ))}

        {!scan && !scanError && <div className="loading">Recherche de tes jeux…</div>}

        {scan && games.length === 0 && (
          <div className="empty">
            <h2>Aucun jeu trouvé</h2>
            <p>Ludex cherche pour l'instant les jeux installés via Steam, Epic Games et Riot. Installe un jeu avec l'un d'eux, puis rescanne.</p>
            <button className="btn-ghost" type="button" onClick={refresh}><RefreshIcon /> Rescanner</button>
          </div>
        )}

        {selected && <Hero game={selected} onPlay={() => play(selected)} onOpenFolder={() => openFolder(selected)} />}

        {games.length > 0 && (
          <section className="library" aria-labelledby="lib-title">
            <div className="row-head">
              <h2 id="lib-title">Bibliothèque</h2>
              {missing.length > 0 && (
                <span className="row-head__note">Non détecté sur ce PC : {missing.map((s) => PLATFORM_LABEL[s.platform]).join(", ")}</span>
              )}
            </div>
            <div className="chips" role="group" aria-label="Filtrer par launcher">
              <button type="button" className="chip" aria-pressed={filter === "all"} onClick={() => setFilter("all")}>
                Tous <span>{games.length}</span>
              </button>
              {installedSources.map((s) => (
                <button key={s.platform} type="button" className="chip" aria-pressed={filter === s.platform} onClick={() => setFilter(s.platform)}>
                  {PLATFORM_LABEL[s.platform]} <span>{s.count}</span>
                </button>
              ))}
            </div>
            <div className="grid">
              {visible.map((g) => (
                <button
                  key={g.id}
                  type="button"
                  className="card"
                  aria-pressed={g.id === selected?.id}
                  onClick={() => setSelectedId(g.id)}
                  onDoubleClick={() => play(g)}
                  title="Double-clic pour jouer"
                >
                  <Cover game={g} />
                  <span className="card__title">{g.title}</span>
                  <span className="platform">{PLATFORM_LABEL[g.platform]}</span>
                </button>
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

      {paletteOpen && <CommandPalette games={games} onClose={() => setPaletteOpen(false)} onLaunch={play} />}
    </div>
  );
}
