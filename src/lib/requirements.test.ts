import { afterEach, describe, expect, it } from "vitest";
import { setLang } from "./i18n";
import type { PcSpecs, RequirementKind, RequirementLine, Requirements, RequirementVerdict } from "./api";
import { formatGib, pcText, requirementRows, requirementSummary } from "./requirements";
import { espaces } from "../test/jeu";

const GIB = 1024 ** 3;

const pc: PcSpecs = {
  windows: "Windows 11 Home 26H2 (build 26300.9974)",
  cpu: "AMD Ryzen 7 9800X3D",
  cores: 8,
  threads: 16,
  memory: 64 * GIB,
  gpus: [
    { name: "NVIDIA GeForce RTX 3070", vram: 7.8 * GIB },
    { name: "AMD Radeon Graphics", vram: 0.5 * GIB },
  ],
  directx: 12,
  disk: { letter: "E:", free: 993 * GIB, game: null },
};

const line = (kind: RequirementKind, verdict: RequirementVerdict = "ok", text = "x"): RequirementLine => ({ kind, text, verdict });

const req = (minimum: RequirementLine[], recommended: RequirementLine[] = []): Requirements => ({ minimum, recommended, pc });

describe("formatGib", () => {
  it("arrondit au Go, une décimale sous 2 Go", () => {
    expect(formatGib(64 * GIB)).toBe("64 Go");
    expect(formatGib(7.8 * GIB)).toBe("8 Go");
    expect(formatGib(0.5 * GIB)).toBe("0,5 Go");
    expect(espaces(formatGib(1118 * GIB))).toBe("1 118 Go");
  });
});

describe("pcText", () => {
  it("décrit chaque ligne du matériel", () => {
    expect(pcText("system", pc)).toBe("Windows 11 Home 26H2 (build 26300.9974)");
    expect(pcText("cpu", pc)).toBe("AMD Ryzen 7 9800X3D, 8 cœurs, 16 threads");
    expect(pcText("memory", pc)).toBe("64 Go");
    expect(pcText("gpu", pc)).toBe("NVIDIA GeForce RTX 3070 (8 Go) · AMD Radeon Graphics (0,5 Go)");
    expect(pcText("directx", pc)).toBe("DirectX 12");
    expect(pcText("storage", pc)).toBe("993 Go libres sur E:");
    expect(pcText("notes", pc)).toBeNull();
  });

  it("tient compte du jeu installé et des données manquantes", () => {
    expect(pcText("storage", { ...pc, disk: { letter: "D:", free: 96 * GIB, game: 18 * GIB } })).toBe("96 Go libres sur D:, jeu installé : 18 Go");
    const empty: PcSpecs = { windows: "", cpu: null, cores: null, threads: null, memory: null, gpus: [], directx: null, disk: null };
    for (const kind of ["system", "cpu", "memory", "gpu", "directx", "storage"] as RequirementKind[]) expect(pcText(kind, empty)).toBeNull();
    expect(pcText("cpu", { ...pc, cores: 1, threads: 1 })).toBe("AMD Ryzen 7 9800X3D, 1 cœur");
  });
});

describe("requirementRows", () => {
  it("réunit les deux configurations dans l'ordre fixe", () => {
    const rows = requirementRows(req([line("storage"), line("cpu"), line("notes")], [line("memory"), line("cpu", "unknown")]));
    expect(rows.map((r) => r.kind)).toEqual(["cpu", "memory", "storage", "notes"]);
    expect(rows[0].recommended?.verdict).toBe("unknown");
    expect(rows[1].minimum).toBeNull();
    expect(rows[1].pc).toBe("64 Go");
    expect(rows[3].pc).toBeNull();
  });
});

describe("requirementSummary", () => {
  it("signale d'abord ce qui est sous le minimum", () => {
    expect(requirementSummary(req([line("memory", "below"), line("storage", "below"), line("cpu")], [line("gpu", "below")]))).toBe(
      "Sous la configuration minimale pour : Mémoire et Stockage.",
    );
  });

  it("puis ce qui est sous le recommandé", () => {
    expect(requirementSummary(req([line("memory")], [line("memory", "below"), line("gpu", "below"), line("cpu", "below")]))).toBe(
      "Minimum atteint, sous la configuration recommandée pour : Mémoire, Graphiques et Processeur.",
    );
  });

  it("dit ce qui est atteint et ce qui reste à vérifier", () => {
    expect(requirementSummary(req([line("cpu")], [line("cpu"), line("memory"), line("notes", "unknown")]))).toBe(
      "Ce PC atteint la configuration recommandée.",
    );
    expect(requirementSummary(req([line("cpu"), line("memory")]))).toBe("Ce PC atteint la configuration minimale.");
    expect(requirementSummary(req([line("cpu")], [line("cpu", "unknown"), line("memory"), line("gpu", "unknown"), line("storage")]))).toBe(
      "Ce PC atteint la configuration recommandée pour 2 points sur 4. Processeur et Graphiques : à vérifier à la main.",
    );
  });

  it("n'invente rien sans ligne comparable", () => {
    expect(requirementSummary(req([line("cpu", "unknown")]))).toBe("Rien de comparable automatiquement : à vérifier à la main.");
    expect(requirementSummary(req([line("notes", "unknown")]))).toBeNull();
  });
});

describe("textes en anglais", () => {
  afterEach(() => setLang("fr"));

  it("écrit les tailles et la colonne « Ce PC »", () => {
    setLang("en");
    expect(formatGib(0.5 * GIB)).toBe("0.5 GB");
    expect(formatGib(1118 * GIB)).toBe("1,118 GB");
    expect(pcText("cpu", pc)).toBe("AMD Ryzen 7 9800X3D, 8 cores, 16 threads");
    expect(pcText("cpu", { ...pc, cores: 1, threads: 2 })).toBe("AMD Ryzen 7 9800X3D, 1 core, 2 threads");
    expect(pcText("gpu", pc)).toBe("NVIDIA GeForce RTX 3070 (8 GB) · AMD Radeon Graphics (0.5 GB)");
    expect(pcText("storage", { ...pc, disk: { letter: "D:", free: 96 * GIB, game: 18 * GIB } })).toBe("96 GB free on D:, game installed: 18 GB");
  });

  it("donne la synthèse", () => {
    setLang("en");
    expect(requirementSummary(req([line("memory", "below"), line("storage", "below"), line("cpu")]))).toBe(
      "Below the minimum requirements for: Memory and Storage.",
    );
    expect(requirementSummary(req([line("memory")], [line("memory", "below"), line("gpu", "below"), line("cpu", "below")]))).toBe(
      "Minimum met, below the recommended requirements for: Memory, Graphics and Processor.",
    );
    expect(requirementSummary(req([line("cpu"), line("memory")]))).toBe("This PC meets the minimum requirements.");
    expect(requirementSummary(req([line("cpu")], [line("cpu", "unknown"), line("memory"), line("gpu", "unknown"), line("storage")]))).toBe(
      "This PC meets the recommended requirements on 2 of 4 points. Processor and Graphics: check manually.",
    );
    expect(requirementSummary(req([line("cpu", "unknown")]))).toBe("Nothing can be compared automatically: check manually.");
  });
});
