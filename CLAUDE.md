# Ludex

Launcher hub pour Windows : détecte les jeux installés de tous les launchers (Steam, Epic, Riot pour l'instant) et les lance depuis une seule interface. Objectif : plus moderne et plus beau que Playnite ou GOG Galaxy.

## Stack

- Tauri 2 (Rust, edition 2024) dans `src-tauri/`
- React 19 + TypeScript + Vite dans `src/`
- Polices embarquées via @fontsource-variable : Geist (interface), Geist Mono (chiffres, méta, touches)

## Commandes

- `npm run tauri dev` : lance l'app
- `npx tsc --noEmit` puis `npm run build` : vérif front
- `cd src-tauri && cargo test` : tests Rust
- `cd src-tauri && cargo test scan_this_pc -- --ignored --nocapture` : scan réel du PC

## Architecture

- `src-tauri/src/library/` : une source par launcher (`steam.rs`, `epic.rs`, `riot.rs`), lecture de fichiers locaux uniquement, aucune connexion à un compte. `vdf.rs` parse le format Valve. `mod.rs` définit `Game`, `Launch`, `ScanResult` et agrège les sources (`Ok(None)` = launcher absent).
- `src-tauri/src/lib.rs` : commandes Tauri `scan_library`, `launch_game` (renvoie l'heure du lancement), `open_install_dir`, `set_favorite`, `set_hidden`. Jeux scannés et préférences gardés dans `AppState`.
- `src-tauri/src/store.rs` : préférences (favoris, masqués, outils ré-affichés, dernier lancement Ludex) dans `user-data.json` du dossier de données de l'app (`%APPDATA%/com.ludex.app`).
- Steam fournit `LastPlayed` et `SizeOnDisk`, Epic `InstallSize`. Les outils (Wallpaper Engine, `* Editor`, `* Dedicated Server`…) sont `is_tool` : masqués par défaut, jamais exclus.
- **Sécurité** : le front n'envoie qu'un id de jeu. La commande de lancement et le dossier sont reconstruits côté Rust depuis l'état. Ne jamais exposer de commande qui prend un chemin ou une URI arbitraire venant du front.
- `src/lib/api.ts` : types et appels `invoke`. `src/lib/color.ts` : couleur d'ambiance extraite de la jaquette (fallback : hash du titre).
- `src/lib/library.ts` : filtres, tri, récents, préférences d'affichage (localStorage). `src/lib/format.ts` : dates relatives, tailles, recherche.
- `src/components/` : GameCard, LibraryGrid (colonnes mesurées, insère la fiche sous la rangée), GameStrip (fiche dépliable), ResumeRow, Art et Logo, Cover, States, Icons.
- `src/App.tsx` : écran unique « Focus » : grand champ de recherche, Reprendre, filtres et tri, grille, barre de raccourcis (taper n'importe où, flèches, Entrée, Espace, Échap, Ctrl+D, Ctrl+K, F5).

## Design system

- Tokens dans `src/styles/tokens.css` (préfixe `--lx-*`), styles dans `src/styles/app.css`.
- Palette validée : base monochrome (#0B0B0B, #1A1A1A, #242424, #333333, blanc) + accent turquoise #14B8A6. Thème clair via `[data-theme="light"]`. D'autres thèmes viendront plus tard : toujours passer par les tokens, jamais de couleur en dur.
- `--lx-ambient` : couleur dérivée du jeu sélectionné, réservée au contour de sélection. Bouton Jouer en blanc cassé (`--lx-text` sur `--lx-bg`).
- Passe unslop appliquée : grille de 4px, icônes 18px trait 1.5, graisses 400 à 600, pas de majuscules espacées, chiffres tabulaires, rayons s/m/l = 8/10/12, une seule courbe `--lx-ease`, survol sans déplacement (contour ou voile qui s'éclaircit), léger `scale` au clic.

## État actuel

- Testé sur le PC de l'utilisateur : Steam 69 jeux, Riot 2, Epic absent (Epic testé seulement en unitaire).
- Refonte UI « Focus » faite (branche `refonte-ui-focus`). Audit et maquettes : `maquettes/refonte-ui.html` (non versionné).
- Problèmes connus : jeux Steam récents (MECCHA CHAMELEON, NBA 2K26, LEGO Batman…) sans jaquette sur l'URL classique, jaquette générée à la place (piste : cache local `Steam/appcache/librarycache`). Pas de CSP. Amis et Succès pas commencés.

## Conventions

- Interface, messages d'erreur et commentaires en français.
- Ne jamais utiliser le tiret cadratin dans les textes.
