import { useState } from "react";
import { t } from "../lib/i18n";
import { markFocus, restoreFocus } from "./focus";

interface Props {
  initial?: string;
  maxLength: number;
  label: string;
  placeholder?: string;
  className?: string;
  /** Texte validé par Entrée (le nettoyage final est fait côté Rust). */
  onSubmit: (value: string) => void;
  /** Échap, clic ailleurs, ou après `onSubmit`. */
  onDone: () => void;
}

/** Champ sur place, comme « Renommer » : Entrée valide, Échap (ou un clic ailleurs) annule.
 * Fermé au clavier, il rend le focus au bouton qui l'a ouvert (sinon le focus tombait sur la page). */
export function InlineInput({ initial = "", maxLength, label, placeholder, className, onSubmit, onDone }: Props) {
  const [value, setValue] = useState(initial);
  // Bouton qui avait le focus quand le champ s'est ouvert (le champ prend le focus juste après).
  const [opener] = useState(markFocus);
  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation(); // pas de raccourci global (Ctrl+D, flèches) pendant la saisie
    if (e.key === "Enter") {
      e.preventDefault();
      if (value.trim() !== initial.trim()) onSubmit(value);
      onDone();
      restoreFocus(opener);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone();
      restoreFocus(opener);
    }
  };
  return (
    <input
      className={`inline-input ${className ?? ""}`}
      value={value}
      maxLength={maxLength}
      autoFocus
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={onKeyDown}
      onBlur={onDone}
      aria-label={label}
      placeholder={placeholder}
      title={t.common.inlineInputHint}
      spellCheck={false}
      autoComplete="off"
    />
  );
}
