// Configuration requise (onglet Aperçu de la fiche) : lignes du tableau, colonne « Ce PC » et
// phrase de synthèse. Les verdicts viennent de Rust (`requirements.rs`) ; ici, seulement l'affichage.
import type { PcSpecs, RequirementKind, RequirementLine, Requirements, RequirementVerdict } from "./api";
import { live, nf, t } from "./i18n";

export const REQUIREMENT_LABEL: Record<RequirementKind, string> = live(() => t.fiche.reqs.kind);

const ORDER: RequirementKind[] = ["system", "cpu", "memory", "gpu", "directx", "storage", "notes"];

export const VERDICT_LABEL: Record<RequirementVerdict, string> = live(() => t.fiche.reqs.verdict);

export interface RequirementRow {
  kind: RequirementKind;
  minimum: RequirementLine | null;
  recommended: RequirementLine | null;
  /** Texte de la colonne « Ce PC » (`null` : rien à montrer). */
  pc: string | null;
}

const GIB = 1024 ** 3;

/** Taille en Go comme Windows les compte : « 64 Go », « 0,5 Go » (une décimale sous 2 Go). */
export function formatGib(bytes: number): string {
  const gib = bytes / GIB;
  return t.fiche.reqs.gb(nf({ maximumFractionDigits: 1 }).format(gib >= 2 ? Math.round(gib) : Math.round(gib * 10) / 10));
}

/** Colonne « Ce PC » d'une ligne. */
export function pcText(kind: RequirementKind, pc: PcSpecs): string | null {
  const r = t.fiche.reqs;
  switch (kind) {
    case "system":
      return pc.windows || null;
    case "cpu": {
      if (!pc.cpu) return null;
      const parts = [pc.cpu];
      if (pc.cores) parts.push(r.cores(pc.cores));
      if (pc.threads && pc.threads !== pc.cores) parts.push(r.threads(pc.threads));
      return parts.join(", ");
    }
    case "memory":
      return pc.memory ? formatGib(pc.memory) : null;
    case "gpu":
      return pc.gpus.length ? pc.gpus.map((g) => (g.vram > 0 ? `${g.name} (${formatGib(g.vram)})` : g.name)).join(" · ") : null;
    case "directx":
      return pc.directx ? `DirectX ${pc.directx}` : null;
    case "storage": {
      if (!pc.disk) return null;
      const free = r.free(formatGib(pc.disk.free), pc.disk.letter);
      return pc.disk.game ? r.withGame(free, formatGib(pc.disk.game)) : free;
    }
    case "notes":
      return null;
  }
}

/** Lignes du tableau, dans l'ordre fixe : chaque type présent dans l'une des deux configurations. */
export function requirementRows(req: Requirements): RequirementRow[] {
  const find = (lines: RequirementLine[], kind: RequirementKind) => lines.find((l) => l.kind === kind) ?? null;
  return ORDER.filter((k) => find(req.minimum, k) || find(req.recommended, k)).map((kind) => ({
    kind,
    minimum: find(req.minimum, kind),
    recommended: find(req.recommended, kind),
    pc: pcText(kind, req.pc),
  }));
}

/** « A », « A et B », « A, B et C ». */
function list(names: string[]): string {
  return names.length <= 1 ? names.join("") : `${names.slice(0, -1).join(", ")} ${t.fiche.reqs.and} ${names[names.length - 1]}`;
}

const named = (lines: RequirementLine[], verdict: RequirementVerdict) =>
  lines.filter((l) => l.kind !== "notes" && l.verdict === verdict).map((l) => REQUIREMENT_LABEL[l.kind]);

/** Phrase de synthèse au-dessus du tableau. `null` sans aucune ligne comparable. */
export function requirementSummary(req: Requirements): string | null {
  const r = t.fiche.reqs;
  const belowMin = named(req.minimum, "below");
  if (belowMin.length) return r.belowMin(list(belowMin));
  const belowRec = named(req.recommended, "below");
  if (belowRec.length) return r.belowRec(list(belowRec));
  const target = req.recommended.length ? req.recommended : req.minimum;
  const which = req.recommended.length ? r.recommended : r.minimum;
  const ok = named(target, "ok");
  const unknown = named(target, "unknown");
  if (!ok.length) return unknown.length ? r.nothing : null;
  if (!unknown.length) return r.reached(which);
  return r.partial(which, ok.length, ok.length + unknown.length, list(unknown));
}
