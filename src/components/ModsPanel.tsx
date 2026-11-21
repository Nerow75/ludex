import { useEffect, useState } from "react";
import { openModPage, openModsDir, refreshMods, type WorkshopMod, type WorkshopMods } from "../lib/api";
import { t } from "../lib/i18n";
import { modDetail, modsSummary, modTitle } from "../lib/workshop";

interface Props {
  gameId: string;
  /** Liste déjà lue à l'ouverture de la fiche (disque et cache). */
  initial: WorkshopMods;
}

/** Vignette du mod ; cadre vide si elle manque ou ne se charge pas. */
function ModThumb({ mod }: { mod: WorkshopMod }) {
  const [failed, setFailed] = useState(false);
  if (!mod.previewUrl || failed) return <span className="mods__thumb" aria-hidden="true" />;
  return <img className="mods__thumb" src={mod.previewUrl} alt="" loading="lazy" decoding="async" onError={() => setFailed(true)} />;
}

/** Onglet « Mods » : mods Steam Workshop installés, titres demandés à Steam s'ils manquent. */
export function ModsPanel({ gameId, initial }: Props) {
  const [list, setList] = useState<WorkshopMods>(initial);
  const [loading, setLoading] = useState(initial.stale);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setList(initial);
    if (!initial.stale) return;
    let alive = true;
    setLoading(true);
    refreshMods(gameId)
      .then((l) => alive && l && setList(l))
      .catch((e) => alive && setError(String(e)))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [gameId, initial]);

  const run = (task: Promise<void>) => {
    setError(null);
    task.catch((e) => setError(String(e)));
  };

  // Titres demandés sans réponse (hors ligne, ou Steam qui ne répond pas) : on le dit une fois.
  const untitled = list.mods.filter((m) => m.title === null).length;
  return (
    <div className="mods">
      <div className="mods__head">
        <p className="mods__summary">{modsSummary(list)}</p>
        <button type="button" className="btn-ghost" data-nav disabled={!list.folder} onClick={() => run(openModsDir(gameId))}>
          {t.fiche.mods.openFolder}
        </button>
      </div>
      {loading && <p className="fiche__note">{t.fiche.mods.loading}</p>}
      {!loading && list.stale && untitled > 0 && (
        <p className="fiche__note">{t.fiche.mods.noTitles}</p>
      )}
      {error && <p className="mods__msg" role="status">{error}</p>}
      <ul className="mods__list" aria-label={t.fiche.mods.listLabel}>
        {list.mods.map((m) => (
          <li key={m.id} className="mods__row">
            <ModThumb mod={m} />
            <div className="mods__text">
              <span className="mods__title" title={m.title ? `${m.title} (${m.id})` : undefined}>{modTitle(m)}</span>
              <span className="mods__meta">
                {modDetail(m)}
                {m.pending && <span className="mods__pending"> · {t.fiche.mods.pendingBadge}</span>}
              </span>
            </div>
            <button
              type="button"
              className="btn-ghost"
              data-nav
              aria-label={t.fiche.mods.viewLabel(modTitle(m))}
              onClick={() => run(openModPage(gameId, m.id))}
            >
              {t.fiche.mods.view}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
