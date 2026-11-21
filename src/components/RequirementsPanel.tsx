// Section « Configuration » de l'onglet Aperçu : configuration minimale et recommandée de la boutique
// Steam, ce PC à côté, une pastille par ligne. Lue à l'ouverture de l'onglet ; cachée sans données.
import { useEffect, useState } from "react";
import { gameRequirements, type RequirementLine, type Requirements } from "../lib/api";
import { REQUIREMENT_LABEL, VERDICT_LABEL, requirementRows, requirementSummary } from "../lib/requirements";

function Cell({ line, column }: { line: RequirementLine | null; column: string }) {
  if (!line) return <span role="cell" className="reqs__cell is-empty" data-label={column}>·</span>;
  const marked = line.kind !== "notes";
  return (
    <span role="cell" className="reqs__cell" data-label={column} title={marked ? VERDICT_LABEL[line.verdict] : undefined}>
      {marked && <i className={`reqs__dot is-${line.verdict}`} aria-hidden="true" />}
      <span className="reqs__text">{line.text}</span>
      {marked && <span className="reqs__sr">{` (${VERDICT_LABEL[line.verdict]})`}</span>}
    </span>
  );
}

export function RequirementsPanel({ gameId }: { gameId: string }) {
  const [req, setReq] = useState<Requirements | null>(null);
  useEffect(() => {
    let live = true;
    gameRequirements(gameId)
      .then((r) => live && setReq(r))
      .catch(() => live && setReq(null));
    return () => {
      live = false;
    };
  }, [gameId]);
  if (!req) return null;
  const rows = requirementRows(req);
  if (!rows.length) return null;
  const hasRec = req.recommended.length > 0;
  const summary = requirementSummary(req);
  return (
    <section className="reqs" aria-label="Configuration requise">
      <p className="fiche__label" role="heading" aria-level={3}>Configuration</p>
      {summary && <p className="reqs__summary">{summary}</p>}
      <div className={`reqs__table ${hasRec ? "" : "is-single"}`} role="table" aria-label="Configuration requise et ce PC">
        <div role="row" className="reqs__row reqs__head">
          <span role="columnheader"><span className="reqs__sr">Élément</span></span>
          <span role="columnheader">Minimale</span>
          {hasRec && <span role="columnheader">Recommandée</span>}
          <span role="columnheader">Ce PC</span>
        </div>
        {rows.map((r) => (
          <div key={r.kind} role="row" className="reqs__row" data-nav tabIndex={-1}>
            <span role="rowheader" className="reqs__kind">{REQUIREMENT_LABEL[r.kind]}</span>
            <Cell line={r.minimum} column="Minimale" />
            {hasRec && <Cell line={r.recommended} column="Recommandée" />}
            <span role="cell" className={`reqs__cell reqs__pc ${r.pc ? "" : "is-empty"}`} data-label="Ce PC">{r.pc ?? "·"}</span>
          </div>
        ))}
      </div>
      <p className="fiche__note">
        Configurations de la boutique Steam. Processeur et carte graphique ne sont comparés que si la correspondance est sûre.
      </p>
    </section>
  );
}
