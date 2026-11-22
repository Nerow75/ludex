import { useCallback, useEffect, useState } from "react";
import { epicAccount, gogAccount, onEpicAccount, onGogAccount, type GogAccountState } from "../lib/api";
import { t } from "../lib/i18n";
import { readStoreFilter, saveStoreFilter, STORES, storeName, visibleStores, type Store, type StoreFilter } from "../lib/notInstalled";
import type { SectionContext } from "../lib/sections";
import { EpicGroup } from "./EpicGroup";
import { GogGroup } from "./GogView";
import { SteamGroup } from "./SteamGroup";
import { EmptyState } from "./States";

/**
 * Section « Pas installés » : jeux possédés dans une boutique (Steam, GOG, Epic) qui ne sont pas
 * sur ce PC, un groupe par boutique, filtre par boutique retenu dans `ludex.notInstalledStore`.
 * Steam sans compte (liste locale, clé Web API facultative) ; GOG et Epic par des comptes
 * facultatifs, connectés dans les Paramètres. Sans Steam ni aucun compte, un état vide y renvoie.
 */
export function NotInstalledView(ctx: SectionContext) {
  const [filter, setFilter] = useState<StoreFilter>(readStoreFilter);
  const [accounts, setAccounts] = useState<Partial<Record<Store, GogAccountState>>>({});
  const [counts, setCounts] = useState<Partial<Record<Store, number | null>>>({});
  const n = t.library.notInstalled;

  useEffect(() => {
    const set = (store: Store) => (state: GogAccountState) => setAccounts((a) => ({ ...a, [store]: state }));
    gogAccount().then((a) => set("gog")(a.state)).catch(() => {});
    epicAccount().then((a) => set("epic")(a.state)).catch(() => {});
    const offGog = onGogAccount((a) => set("gog")(a.state));
    const offEpic = onEpicAccount((a) => set("epic")(a.state));
    return () => {
      offGog.then((stop) => stop()).catch(() => {});
      offEpic.then((stop) => stop()).catch(() => {});
    };
  }, []);

  const onSteamCount = useCallback((c: number | null) => setCounts((x) => ({ ...x, steam: c })), []);
  const onGogCount = useCallback((c: number | null) => setCounts((x) => ({ ...x, gog: c })), []);
  const onEpicCount = useCallback((c: number | null) => setCounts((x) => ({ ...x, epic: c })), []);

  const choose = (f: StoreFilter) => {
    setFilter(f);
    saveStoreFilter(f);
  };

  // `counts.steam` : `undefined` pendant la lecture, `null` si Steam est absent de ce PC.
  const none = accounts.gog === "off" && accounts.epic === "off" && counts.steam === null;
  const shown = visibleStores(filter);
  const total = shown.reduce((sum, s) => sum + (counts[s] ?? 0), 0);

  return (
    <section className="wishes gog ni" aria-label={n.title}>
      <div className="bar">
        <h2>
          {n.title}
          {!none && total > 0 && (
            <span>
              <span className="sr-only">, </span>
              {total}
            </span>
          )}
        </h2>
        {!none && (
          <div className="seg" role="radiogroup" aria-label={n.filter}>
            {(["all", ...STORES] as StoreFilter[]).map((f) => (
              <button key={f} type="button" role="radio" aria-checked={filter === f} data-nav onClick={() => choose(f)}>
                {f === "all" ? n.all : storeName(f)}
              </button>
            ))}
          </div>
        )}
      </div>

      {none ? (
        <EmptyState
          title={n.subtitle}
          action={
            <button type="button" className="btn-ghost" data-nav onClick={ctx.openSettings}>
              {t.library.gog.openSettings}
            </button>
          }
        >
          {n.nothing}
        </EmptyState>
      ) : (
        <>
          {shown.includes("steam") && <SteamGroup ctx={ctx} onCount={onSteamCount} />}
          {shown.includes("gog") && <GogGroup ctx={ctx} onCount={onGogCount} />}
          {shown.includes("epic") && <EpicGroup ctx={ctx} onCount={onEpicCount} />}
        </>
      )}
    </section>
  );
}
