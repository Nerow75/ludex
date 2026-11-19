import type { Game } from "./api";

/*
 * Recherche tolérante. Rang d'un jeu pour une recherche, du meilleur au moins bon :
 *   0 préfixe du titre (« rocket », « the witcher » ou « witcher »)
 *   1 initiales, chiffres compris (« cod », « gta », « fm20 », « rdr2 », « sc2 »)
 *   2 début d'un mot du titre (« league »)
 *   3 sous-chaîne, ou chaque mot de la recherche au début d'un mot du titre (« manager football »)
 *   4 une faute de frappe, 5 deux fautes (« rocet », « footbal manger ») : jamais sous 4 lettres,
 *     première lettre juste, nombres jamais changés
 *   6 studio, éditeur ou tag Steam dont un mot commence par la recherche
 * Accents, casse, ®/™, apostrophes et ponctuation ignorés. Fonctions pures, sans dépendance.
 */

export const RANK_META = 6;

const ARTICLES = new Set(["the", "a", "an", "le", "la", "les", "l"]);
const ROMAN: Record<string, string> = { ii: "2", iii: "3", iv: "4", v: "5", vi: "6", vii: "7", viii: "8", ix: "9", x: "10" };
const isDigit = (c: string) => c >= "0" && c <= "9";

/** Texte sans accents, casse, ®/™ ni ponctuation ; lettres et chiffres séparés (« cs2 » → « cs 2 »). */
function fold(s: string, camel = false): string[] {
  let t = s.normalize("NFD").replace(/\p{M}/gu, "").replace(/[®™©]/g, "").replace(/['’`´]/g, "").replace(/&/g, " and ");
  // « StarCraft » → « Star Craft » pour les initiales seulement (« sc2 »).
  if (camel) t = t.replace(/(\p{Ll})(\p{Lu})/gu, "$1 $2");
  return t
    .toLowerCase()
    .replace(/(\p{L})(\p{N})|(\p{N})(\p{L})/gu, (_, a, b, c, d) => (a ? `${a} ${b}` : `${c} ${d}`))
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);
}

interface Prepared {
  words: string[];
  joined: string;
  compact: string;
  /** Sans article de tête (« The Witcher 3 » → « witcher 3 »), sinon `joined`. */
  bare: string;
  /** Mots pour les initiales (« StarCraft II » → star, craft, ii). */
  acronym: string[];
}

function prepare(s: string, camel = false): Prepared {
  const words = fold(s);
  const joined = words.join(" ");
  const bare = words.length > 1 && ARTICLES.has(words[0]) ? words.slice(1).join(" ") : joined;
  return { words, joined, compact: words.join(""), bare, acronym: camel ? fold(s, true) : words };
}

// Titres déjà préparés : la recherche relit les mêmes à chaque touche.
const titles = new Map<string, Prepared>();
function prepareTitle(title: string): Prepared {
  let p = titles.get(title);
  if (!p) {
    if (titles.size > 4000) titles.clear();
    p = prepare(title, true);
    titles.set(title, p);
  }
  return p;
}

/** Valeur d'un mot nombre (« 2020 », « ii » hors premier mot), `null` sinon. */
function numberOf(word: string, index: number): string | null {
  if (isDigit(word[0])) return word;
  return index > 0 ? (ROMAN[word] ?? null) : null;
}

/**
 * La recherche (sans espaces) suit les initiales des mots à partir de `i` : une lettre par mot,
 * un nombre du titre pris en entier ou par sa fin (« 20 » pour « 2020 », deux chiffres au moins).
 */
function followsInitials(words: string[], i: number, q: string, j: number): boolean {
  if (j === q.length) return true;
  if (i === words.length) return false;
  const w = words[i];
  const num = numberOf(w, i);
  if (num !== null && isDigit(q[j])) {
    let k = j;
    while (k < q.length && isDigit(q[k])) k++;
    const run = q.slice(j, k);
    const fits = run === num || (run.length >= 2 && num.length > run.length && num.endsWith(run));
    return fits && followsInitials(words, i + 1, q, k);
  }
  // Un nombre écrit en chiffres ne se saute pas et ne donne pas de lettre.
  if (isDigit(w[0])) return false;
  return w[0] === q[j] && followsInitials(words, i + 1, q, j + 1);
}

function initialsMatch(t: Prepared, q: Prepared): boolean {
  if (q.compact.length < 2) return false;
  const w = t.acronym;
  if (followsInitials(w, 0, q.compact, 0)) return true;
  return w.length > 1 && ARTICLES.has(w[0]) && followsInitials(w, 1, q.compact, 0);
}

/**
 * Distance d'édition (transpositions comprises) entre `a` et le début le plus proche de `b`
 * d'au moins `minLen` lettres (ou `b` entier s'il est plus court).
 */
export function prefixDistance(a: string, b: string, minLen = 0): number {
  const m = a.length;
  const n = b.length;
  // d[i][j] : distance entre a[0..i] et b[0..j].
  const d: number[][] = Array.from({ length: m + 1 }, (_, i) => Array.from({ length: n + 1 }, (_, j) => (i === 0 ? j : j === 0 ? i : 0)));
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      let v = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost);
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) v = Math.min(v, d[i - 2][j - 2] + 1);
      d[i][j] = v;
    }
  }
  return Math.min(...d[m].slice(Math.min(minLen, n)));
}

/**
 * Début du mot comparé : au moins aussi long que la recherche jusqu'à 5 lettres (« star » ne
 * trouve pas « Strike » par une lettre en trop), une lettre de moins au-delà (« rockket »).
 */
const minPrefix = (len: number) => (len < 6 ? len : len - 1);

/** Fautes tolérées : aucune sous 4 lettres, une jusqu'à 6, deux au-delà. */
export const maxTypos = (len: number) => (len < 4 ? 0 : len < 7 ? 1 : 2);

/** Un mot sans chiffre de la recherche contre un mot du titre : fautes ou `null` si trop loin. */
function wordTypos(token: string, word: string): number | null {
  if (word.startsWith(token)) return 0;
  const max = maxTypos(token.length);
  if (max === 0 || word[0] !== token[0] || isDigit(word[0])) return null;
  const d = prefixDistance(token, word, minPrefix(token.length));
  return d <= max ? d : null;
}

/** Fautes de frappe : nombre de fautes (1 ou 2), `null` si trop loin ou non permis. */
function typoMatch(t: Prepared, q: Prepared): number | null {
  const letters = q.compact.replace(/\d/g, "").length;
  if (letters < 4) return null;
  let best: number | null = null;
  // Le titre tapé d'un bloc, espaces ignorés (« rocketleage »), seulement sans chiffre.
  if (!/\d/.test(q.compact)) {
    for (const target of [t.compact, t.bare.replace(/ /g, "")]) {
      if (target[0] !== q.compact[0]) continue;
      const d = prefixDistance(q.compact, target, minPrefix(q.compact.length));
      if (d > 0 && d <= maxTypos(q.compact.length)) best = best === null ? d : Math.min(best, d);
    }
  }
  // Mot par mot : chaque mot de la recherche trouve un mot du titre ; nombres à l'identique.
  let total = 0;
  let ok = true;
  for (const tok of q.words) {
    if (isDigit(tok[0])) {
      if (!t.words.some((w, i) => numberOf(w, i) === tok)) ok = false;
    } else {
      const ds = t.words.map((w) => wordTypos(tok, w)).filter((d): d is number => d !== null);
      if (ds.length === 0) ok = false;
      else total += Math.min(...ds);
    }
    if (!ok) break;
  }
  if (ok && total > 0 && total <= maxTypos(letters)) best = best === null ? total : Math.min(best, total);
  return best;
}

/** Rang du titre pour une recherche déjà préparée (voir l'en-tête), `null` si rien ne correspond. */
function rankTitle(t: Prepared, q: Prepared): number | null {
  if (!q.compact) return null;
  if (t.joined.startsWith(q.joined) || t.bare.startsWith(q.joined) || t.compact.startsWith(q.compact)) return 0;
  if (initialsMatch(t, q)) return 1;
  if (` ${t.joined}`.includes(` ${q.joined}`)) return 2;
  if (t.compact.includes(q.compact) || q.words.every((tok) => t.words.some((w) => w.startsWith(tok)))) return 3;
  const typos = typoMatch(t, q);
  return typos === null ? null : 3 + typos;
}

/** Un studio, un éditeur ou un tag dont un mot commence par la recherche (« ubisoft », « coop »). */
function metaMatch(game: Game, q: Prepared): boolean {
  const d = game.details;
  if (!d || !q.joined) return false;
  return [...d.developers, ...d.publishers, ...d.tags].some((v) => ` ${prepare(v).joined}`.includes(` ${q.joined}`));
}

export type Query = Prepared;

/** Recherche préparée une fois pour toute la liste. */
export const prepareQuery = (query: string): Query => prepare(query);

/** Rang d'un titre seul pour une recherche (0 à 5), `null` si rien ne correspond. */
export const titleRank = (title: string, query: string | Query): number | null =>
  rankTitle(prepareTitle(title), typeof query === "string" ? prepare(query) : query);

/** Rang d'un jeu (0 à 5 par le titre, `RANK_META` par studio, éditeur ou tag), `null` sinon. */
export function searchRank(game: Game, query: string | Query): number | null {
  const q = typeof query === "string" ? prepare(query) : query;
  const r = rankTitle(prepareTitle(game.title), q);
  if (r !== null) return r;
  return metaMatch(game, q) ? RANK_META : null;
}
