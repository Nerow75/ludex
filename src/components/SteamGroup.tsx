import { useCallback, useEffect, useState } from "react";
import { onSteamKey, steamInstall, steamOwned, type SteamGame, type SteamOwnedLibrary } from "../lib/api";
import { t } from "../lib/i18n";
import { steamNote, steamSummary, storeName } from "../lib/notInstalled";
import type { SectionContext } from "../lib/sections";
import { Tile } from "./GogView";
import { LauncherIcon } from "./LauncherIcon";

/**
 * Groupe Steam de la section « Pas installés » : jeux Steam possédés qui ne sont pas sur ce PC.
 * Sans clé, liste locale (jeux déjà lancés sur le compte de ce PC) ; avec une clé Web API
 * enregistrée dans les Paramètres, liste complète. « Installer dans Steam » ouvre Steam sur le jeu
 * (`steam://install/<appid>` reconstruite côté Rust) ; Ludex ne télécharge rien pour Steam.
 * `onCount(null)` : Steam absent de ce PC.
 */
export function SteamGroup({ ctx, onCount }: { ctx: SectionContext; onCount?: (n: number | null) => void }) {
  const [lib, setLib] = useState<SteamOwnedLibrary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const st = t.library.steam;
  // Un jeu installé ou désinstallé change la liste.
  const steamCount = ctx.games.filter((x) => x.platform === "steam").length;

  const load = useCallback((refresh: boolean) => {
    setBusy(true);
    return steamOwned(refresh)
      .then((l) => {
        setLib(l);
        setError(l.error);
      })
      .catch((err) => setError(String(err)))
      .finally(() => setBusy(false));
  }, []);

  useEffect(() => {
    load(false);
  }, [load, steamCount]);

  useEffect(() => {
    const off = onSteamKey(() => load(false));
    return () => {
      off.then((stop) => stop()).catch(() => {});
    };
  }, [load]);

  useEffect(() => {
    if (lib) onCount?.(lib.steam ? lib.games.length : null);
  }, [lib, onCount]);

  const install = async (game: SteamGame) => {
    setError(null);
    setInfo(null);
    try {
      await steamInstall(game.id);
      setInfo(st.opened(game.title));
    } catch (err) {
      setError(String(err));
    }
  };

  const note = lib?.steam ? steamNote(lib) : null;
  return (
    <section className="ni__group" aria-labelledby="ni-steam">
      <div className="ni__head">
        <h3 id="ni-steam" className="with-icon">
          <LauncherIcon platform="steam" size={18} />
          {storeName("steam")}
          {lib?.steam && (
            <span className="ni__count">
              <span className="sr-only">, </span>
              {lib.games.length}
            </span>
          )}
        </h3>
        {lib?.steam && (
          <button type="button" className="btn-ghost" data-nav disabled={busy} onClick={() => load(true)}>
            {busy ? st.refreshing : t.library.gog.refresh}
          </button>
        )}
      </div>

      {!lib && !error && <p className="wishes__note" role="status">{st.loading}</p>}
      {error && <p className="wishes__msg" role="alert">{error}</p>}
      {info && <p className="wishes__note" role="status">{info}</p>}
      {lib && !lib.steam && <p className="wishes__note ni__off">{st.absent}</p>}

      {note && (
        <p className={`wishes__note ni__off${note === "refused" ? " ni__warn" : ""}`}>
          {note === "refused" ? st.keyRefused : st.localNote}{" "}
          <button type="button" className="btn-ghost set__small" data-nav onClick={ctx.openSettings}>
            {note === "refused" ? st.fixKey : st.addKey}
          </button>
        </p>
      )}

      {lib?.steam && (
        <>
          <p className="wishes__note">
            {steamSummary(lib)} {lib.games.length > 0 && st.installHint}
          </p>
          {lib.games.length > 0 && (
            <ul className="wishes__list" aria-label={st.subtitle}>
              {lib.games.map((game) => (
                <li key={game.id} className="wish gog__row">
                  <div className="wish__main">
                    <Tile url={game.imageUrl} />
                    <div className="wish__text">
                      <span className="wish__title">{game.title}</span>
                    </div>
                  </div>
                  <div className="wish__actions">
                    <button type="button" className="btn-ghost" data-nav onClick={() => install(game)} aria-label={st.installLabel(game.title)}>
                      {st.install}
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
