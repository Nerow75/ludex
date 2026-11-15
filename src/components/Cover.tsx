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
  const [failed, setFailed] = useState(false);
  const showImage = game.coverUrl && !failed;

  return (
    <div
      className={`cover ${showImage ? "" : "cover--generated"} ${className}`}
      style={{ ["--cover-tint" as string]: titleColor(game.title) }}
      data-mono={monogram(game.title)}
    >
      {showImage ? (
        <img src={game.coverUrl!} alt="" loading="lazy" draggable={false} onError={() => setFailed(true)} />
      ) : (
        <span className="cover__title">{game.title}</span>
      )}
    </div>
  );
}
