import { useEffect, useMemo, useState } from "react";
import { openInstallDir, PLATFORM_LABEL, storageReport, uninstallGame, type DiskUsage, type Game, type StorageReport, type UninstallCandidate } from "../lib/api";
import { formatDate, formatSize, relativeTime } from "../lib/format";
import type { SectionContext } from "../lib/sections";
import { Cover } from "./Cover";
import { EmptyState } from "./States";

/** Part d'un total, en % borné (largeur d'un segment de barre). */
const share = (part: number, total: number) => (total > 0 ? Math.min(100, Math.max(0, (part / total) * 100)) : 0);

function DiskRow({ disk }: { disk: DiskUsage }) {
  const used = Math.max(0, disk.total - disk.free);
  const games = Math.min(disk.gamesBytes, used);
  const freePct = Math.round(share(disk.free, disk.total));
  const label = `${disk.letter}${disk.name ? ` ${disk.name}` : ""}`;
  return (
    <li className={`disk${disk.low ? " is-low" : ""}`}>
      <div className="disk__head">
        <b>{disk.letter}</b>
        {disk.name && <span className="disk__name">{disk.name}</span>}
        <span className="disk__free">{formatSize(disk.free)} libres sur {formatSize(disk.total)}</span>
      </div>
      <div
        className="disk__bar"
        role="img"
        aria-label={`${label} : jeux ${formatSize(disk.gamesBytes)}, autres fichiers ${formatSize(used - games)}, libre ${formatSize(disk.free)}`}
      >
        <span className="disk__games" style={{ width: `${share(games, disk.total)}%` }} />
        <span className="disk__other" style={{ width: `${share(used - games, disk.total)}%` }} />
      </div>
      <p className="disk__legend">
        <span className="disk__key disk__key--games">Jeux {formatSize(disk.gamesBytes)}</span>
        {disk.platforms.map((p) => (
          <span key={p.platform} title={`${p.games} jeu${p.games > 1 ? "x" : ""}`}>
            {PLATFORM_LABEL[p.platform]} {formatSize(p.bytes)}
          </span>
        ))}
        <span className="disk__key disk__key--other">Autres fichiers {formatSize(used - games)}</span>
        {disk.unknown > 0 && <span>{disk.unknown} jeu{disk.unknown > 1 ? "x" : ""} sans taille connue</span>}
      </p>
      {disk.low && <p className="disk__warn">Presque plein : {freePct} % d'espace libre</p>}
    </li>
  );
}

/**
 * Section « Stockage » : une barre par disque qui porte des jeux (part des jeux de Ludex, autres
 * fichiers, espace libre), puis les jeux pas lancés depuis six mois ou jamais lancés, le plus gros
 * d'abord. Ludex ne supprime rien : Steam ouvre sa propre désinstallation, les autres ouvrent le dossier.
 */
export function StorageView({ games, openGame }: SectionContext) {
  const [report, setReport] = useState<StorageReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [poll, setPoll] = useState(0);

  // Relu quand la bibliothèque change (taille mesurée, jeu désinstallé…), avec un petit délai
  // pour regrouper les événements, puis toutes les 2 s tant que des dossiers sont mesurés.
  useEffect(() => {
    let alive = true;
    const t = window.setTimeout(() => {
      storageReport()
        .then((r) => {
          if (!alive) return;
          setReport(r);
          setError(null);
        })
        .catch((e) => alive && setError(String(e)));
    }, 300);
    return () => {
      alive = false;
      window.clearTimeout(t);
    };
  }, [games, poll]);
  useEffect(() => {
    if (!report?.measuring) return;
    const t = window.setTimeout(() => setPoll((n) => n + 1), 2000);
    return () => window.clearTimeout(t);
  }, [report]);

  const byId = useMemo(() => new Map(games.map((g) => [g.id, g])), [games]);
  const list = useMemo(
    () => (report?.candidates ?? []).flatMap((c) => (byId.has(c.id) ? [[c, byId.get(c.id) as Game] as const] : [])),
    [report, byId],
  );
  // Disque presque plein en tête.
  const disks = useMemo(() => [...(report?.disks ?? [])].sort((a, b) => Number(b.low) - Number(a.low)), [report]);
  const total = list.reduce((sum, [c]) => sum + c.bytes, 0);

  const act = async (c: UninstallCandidate) => {
    try {
      if (c.uninstall) {
        await uninstallGame(c.id);
        setNote(`Steam demande confirmation avant de désinstaller ${c.title}. La liste se met à jour ensuite.`);
      } else {
        await openInstallDir(c.id);
        setNote(null);
      }
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  if (!report) {
    return error ? <p className="notice notice--error">{error}</p> : <p className="hint">Lecture des disques…</p>;
  }
  if (disks.length === 0) {
    return <EmptyState title="Aucun disque à afficher">Les disques qui contiennent des jeux de Ludex apparaîtront ici</EmptyState>;
  }
  return (
    <section aria-labelledby="storage-title" className="storage">
      <div className="bar">
        <h2 id="storage-title">Stockage<span>{disks.length} disque{disks.length > 1 ? "s" : ""}</span></h2>
        {report.measuring && <span className="storage__busy">Mesure des dossiers en cours</span>}
      </div>
      {error && <p className="notice notice--error">{error}</p>}
      {note && <p className="notice">{note}</p>}
      <ul className="disks">
        {disks.map((d) => <DiskRow key={d.letter} disk={d} />)}
      </ul>

      <div className="bar">
        <h2>À désinstaller<span>{list.length ? `${list.length} · ${formatSize(total)}` : "0"}</span></h2>
      </div>
      {list.length === 0 ? (
        <p className="hint storage__none">Aucun jeu de taille connue n'est resté sans lancement depuis six mois</p>
      ) : (
        <>
          <p className="storage__intro">Pas lancés depuis six mois, ou jamais lancés, le plus gros d'abord</p>
          <ol className="hist store">
            {list.map(([c, g]) => (
              <li key={c.id} className="hist__item store__item">
                <button
                  type="button"
                  className="hist__row store__row"
                  data-nav
                  onClick={() => openGame(g)}
                  title={
                    c.lastPlayed
                      ? `Dernier lancement : ${formatDate(c.lastPlayed)}`
                      : c.launcherDates
                        ? undefined
                        : `${PLATFORM_LABEL[c.platform]} ne note pas les lancements : seuls ceux faits depuis Ludex sont connus`
                  }
                >
                  <Cover game={g} className="hist__cover" />
                  <span className="hist__title">{c.title}</span>
                  <span className="hist__meta">{PLATFORM_LABEL[c.platform]}{c.drive ? ` · ${c.drive}` : ""}</span>
                  <span className="hist__when">{c.lastPlayed || c.launcherDates ? relativeTime(c.lastPlayed) : "Jamais depuis Ludex"}</span>
                  <span className="store__size">{formatSize(c.bytes)}</span>
                </button>
                <button type="button" className="btn-ghost store__action" data-nav onClick={() => act(c)}>
                  {c.uninstall ? "Désinstaller" : "Ouvrir le dossier"}
                </button>
              </li>
            ))}
          </ol>
        </>
      )}
    </section>
  );
}
