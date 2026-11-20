import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { launchGame, PLATFORM_LABEL, type Game } from "../lib/api";
import { cleanTitle, installBadge, relativeTime } from "../lib/format";
import { hideQuick, onLibraryChanged, onQuickShow, quickGames, quickResults } from "../lib/quick";
import { Cover } from "./Cover";
import { LauncherIcon } from "./LauncherIcon";
import { LudexMark } from "./Icons";

const errorText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : "Erreur inconnue");

/**
 * Recherche rapide, ouverte par le raccourci global (`quick.rs`) : quelques lettres, ↑ ↓ pour
 * choisir, Entrée lance le jeu, Échap ferme. Le focus reste toujours dans le champ.
 */
export function QuickLauncher() {
  const [games, setGames] = useState<Game[] | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [launching, setLaunching] = useState<string | null>(null);
  const [error, setError] = useState<{ title: string; detail: string } | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);

  const results = useMemo(() => quickResults(games ?? [], query), [games, query]);
  const current = results[Math.min(selected, results.length - 1)] ?? null;

  const load = useCallback(() => {
    quickGames()
      .then(setGames)
      .catch((e) => setError({ title: "Bibliothèque illisible", detail: errorText(e) }));
  }, []);

  useEffect(() => {
    load();
    const focus = () => input.current?.focus();
    const shown = onQuickShow(() => {
      setQuery("");
      setSelected(0);
      setError(null);
      setLaunching(null);
      list.current?.scrollTo({ top: 0 });
      load();
      focus();
    });
    const changed = onLibraryChanged(load);
    window.addEventListener("focus", focus);
    // Menu natif de la WebView inutile ici (gardé dans le champ, pour coller).
    const noMenu = (e: MouseEvent) => {
      if (e.target !== input.current) e.preventDefault();
    };
    document.addEventListener("contextmenu", noMenu);
    return () => {
      shown.then((stop) => stop()).catch(() => {});
      changed.then((stop) => stop()).catch(() => {});
      window.removeEventListener("focus", focus);
      document.removeEventListener("contextmenu", noMenu);
    };
  }, [load]);

  // Le jeu choisi reste visible quand on descend dans la liste.
  useEffect(() => {
    if (current) document.getElementById(`quick-${current.id}`)?.scrollIntoView({ block: "nearest" });
  }, [current]);

  const launch = async (game: Game) => {
    if (launching) return;
    setLaunching(game.id);
    setError(null);
    try {
      // Rust cache la fenêtre et prévient la fenêtre principale.
      await launchGame(game.id);
    } catch (e) {
      setError({ title: `Impossible de lancer ${cleanTitle(game.title)}`, detail: errorText(e) });
    } finally {
      setLaunching(null);
    }
  };

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const last = results.length - 1;
    const index = Math.min(selected, last);
    switch (e.key) {
      case "ArrowDown":
        setSelected(Math.min(index + 1, last));
        break;
      case "ArrowUp":
        setSelected(Math.max(index - 1, 0));
        break;
      case "PageDown":
        setSelected(Math.min(index + 5, last));
        break;
      case "PageUp":
        setSelected(Math.max(index - 5, 0));
        break;
      case "Enter":
        if (current && !e.repeat) void launch(current);
        break;
      case "Escape":
        void hideQuick();
        break;
      case "Tab":
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  const empty =
    games === null
      ? null
      : games.length === 0
        ? "Bibliothèque pas encore lue : ouvre Ludex le temps d'un scan."
        : results.length === 0
          ? `Aucun jeu pour « ${query.trim()} ».`
          : null;

  return (
    <div className="quick">
      <div className="quick__search">
        <LudexMark size={24} />
        <input
          ref={input}
          className="quick__input"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setSelected(0);
            setError(null);
          }}
          onKeyDown={onKey}
          placeholder="Lancer un jeu…"
          aria-label="Rechercher un jeu à lancer"
          role="combobox"
          aria-expanded={results.length > 0}
          aria-controls="quick-list"
          aria-activedescendant={current ? `quick-${current.id}` : undefined}
          autoFocus
          spellCheck={false}
          autoComplete="off"
        />
      </div>
      {error && (
        <p className="quick__error" role="alert">
          <strong>{error.title}</strong> {error.detail}
        </p>
      )}
      <div className="quick__list" id="quick-list" role="listbox" aria-label={query.trim() ? "Résultats" : "Derniers jeux joués"} ref={list}>
        {empty && <p className="quick__empty">{empty}</p>}
        {results.map((g, i) => {
          const badge = installBadge(g.installState);
          const isCurrent = g === current;
          return (
            <div
              key={g.id}
              id={`quick-${g.id}`}
              role="option"
              aria-selected={isCurrent}
              className={`quick__item${isCurrent ? " is-selected" : ""}${launching === g.id ? " is-launching" : ""}`}
              // Le focus reste dans le champ : la frappe continue après un clic.
              onMouseDown={(e) => e.preventDefault()}
              onMouseMove={() => i !== selected && setSelected(i)}
              onClick={() => void launch(g)}
            >
              <Cover game={g} className="quick__cover" />
              <span className="quick__text">
                <span className="quick__title">{cleanTitle(g.title)}</span>
                <span className="quick__meta">
                  <LauncherIcon platform={g.platform} size={12} />{PLATFORM_LABEL[g.platform]} · {badge ? <span className="is-pending">{badge}</span> : relativeTime(g.lastPlayed)}
                </span>
              </span>
              <kbd className="quick__enter">{launching === g.id ? "Lancement…" : "Entrée"}</kbd>
            </div>
          );
        })}
      </div>
      <div className="quick__keys" aria-hidden>
        <span>
          <kbd>↑</kbd> <kbd>↓</kbd> choisir
        </span>
        <span>
          <kbd>Entrée</kbd> jouer
        </span>
        <span>
          <kbd>Échap</kbd> fermer
        </span>
      </div>
    </div>
  );
}
