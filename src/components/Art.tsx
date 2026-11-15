import { useState } from "react";
import type { Game } from "../lib/api";
import { titleColor } from "../lib/color";
import { cleanTitle } from "../lib/format";

/** Visuel large du jeu (library_hero), sinon dégradé teinté par le titre. */
export function Art({ game, className = "" }: { game: Game; className?: string }) {
  const [failed, setFailed] = useState<string | null>(null);
  const show = game.heroUrl && failed !== game.id;
  return (
    <div className={`art ${className}`} style={{ ["--art-tint" as string]: titleColor(game.title) }} aria-hidden>
      {show && <img key={game.id} src={game.heroUrl!} alt="" draggable={false} onError={() => setFailed(game.id)} />}
    </div>
  );
}

/** Logo officiel détouré, sinon le titre en typo display. */
export function Logo({ game, className = "" }: { game: Game; className?: string }) {
  const [failed, setFailed] = useState<string | null>(null);
  const show = game.logoUrl && failed !== game.id;
  return (
    <div className={`game-logo ${className}`}>
      {show ? (
        <img key={game.id} src={game.logoUrl!} alt={game.title} draggable={false} onError={() => setFailed(game.id)} />
      ) : (
        <span className="game-logo__text">{cleanTitle(game.title)}</span>
      )}
    </div>
  );
}
