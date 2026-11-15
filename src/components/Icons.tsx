const base = {
  width: 20,
  height: 20,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

export const HomeIcon = () => (
  <svg {...base}><path d="M4 10.5 12 4l8 6.5V20h-5v-5H9v5H4z" /></svg>
);
export const FriendsIcon = () => (
  <svg {...base}>
    <circle cx="9" cy="8.5" r="3.5" />
    <path d="M2.5 20c.6-3.6 3.2-5.5 6.5-5.5s5.9 1.9 6.5 5.5M15.5 5.2a3.4 3.4 0 0 1 0 6.6M18 14.8c2 .8 3.2 2.5 3.5 5.2" />
  </svg>
);
export const TrophyIcon = () => (
  <svg {...base}>
    <path d="M8 4h8v5a4 4 0 0 1-8 0z" />
    <path d="M8 6H5a3 3 0 0 0 3 4M16 6h3a3 3 0 0 1-3 4M12 13v4M8.5 20h7" />
  </svg>
);
export const RefreshIcon = () => (
  <svg {...base}><path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7" /></svg>
);
export const SearchIcon = () => (
  <svg {...base} width={16} height={16} strokeWidth={2}><circle cx="11" cy="11" r="6.5" /><path d="m16 16 4 4" /></svg>
);
export const PlayIcon = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden><path d="M4 2.5v11l9-5.5z" /></svg>
);
export const FolderIcon = () => (
  <svg {...base} width={16} height={16}><path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" /></svg>
);
