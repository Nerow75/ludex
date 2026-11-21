import { useCallback, useEffect, useState } from "react";
import { chooseSavesDir, onSavesBackup, savesOverview, setAutoSaves, type SavesOverview } from "../lib/api";
import { formatBytes, relativeTime } from "../lib/format";
import { SavesPanel } from "./SavesPanel";
import { EmptyState } from "./States";

const copies = (n: number) => (n === 0 ? "Aucune copie" : `${n} copie${n > 1 ? "s" : ""}`);

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
    <section className="saves-page" aria-label="Sauvegardes">
      <div className="bar">
        <h2>Sauvegardes{overview && <span><span className="sr-only">, </span>{overview.games.length}</span>}</h2>
        <button type="button" className="btn-ghost" onClick={chooseDir}>Changer le dossier</button>
      </div>
      {overview && (
        <div className="saves-page__settings">
          <p className="saves__note">
            Copies dans <span className="path" title={overview.dir}>{overview.dir}</span>, 5 gardées par jeu.
          </p>
          <label className="saves-page__auto">
            <input type="checkbox" checked={overview.auto} onChange={(e) => toggleAuto(e.target.checked)} />
            Copier à la fin de chaque partie lancée depuis Ludex
          </label>
          {overview.status === "loading" && <p className="saves__note">Base des sauvegardes en cours de téléchargement.</p>}
          {overview.status === "unavailable" && <p className="saves__note">Base des sauvegardes indisponible : vérifie la connexion.</p>}
        </div>
      )}
      {error && <p className="saves__msg is-error" role="status">{error}</p>}
      {!overview && !error && <p className="saves__note">Recherche des sauvegardes…</p>}
      {overview && overview.games.length === 0 && (
        <EmptyState title="Aucune sauvegarde trouvée">
          Ludex cherche les sauvegardes de tes jeux grâce à la base communautaire Ludusavi.
        </EmptyState>
      )}
      {overview && overview.games.length > 0 && (
        <ul className="saves-page__list">
          {overview.games.map((g) => (
            <li key={g.id} className={open === g.id ? "is-open" : undefined}>
              <button type="button" className="saves-page__row" aria-expanded={open === g.id} onClick={() => setOpen(open === g.id ? null : g.id)}>
                <span className="saves-page__title">{g.title}</span>
                <span className="saves__meta">
                  {g.locations > 0 ? formatBytes(g.size) : "Rien sur ce PC"}
                  {" · "}
                  {copies(g.copies)}
                  {g.lastCopy ? `, la dernière ${relativeTime(g.lastCopy)}` : ""}
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
