import type { ReactNode } from "react";
import { GamepadIcon, SearchIcon } from "./Icons";

/** Squelette du premier scan : même géométrie que l'écran chargé, pour éviter les sauts. */
export function LibrarySkeleton() {
  return (
    <div className="skeleton" aria-busy="true" aria-label="Recherche de tes jeux">
      <div className="sk sk--line" style={{ width: 120 }} />
      <div className="resume__grid">
        {Array.from({ length: 4 }, (_, i) => <div key={i} className="sk sk--tile" />)}
      </div>
      <div className="sk sk--line" style={{ width: 160, margin: "28px 0 16px" }} />
      <div className="grid">
        {Array.from({ length: 16 }, (_, i) => (
          <div key={i} className="card">
            <div className="sk sk--cover" />
            <div className="sk sk--line" style={{ width: "80%" }} />
            <div className="sk sk--line" style={{ width: "45%" }} />
          </div>
        ))}
      </div>
    </div>
  );
}

interface EmptyProps {
  icon?: "search" | "gamepad";
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}

export function EmptyState({ icon = "gamepad", title, children, action }: EmptyProps) {
  return (
    <div className="empty" role="status">
      <span className="empty__icon">{icon === "search" ? <SearchIcon size={28} /> : <GamepadIcon />}</span>
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action}
    </div>
  );
}
