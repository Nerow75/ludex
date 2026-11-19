/** Icônes au trait : une seule taille (18) et une seule épaisseur (1.5) dans toute l'interface. */
const base = {
  width: 18,
  height: 18,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

export const RefreshIcon = () => (
  <svg {...base}><path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7" /></svg>
);
export const DownloadIcon = () => (
  <svg {...base}><path d="M12 4v11m-4.5-4.5L12 15l4.5-4.5M5 19.5h14" /></svg>
);
export const SearchIcon = () => (
  <svg {...base}><circle cx="11" cy="11" r="6.5" /><path d="m16 16 4 4" /></svg>
);
/** Marque Ludex : une touche de clavier avec le triangle « Jouer » (source : assets/logo/ludex-mark.svg). */
export const LudexMark = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden>
    <rect x="2" y="2" width="60" height="60" rx="14" fill="var(--lx-accent-deep)" />
    <rect x="7" y="5" width="50" height="46" rx="10" fill="var(--lx-accent)" />
    <path d="M37 11.5v17l14-8.5z" fill="var(--lx-on-accent)" stroke="var(--lx-on-accent)" strokeWidth="1.5" strokeLinejoin="round" />
  </svg>
);

/** Triangle plein : un glyphe, pas une icône au trait. */
export const PlayIcon = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden><path d="M4 2.5v11l9-5.5z" /></svg>
);
export const FolderIcon = () => (
  <svg {...base}><path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" /></svg>
);
const STAR = "m12 3.8 2.5 5.1 5.6.8-4 3.9 1 5.6-5.1-2.7-5 2.7.9-5.6-4-3.9 5.6-.8z";
/** `badge` : version 16px pour la pastille posée sur les jaquettes. */
export const StarIcon = ({ filled = false, badge = false }: { filled?: boolean; badge?: boolean }) => (
  <svg {...base} {...(badge && { width: 16, height: 16 })} fill={filled ? "currentColor" : "none"}><path d={STAR} /></svg>
);
export const EyeOffIcon = () => (
  <svg {...base}>
    <path d="M3 3l18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M9.4 5.2A9.6 9.6 0 0 1 12 5c5 0 8.5 4.5 9.5 7a13 13 0 0 1-2.6 3.7M6.2 6.6C4.3 7.9 3 9.8 2.5 12c1 2.5 4.5 7 9.5 7 1.6 0 3-.4 4.3-1.1" />
  </svg>
);
export const PlusIcon = () => (
  <svg {...base}><path d="M12 5v14M5 12h14" /></svg>
);
/** Fichier exécutable : « Changer l'exécutable ». */
export const FileIcon = () => (
  <svg {...base}><path d="M14 3H7.5A1.5 1.5 0 0 0 6 4.5v15A1.5 1.5 0 0 0 7.5 21h9a1.5 1.5 0 0 0 1.5-1.5V7z" /><path d="M14 3v4h4M10 12l-2 2 2 2M14 12l2 2-2 2" /></svg>
);
export const PencilIcon = () => (
  <svg {...base}><path d="M15.5 5.5l3 3M4 20l1-4L16.2 4.8a2.1 2.1 0 0 1 3 3L8 19z" /></svg>
);
export const TrashIcon = () => (
  <svg {...base}><path d="M4 7h16M10 11v6M14 11v6M6 7l1 12.5A1.5 1.5 0 0 0 8.5 21h7a1.5 1.5 0 0 0 1.5-1.5L18 7M9 7V4.5A1.5 1.5 0 0 1 10.5 3h3A1.5 1.5 0 0 1 15 4.5V7" /></svg>
);
export const CloseIcon = () => (
  <svg {...base}><path d="M6 6l12 12M18 6 6 18" /></svg>
);
export const GamepadIcon = () => (
  <svg {...base}>
    <path d="M7 9h4M9 7v4M15.5 9.5h.01M17.5 11.5h.01" />
    <path d="M6.5 5h11A4.5 4.5 0 0 1 22 9.5l-.6 6.2a2.6 2.6 0 0 1-4.6 1.4L15 15H9l-1.8 2.1a2.6 2.6 0 0 1-4.6-1.4L2 9.5A4.5 4.5 0 0 1 6.5 5z" />
  </svg>
);
export const ChevronLeftIcon = () => (
  <svg {...base}><path d="m15 5-7 7 7 7" /></svg>
);
export const ChevronRightIcon = () => (
  <svg {...base}><path d="m9 5 7 7-7 7" /></svg>
);
/** Capture d'écran : une image avec un paysage. */
export const ImageIcon = () => (
  <svg {...base}><rect x="3" y="4.5" width="18" height="15" rx="1.5" /><path d="m3 16 5-5 4 4 3-3 6 6" /><circle cx="15.5" cy="9" r="1.5" /></svg>
);
