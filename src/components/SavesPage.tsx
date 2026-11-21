import { useCallback, useEffect, useState } from "react";
import { chooseSavesDir, onSavesBackup, savesOverview, setAutoSaves, type SavesOverview } from "../lib/api";
import { formatBytes, relativeTime } from "../lib/format";
import { t } from "../lib/i18n";
import { SavesPanel } from "./SavesPanel";
import { EmptyState } from "./States";

const copies = (n: number) => t.fiche.saves.count(n);

/**
 * Page « Sauvegardes » autonome : jeux de la bibliothèque dont des sauvegardes sont trouvées sur ce
 * PC ou déjà copiées, dossier des copies et copie automatique. Chaque jeu se déplie sur ses
 * emplacements et ses copies (même panneau que la fiche).
 */
export function SavesPage() {
  const [overview, setOverview] = useState<SavesOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);

  const reload = useCallback(() => {
    savesOverview()
      .then((o) => {
        setOverview(o);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    reload();
    const off = onSavesBackup(() => reload());
    return () => {
      off.then((f) => f());
    };
  }, [reload]);

  const chooseDir = async () => {
    try {
      if (await chooseSavesDir()) reload();
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleAuto = async (value: boolean) => {
    setOverview((o) => (o ? { ...o, auto: value } : o));
    try {
      await setAutoSaves(value);
    } catch (e) {
      setError(String(e));
      reload();
    }
  };

  return (
    <section className="saves-page" aria-label={t.fiche.saves.page}>
      <div className="bar">
        <h2>{t.fiche.saves.page}{overview && <span><span className="sr-only">, </span>{overview.games.length}</span>}</h2>
        <button type="button" className="btn-ghost" onClick={chooseDir}>{t.fiche.saves.changeDir}</button>
      </div>
      {overview && (
        <div className="saves-page__settings">
          <p className="saves__note">
            {t.fiche.saves.copiesIn} <span className="path" title={overview.dir}>{overview.dir}</span>{t.fiche.saves.kept}
          </p>
          <label className="saves-page__auto">
            <input type="checkbox" checked={overview.auto} onChange={(e) => toggleAuto(e.target.checked)} />
            {t.fiche.saves.auto}
          </label>
          {overview.status === "loading" && <p className="saves__note">{t.fiche.saves.loading}</p>}
          {overview.status === "unavailable" && <p className="saves__note">{t.fiche.saves.unavailable}</p>}
        </div>
      )}
      {error && <p className="saves__msg is-error" role="status">{error}</p>}
      {!overview && !error && <p className="saves__note">{t.fiche.saves.searching}</p>}
      {overview && overview.games.length === 0 && (
        <EmptyState title={t.fiche.saves.emptyTitle}>
          {t.fiche.saves.emptyText}
        </EmptyState>
      )}
      {overview && overview.games.length > 0 && (
        <ul className="saves-page__list">
          {overview.games.map((g) => (
            <li key={g.id} className={open === g.id ? "is-open" : undefined}>
              <button type="button" className="saves-page__row" aria-expanded={open === g.id} onClick={() => setOpen(open === g.id ? null : g.id)}>
                <span className="saves-page__title">{g.title}</span>
                <span className="saves__meta">
                  {g.locations > 0 ? formatBytes(g.size) : t.fiche.saves.nothingHere}
                  {" · "}
                  {copies(g.copies)}
                  {g.lastCopy ? t.fiche.saves.last(relativeTime(g.lastCopy)) : ""}
                </span>
              </button>
              {open === g.id && <SavesPanel gameId={g.id} />}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
