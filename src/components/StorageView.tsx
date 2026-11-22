import { Fragment, useEffect, useMemo, useState, type ReactNode } from "react";
import { PLATFORM_LABEL, storageReport, type DiskUsage, type Game, type StorageReport, type UninstallCandidate } from "../lib/api";
import { formatDate, formatSize, relativeTime } from "../lib/format";
import { t } from "../lib/i18n";
import type { SectionContext } from "../lib/sections";
import { Cover } from "./Cover";
import { LauncherIcon } from "./LauncherIcon";
import { EmptyState } from "./States";
import { FootprintPanel } from "./FootprintPanel";
import { MoveJobView, MovePanel, useMoveJob } from "./MovePanel";
import { moveActive } from "../lib/gameMove";

/** Part d'un total, en % borné (largeur d'un segment de barre). */
const share = (part: number, total: number) => (total > 0 ? Math.min(100, Math.max(0, (part / total) * 100)) : 0);

/**
 * « Jeux de ce disque » sous la barre d'un disque, repliée : le plus gros d'abord, « Déplacer… »
 * ouvre le panneau de déplacement sous la ligne.
 */
function DiskGames({ disk, byId, openGame }: { disk: DiskUsage; byId: Map<string, Game>; openGame: (game: Game) => void }) {
  const [open, setOpen] = useState<string | null>(null);
  const list = disk.ids
    .flatMap((id) => (byId.has(id) ? [byId.get(id) as Game] : []))
    .sort((a, b) => (b.sizeOnDisk ?? 0) - (a.sizeOnDisk ?? 0) || a.title.localeCompare(b.title));
  if (list.length === 0) return null;
  const m = t.fiche.move;
  return (
    <details className="disk__list">
      <summary data-nav>{m.diskGames(list.length)}</summary>
      <ol className="hist store">
        {list.map((g) => (
          <Fragment key={g.id}>
            <li className="hist__item store__item">
              <button type="button" className="hist__row store__row" data-nav onClick={() => openGame(g)}>
                <Cover game={g} className="hist__cover" />
                <span className="hist__title">{g.title}</span>
                <span className="hist__meta">
                  <LauncherIcon platform={g.platform} size={12} />
                  {PLATFORM_LABEL[g.platform]}
                  {g.hidden ? ` · ${m.hidden}` : ""}
                </span>
                <span className="hist__when" aria-hidden />
                <span className="store__size">{g.sizeOnDisk ? formatSize(g.sizeOnDisk) : m.unknownSize}</span>
              </button>
              <button
                type="button"
                className="btn-ghost store__action"
                data-nav
                aria-expanded={open === g.id}
                title={m.storageHint}
                onClick={() => setOpen((o) => (o === g.id ? null : g.id))}
              >
                {open === g.id ? m.close : m.storageAction}
              </button>
            </li>
            {open === g.id && (
              <li className="store__panel">
                <MovePanel id={g.id} title={g.title} />
              </li>
            )}
          </Fragment>
        ))}
      </ol>
    </details>
  );
}

function DiskRow({ disk, children }: { disk: DiskUsage; children?: ReactNode }) {
  const used = Math.max(0, disk.total - disk.free);
  const games = Math.min(disk.gamesBytes, used);
  const freePct = Math.round(share(disk.free, disk.total));
  const label = `${disk.letter}${disk.name ? ` ${disk.name}` : ""}`;
  const st = t.library.storage;
  return (
    <li className={`disk${disk.low ? " is-low" : ""}`}>
      <div className="disk__head">
        <b>{disk.letter}</b>
        {disk.name && <span className="disk__name">{disk.name}</span>}
        <span className="disk__free">{st.free(formatSize(disk.free), formatSize(disk.total))}</span>
      </div>
      <div
        className="disk__bar"
        role="img"
        aria-label={st.bar(label, formatSize(disk.gamesBytes), formatSize(used - games), formatSize(disk.free))}
      >
        <span className="disk__games" style={{ width: `${share(games, disk.total)}%` }} />
        <span className="disk__other" style={{ width: `${share(used - games, disk.total)}%` }} />
      </div>
      <p className="disk__legend">
        <span className="disk__key disk__key--games">{st.games(formatSize(disk.gamesBytes))}</span>
        {disk.platforms.map((p) => (
          <span key={p.platform} title={t.library.gameCount(p.games)}>
            <LauncherIcon platform={p.platform} size={12} />{PLATFORM_LABEL[p.platform]} {formatSize(p.bytes)}
          </span>
        ))}
        <span className="disk__key disk__key--other">{st.otherFiles(formatSize(used - games))}</span>
        {disk.unknown > 0 && <span>{st.unknownSize(disk.unknown)}</span>}
      </p>
      {disk.low && <p className="disk__warn">{st.almostFull(freePct)}</p>}
      {children}
    </li>
  );
}

/**
 * Section « Stockage » : une barre par disque qui porte des jeux (part des jeux de Ludex, autres
 * fichiers, espace libre), puis les jeux pas lancés depuis six mois ou jamais lancés, le plus gros
 * d'abord. « Désinstaller… » ouvre la vue avancée sous la ligne (`FootprintPanel`) : ce que le jeu
 * laisse sur le PC, puis Steam, le désinstalleur officiel ou le dossier. Ludex ne supprime rien.
 */
export function StorageView({ games, openGame }: SectionContext) {
  const [report, setReport] = useState<StorageReport | null>(null);
  const [error, setError] = useState<string | null>(null);
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

  // Vue avancée ouverte : la ligne reste à sa place même quand le jeu quitte la liste (désinstallé),
  // pour en montrer les restes.
  const [open, setOpen] = useState<{ c: UninstallCandidate; g: Game; index: number; kind: "uninstall" | "move" } | null>(null);
  const rows = useMemo(() => {
    if (!open || list.some(([c]) => c.id === open.c.id)) return list;
    const out = [...list];
    out.splice(Math.min(open.index, out.length), 0, [open.c, open.g] as const);
    return out;
  }, [list, open]);
  const toggle = (c: UninstallCandidate, g: Game, index: number, kind: "uninstall" | "move") =>
    setOpen((o) => (o?.c.id === c.id && o.kind === kind ? null : { c, g, index, kind }));
  // Déplacement en cours ou fini : bandeau en haut, sauf si son panneau est déjà ouvert plus bas.
  const [job, setJob] = useMoveJob();
  const banner = job && (moveActive(job) || job.phase !== "cancelled") && !(open?.kind === "move" && open.c.id === job.id) ? job : null;
  const st = t.library.storage;

  if (!report) {
    return error ? <p className="notice notice--error">{error}</p> : <p className="hint">{st.reading}</p>;
  }
  if (disks.length === 0) {
    return <EmptyState title={st.emptyTitle}>{st.emptyBody}</EmptyState>;
  }
  return (
    <section aria-labelledby="storage-title" className="storage">
      <div className="bar">
        <h2 id="storage-title">{st.title}<span><span className="sr-only">, </span>{st.disks(disks.length)}</span></h2>
        {report.measuring && <span className="storage__busy">{st.measuring}</span>}
      </div>
      {error && <p className="notice notice--error">{error}</p>}
      {banner && (
        <div className="foot move move--banner">
          <MoveJobView job={banner} onChange={setJob} />
        </div>
      )}
      <ul className="disks">
        {disks.map((d) => (
          <DiskRow key={d.letter} disk={d}>
            <DiskGames disk={d} byId={byId} openGame={openGame} />
          </DiskRow>
        ))}
      </ul>

      <div className="bar">
        <h2>{st.toUninstall}<span><span className="sr-only">, </span>{list.length ? `${list.length} · ${formatSize(total)}` : "0"}</span></h2>
      </div>
      {list.length === 0 ? (
        <p className="hint storage__none">{st.none}</p>
      ) : (
        <>
          <p className="storage__intro">{st.intro}</p>
          <ol className="hist store">
            {rows.map(([c, g], index) => (
              <Fragment key={c.id}>
              <li className={`hist__item store__item${byId.has(c.id) ? " store__item--two" : ""}`}>
                <button
                  type="button"
                  className="hist__row store__row"
                  data-nav
                  onClick={() => openGame(g)}
                  title={
                    c.lastPlayed
                      ? st.lastLaunch(formatDate(c.lastPlayed))
                      : c.launcherDates
                        ? undefined
                        : st.noLaunchDates(PLATFORM_LABEL[c.platform])
                  }
                >
                  <Cover game={g} className="hist__cover" />
                  <span className="hist__title">{c.title}</span>
                  <span className="hist__meta"><LauncherIcon platform={c.platform} size={12} />{PLATFORM_LABEL[c.platform]}{c.drive ? ` · ${c.drive}` : ""}</span>
                  <span className="hist__when">{c.lastPlayed || c.launcherDates ? relativeTime(c.lastPlayed) : st.neverFromLudex}</span>
                  <span className="store__size">{formatSize(c.bytes)}</span>
                </button>
                <span className="store__acts">
                {byId.has(c.id) && (
                  <button
                    type="button"
                    className="btn-ghost store__action"
                    data-nav
                    aria-expanded={open?.c.id === c.id && open.kind === "move"}
                    onClick={() => toggle(c, g, index, "move")}
                    title={t.fiche.move.storageHint}
                  >
                    {open?.c.id === c.id && open.kind === "move" ? st.close : t.fiche.move.storageAction}
                  </button>
                )}
                <button
                  type="button"
                  className="btn-ghost store__action"
                  data-nav
                  aria-expanded={open?.c.id === c.id && open.kind === "uninstall"}
                  onClick={() => toggle(c, g, index, "uninstall")}
                  title={st.uninstallHint}
                >
                  {open?.c.id === c.id && open.kind === "uninstall" ? st.close : st.uninstall}
                </button>
                </span>
              </li>
              {open?.c.id === c.id && (
                <li className="store__panel">
                  {open.kind === "move" ? (
                    <MovePanel id={c.id} title={c.title} />
                  ) : (
                    <FootprintPanel id={c.id} title={c.title} present={byId.has(c.id)} />
                  )}
                </li>
              )}
              </Fragment>
            ))}
          </ol>
        </>
      )}
    </section>
  );
}
