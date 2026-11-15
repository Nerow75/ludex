# Ludex

Tous tes jeux, un seul launcher. Ludex détecte les jeux installés via Steam, Epic Games et Riot, puis les lance par leur launcher d'origine.

## Stack

- Tauri 2 (Rust) : détection des jeux et lancement, dans `src-tauri/src/library/`
- React + TypeScript + Vite : interface, dans `src/`
- Design system : `src/styles/tokens.css`

## Développement

```bash
npm install
npm run tauri dev
```

Scan réel de la machine, sans interface :

```bash
cd src-tauri
cargo test scan_this_pc -- --ignored --nocapture
```
