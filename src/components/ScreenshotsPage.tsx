import { useCallback, useEffect, useState } from "react";
import { allScreenshots, type ScreenshotGroup } from "../lib/api";
import { ImageIcon, RefreshIcon } from "./Icons";
import { ScreenshotViewer, ShotTile } from "./ScreenshotViewer";

interface Props {
  /** Clic sur le titre d'un jeu : ouvrir sa fiche (facultatif). */
  onOpenGame?: (gameId: string) => void;
}

const count = (n: number) => `${n} capture${n > 1 ? "s" : ""}`;

/**
 * Page « Captures » : toutes les captures (Steam, Xbox Game Bar), jeu par jeu, le jeu à la capture
 * la plus récente d'abord, puis « Autres captures ». Autonome : elle lit elle-même ses données.
 */
export function ScreenshotsPage({ onOpenGame }: Props) {
  const [groups, setGroups] = useState<ScreenshotGroup[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState<{ group: number; index: number } | null>(null);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    allScreenshots()
      .then((g) => {
        setGroups(g);
        setOpen(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(load, [load]);

  const total = groups?.reduce((n, g) => n + g.shots.length, 0) ?? 0;
  const viewed = open && groups?.[open.group];

  return (
    <section className="shots-page" aria-label="Captures" aria-busy={loading}>
      <header className="shots-page__head">
        <h2>Captures</h2>
        {groups && <span className="eyebrow">{count(total)}</span>}
        <button type="button" className="btn-icon" aria-label="Relire les captures" title="Relire les captures" disabled={loading} onClick={load}>
          <RefreshIcon />
        </button>
      </header>

      {error && <p className="shots-page__error" role="alert">{error}</p>}

      {groups && total === 0 && (
        <div className="empty" role="status">
          <span className="empty__icon"><ImageIcon /></span>
          <h3>Aucune capture</h3>
          <p>Ludex lit les captures de Steam (F12 en jeu) et celles de la Xbox Game Bar (Win+Alt+Impr. écran).</p>
        </div>
      )}

      {groups?.map((g, gi) => (
        <section key={g.gameId ?? "autres"} className="shots-group" aria-label={g.title}>
          <h3>
            {g.gameId && onOpenGame ? (
              <button type="button" className="shots-group__link" onClick={() => onOpenGame(g.gameId!)}>{g.title}</button>
            ) : (
              g.title
            )}
            <span className="eyebrow">{count(g.shots.length)}</span>
          </h3>
          <div className="shots-row">
            {g.shots.map((s, i) => (
              <ShotTile key={s.key} shot={s} label={`Capture ${i + 1} de ${g.title}`} onOpen={() => setOpen({ group: gi, index: i })} />
            ))}
          </div>
        </section>
      ))}

      {viewed && open && (
        <ScreenshotViewer
          shots={viewed.shots}
          index={open.index}
          title={viewed.title}
          onIndex={(index) => setOpen({ group: open.group, index })}
          onClose={() => setOpen(null)}
        />
      )}
    </section>
  );
}
