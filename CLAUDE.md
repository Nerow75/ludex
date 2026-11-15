# Ludex

Launcher hub pour Windows : détecte les jeux installés de tous les launchers (Steam, Epic, Riot pour l'instant) et les lance depuis une seule interface. Objectif : plus moderne et plus beau que Playnite ou GOG Galaxy.

## Stack

- Tauri 2 (Rust, edition 2024) dans `src-tauri/`
- React 19 + TypeScript + Vite dans `src/`
- Polices embarquées via @fontsource-variable : Bricolage Grotesque (display), Geist (texte), Geist Mono (mono)

## Commandes

- `npm run tauri dev` : lance l'app
- `npx tsc --noEmit` puis `npm run build` : vérif front
- `cd src-tauri && cargo test` : tests Rust
- `cd src-tauri && cargo test scan_this_pc -- --ignored --nocapture` : scan réel du PC

## Architecture

- `src-tauri/src/library/` : une source par launcher (`steam.rs`, `epic.rs`, `riot.rs`), lecture de fichiers locaux uniquement, aucune connexion à un compte. `vdf.rs` parse le format Valve. `mod.rs` définit `Game`, `Launch`, `ScanResult` et agrège les sources (`Ok(None)` = launcher absent).
- `src-tauri/src/lib.rs` : commandes Tauri `scan_library`, `launch_game`, `open_install_dir`. Les jeux scannés sont gardés dans `AppState`.
- **Sécurité** : le front n'envoie qu'un id de jeu. La commande de lancement et le dossier sont reconstruits côté Rust depuis l'état. Ne jamais exposer de commande qui prend un chemin ou une URI arbitraire venant du front.
- `src/lib/api.ts` : types et appels `invoke`. `src/lib/color.ts` : couleur d'ambiance extraite de la jaquette (fallback : hash du titre).
- `src/components/` : Cover, Hero, CommandPalette (Ctrl+K), Icons.
- `src/App.tsx` : écran unique (rail de navigation, hero du jeu sélectionné, filtres par plateforme, grille de jaquettes).

## Design system

- Tokens dans `src/styles/tokens.css` (préfixe `--lx-*`), styles dans `src/styles/app.css`.
- Palette validée : base monochrome (#0B0B0B, #1A1A1A, #242424, #333333, blanc) + accent turquoise #14B8A6. Thème clair via `[data-theme="light"]`. D'autres thèmes viendront plus tard : toujours passer par les tokens, jamais de couleur en dur.
- `--lx-ambient` : couleur dérivée du jeu sélectionné, utilisée par le hero et le bouton Jouer.

## État actuel

- Testé sur le PC de l'utilisateur : Steam 69 jeux, Riot 2, Epic absent (Epic testé seulement en unitaire).
- Problèmes connus : Wallpaper Engine apparaît (appli Steam, pas un jeu). Amis et Succès sont des onglets désactivés. Pas de CSP (`null` dans tauri.conf.json).
- Prochaine étape : refonte UI/UX.

## Conventions

- Interface, messages d'erreur et commentaires en français.
- Ne jamais utiliser le tiret cadratin dans les textes.
