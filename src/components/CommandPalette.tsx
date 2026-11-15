import { useEffect, useMemo, useRef, useState } from "react";
import { PLATFORM_LABEL, type Game } from "../lib/api";
import { Cover } from "./Cover";

const norm = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

interface Props {
  games: Game[];
  onClose: () => void;
  onLaunch: (game: Game) => void;
}

export function CommandPalette({ games, onClose, onLaunch }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  const results = useMemo(() => {
    const q = norm(query.trim());
    if (!q) return games;
    return games.filter((g) => norm(g.title).includes(q) || norm(PLATFORM_LABEL[g.platform]).includes(q));
  }, [games, query]);

  useEffect(() => setActive(0), [query]);
  useEffect(() => {
    listRef.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown" && results.length) {
      e.preventDefault();
      setActive((i) => (i + 1) % results.length);
    } else if (e.key === "ArrowUp" && results.length) {
      e.preventDefault();
      setActive((i) => (i - 1 + results.length) % results.length);
    } else if (e.key === "Enter" && results[active]) {
      e.preventDefault();
      onLaunch(results[active]);
    }
  };

  return (
    <div className="palette-bg" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="palette" role="dialog" aria-modal="true" aria-label="Lancer un jeu" onKeyDown={onKeyDown}>
        <input
          autoFocus
          id="palette-input"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Tape le nom d'un jeu…"
          autoComplete="off"
          spellCheck={false}
          aria-controls="palette-list"
        />
        <ul id="palette-list" role="listbox" ref={listRef}>
          {results.length === 0 && <li className="palette__empty">Aucun jeu ne correspond.</li>}
          {results.map((g, i) => (
            <li
              key={g.id}
              role="option"
              aria-selected={i === active}
              onMouseEnter={() => setActive(i)}
              onClick={() => onLaunch(g)}
            >
              <Cover game={g} className="palette__cover" />
              <span className="palette__title">{g.title}</span>
              <span className="platform">{PLATFORM_LABEL[g.platform]}</span>
              <kbd className="palette__hint">Entrée</kbd>
            </li>
          ))}
        </ul>
        <div className="palette__foot">
          <span><kbd>↑</kbd> <kbd>↓</kbd> naviguer</span>
          <span><kbd>Entrée</kbd> jouer</span>
          <span><kbd>Échap</kbd> fermer</span>
        </div>
      </div>
    </div>
  );
}
