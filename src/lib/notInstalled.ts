// Section « Pas installés » : boutiques montrées, filtre retenu, textes des groupes Steam et Epic,
// du compte Epic et de la clé Web API Steam. Calculs purs (testés par Vitest), le reste dans `GogView.tsx`, `EpicGroup.tsx` et
// `SteamGroup.tsx`, `NotInstalledView.tsx`.
import type { EpicLibrary, GogAccount, GogAccountState, SteamOwnedLibrary } from "./api";
import { relativeTime } from "./format";
import { t } from "./i18n";

/** Boutiques de la section, dans l'ordre d'affichage. */
export const STORES = ["steam", "gog", "epic"] as const;
export type Store = (typeof STORES)[number];
export type StoreFilter = Store | "all";

const FILTER_KEY = "ludex.notInstalledStore";

/** Filtre retenu ; valeur inconnue = toutes les boutiques. */
export function readStoreFilter(): StoreFilter {
  try {
    const v = localStorage.getItem(FILTER_KEY);
    return v && (STORES as readonly string[]).includes(v) ? (v as Store) : "all";
  } catch {
    return "all";
  }
}

export function saveStoreFilter(filter: StoreFilter): void {
  try {
    if (filter === "all") localStorage.removeItem(FILTER_KEY);
    else localStorage.setItem(FILTER_KEY, filter);
  } catch {
    // localStorage indisponible : filtre gardé le temps de la page.
  }
}

/** Boutiques affichées pour un filtre. */
export function visibleStores(filter: StoreFilter): Store[] {
  return filter === "all" ? [...STORES] : [filter];
}

/** Nom affiché d'une boutique. */
export function storeName(store: Store): string {
  return store === "steam" ? "Steam" : store === "gog" ? "GOG" : "Epic Games";
}

/** Ligne d'un groupe dont le compte n'est pas connecté, `null` s'il l'est. */
export function accountNote(state: GogAccountState, store: Store): string | null {
  const n = t.library.notInstalled;
  if (state === "connected") return null;
  return state === "expired" ? n.expiredNote(storeName(store)) : n.offNote(storeName(store));
}

/** « 3 jeux à installer sur 40 éléments de ta bibliothèque Epic. Liste relue il y a 2 h. » */
export function epicSummary(lib: EpicLibrary): string {
  const e = t.library.epic;
  const base = lib.games.length === 0 ? e.allInstalled(lib.owned) : e.summary(lib.games.length, lib.owned);
  return lib.checked ? `${base} ${t.library.gog.checked(relativeTime(lib.checked))}` : base;
}

/** État du compte Epic dans les Paramètres. */
export function epicAccountText(a: GogAccount): string {
  const c = t.settings.accounts;
  switch (a.state) {
    case "off":
      return a.loginOpen ? c.epicLoginOpen : c.off;
    case "connected":
      return a.username ? c.connectedAs(a.username) : c.connected;
    case "expired":
      return c.epicExpired;
  }
}

/** État du compte Epic dans le Diagnostic. */
export function epicDiagState(state: GogAccountState): { text: string; tone: "ok" | "off" | "warn" } {
  const c = t.settings.accounts;
  if (state === "connected") return { text: c.diagConnected, tone: "ok" };
  if (state === "expired") return { text: c.epicExpired, tone: "warn" };
  return { text: c.diagOff, tone: "off" };
}

/** « 3 jeux à installer sur 40 possédés. Liste relue il y a 2 h. », ou la même chose pour la liste locale. */
export function steamSummary(lib: SteamOwnedLibrary): string {
  const st = t.library.steam;
  if (lib.source === "local") return lib.games.length === 0 ? st.localAllInstalled(lib.owned) : st.localSummary(lib.games.length, lib.owned);
  const base = lib.games.length === 0 ? st.allInstalled(lib.owned) : st.summary(lib.games.length, lib.owned);
  return lib.checked ? `${base} ${t.library.gog.checked(relativeTime(lib.checked))}` : base;
}

/** Note sous le titre du groupe Steam : liste locale, clé refusée, ou rien. */
export function steamNote(lib: SteamOwnedLibrary): "local" | "refused" | null {
  if (lib.key.state === "expired") return "refused";
  return lib.source === "local" ? "local" : null;
}

/** État de la clé Web API Steam dans les Paramètres. */
export function steamKeyText(state: GogAccountState): string {
  const c = t.settings.accounts;
  return state === "connected" ? c.steamOn : state === "expired" ? c.steamRefused : c.steamOff;
}

/** État de la clé Web API Steam dans le Diagnostic. */
export function steamKeyDiagState(state: GogAccountState): { text: string; tone: "ok" | "off" | "warn" } {
  const c = t.settings.accounts;
  if (state === "connected") return { text: c.steamDiagOn, tone: "ok" };
  if (state === "expired") return { text: c.steamRefused, tone: "warn" };
  return { text: c.steamDiagOff, tone: "off" };
}
