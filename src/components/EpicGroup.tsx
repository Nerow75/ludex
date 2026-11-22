import { useCallback, useEffect, useState } from "react";
import { epicInstall, epicLibrary, onEpicAccount, type EpicGame, type EpicLibrary } from "../lib/api";
import { t } from "../lib/i18n";
import { accountNote, epicSummary, storeName } from "../lib/notInstalled";
import type { SectionContext } from "../lib/sections";
import { Tile } from "./GogView";
import { LauncherIcon } from "./LauncherIcon";

/**
 * Groupe Epic de la section « Pas installés » : jeux Epic possédés qui ne sont pas sur ce PC
 * (compte Epic facultatif, connecté dans les Paramètres). « Installer dans Epic » ouvre le
 * launcher Epic sur le jeu (URI reconstruite côté Rust) ; Ludex ne télécharge rien pour Epic.
 */
export function EpicGroup({ ctx, onCount }: { ctx: SectionContext; onCount?: (n: number | null) => void }) {
  const [lib, setLib] = useState<EpicLibrary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const e = t.library.epic;
  // Un jeu installé ou désinstallé change la liste.
  const epicCount = ctx.games.filter((x) => x.platform === "epic").length;

  const load = useCallback((refresh: boolean) => {
    setBusy(true);
    return epicLibrary(refresh)
      .then((l) => {
        setLib(l);
        setError(l.error);
      })
      .catch((err) => setError(String(err)))
      .finally(() => setBusy(false));
  }, []);

  useEffect(() => {
    load(false);
  }, [load, epicCount]);

  useEffect(() => {
    const off = onEpicAccount(() => load(false));
    return () => {
      off.then((stop) => stop()).catch(() => {});
    };
  }, [load]);

  const state = lib?.account.state;
  useEffect(() => {
    onCount?.(lib && state === "connected" ? lib.games.length : null);
  }, [lib, state, onCount]);

  const install = async (game: EpicGame) => {
    setError(null);
    setInfo(null);
    try {
      await epicInstall(game.id);
      setInfo(e.opened(game.title));
    } catch (err) {
      setError(String(err));
    }
  };

  const note = lib ? accountNote(lib.account.state, "epic") : null;
  return (
    <section className="ni__group" aria-labelledby="ni-epic">
      <div className="ni__head">
        <h3 id="ni-epic" className="with-icon">
          <LauncherIcon platform="epic" size={18} />
          {storeName("epic")}
          {lib && state === "connected" && (
            <span className="ni__count">
              <span className="sr-only">, </span>
              {lib.games.length}
            </span>
          )}
        </h3>
        {state === "connected" && (
          <button type="button" className="btn-ghost" data-nav disabled={busy} onClick={() => load(true)}>
            {busy ? e.refreshing : t.library.gog.refresh}
          </button>
        )}
      </div>

      {!lib && !error && <p className="wishes__note" role="status">{e.loading}</p>}
      {error && <p className="wishes__msg" role="alert">{error}</p>}
      {info && <p className="wishes__note" role="status">{info}</p>}

      {note && (
        <p className="wishes__note ni__off">
          {note}{" "}
          <button type="button" className="btn-ghost set__small" data-nav onClick={ctx.openSettings}>
            {t.library.notInstalled.connect}
          </button>
        </p>
      )}

      {lib && state === "connected" && (
        <>
          <p className="wishes__note">
            {epicSummary(lib)} {e.installHint}
          </p>
          {!lib.launcher && lib.games.length > 0 && <p className="wishes__note ni__warn">{e.noLauncher}</p>}
          {lib.games.length === 0 && <p className="wishes__note">{e.emptyTitle}. {e.emptyBody}</p>}
          {lib.games.length > 0 && (
            <ul className="wishes__list" aria-label={e.subtitle}>
              {lib.games.map((game) => (
                <li key={game.id} className="wish gog__row">
                  <div className="wish__main">
                    <Tile url={game.imageUrl} />
                    <div className="wish__text">
                      <span className="wish__title">{game.title}</span>
                    </div>
                  </div>
                  <div className="wish__actions">
                    <button
                      type="button"
                      className="btn-ghost"
                      data-nav
                      disabled={!lib.launcher}
                      onClick={() => install(game)}
                      aria-label={e.installLabel(game.title)}
                    >
                      {e.install}
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
