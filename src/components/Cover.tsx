import { useState } from "react";
import type { Game } from "../lib/api";
import { titleColor } from "../lib/color";

function monogram(title: string) {
  return title
    .replace(/[^\p{L}\p{N} ]/gu, "")
    .split(" ")
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0])
    .join("")
    .toUpperCase();
}

/** Jaquette officielle si disponible, sinon jaquette générée à partir du titre. */
export function Cover({ game, className = "" }: { game: Game; className?: string }) {
  // Échec mémorisé par adresse : une jaquette trouvée plus tard a sa chance.
  const [failed, setFailed] = useState<string | null>(null);
  const [iconFailed, setIconFailed] = useState(false);
  const showImage = game.coverUrl && failed !== game.coverUrl;
  const showIcon = !showImage && game.iconUrl && !iconFailed;

  return (
    <div
      className={`cover ${showImage ? "" : "cover--generated"} ${showIcon ? "cover--icon" : ""} ${className}`}
      style={{ ["--cover-tint" as string]: titleColor(game.title) }}
      data-mono={monogram(game.title)}
    >
      {showImage ? (
        <img src={game.coverUrl!} alt="" loading="lazy" draggable={false} onError={() => setFailed(game.coverUrl)} />
      ) : (
        <>
          {showIcon && <img className="cover__icon" src={game.iconUrl!} alt="" loading="lazy" draggable={false} onError={() => setIconFailed(true)} />}
          <span className="cover__title">{game.title}</span>
        </>
      )}
    </div>
  );
}
