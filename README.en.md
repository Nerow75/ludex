# Ludex

[Français](README.md) · **English**

Ludex brings the games installed on your Windows PC into a single library, whatever their launcher, and starts them with one click or one controller button. It reads what the launchers already keep on disk: no account to link, no password, nothing to set up.

<!-- Planned screenshot: docs/captures/pour-toi.png ("For you" home) -->

## Principles

- **Local reading**: Ludex reads the files and registry entries of the installed launchers, read only. A game is always started by its own launcher (or by its executable when it has none).
- **No account**: no sign-in to Steam, Epic, GOG or anything else. Ludex never asks for a login or a token.
- **No telemetry**: no usage statistics, no crash report sent. The few public services contacted (images, game details, time to beat, updates) are listed below, with what is sent to them.
- **Your data stays with you**: favorites, collections, notes, measured playtime and settings live in `%APPDATA%\com.ludex.app`. You can export them to an archive to move to another PC.

## Supported launchers

| Source | What Ludex reads |
| --- | --- |
| Steam | Steam libraries, playtime, achievements, screenshots, collections, Workshop mods |
| Epic Games | Manifests of installed games |
| GOG | Registry, GOG Galaxy playtime |
| Ubisoft Connect | Registry |
| EA app | Game folders (install manifests) |
| Battle.net | Game folders |
| Rockstar Games Launcher | Registry |
| Xbox and Microsoft Store | Installed game packages |
| Riot Games | Riot Client files |
| Amazon Games | The app's local database (read only) |
| itch.io | The app's local database (read only) |
| Other | "Jeux" or "Games" folders on your drives, games added by hand, FiveM, RedM, Roblox, Ankama games… |

## Main features

- Library as a grid or a list, sorting, filters by launcher, genre, tag and mood, typo-tolerant search ("gta5", "fm20", "rocet").
- "For you" home: tonight's game, games to resume, ideas by mood, backlog of games never started.
- A page for every game: playtime, sessions, Steam achievements, screenshots, saves, system requirements compared with your PC, news, mods, performance.
- Playtime measured by Ludex for launchers that don't keep it (Epic, Ubisoft, Riot, Xbox…), history, journal, weekly, monthly and yearly recap, stats.
- Automatic save backups at the end of a session (locations from the community Ludusavi database) and restore.
- Couch mode for the TV, fully usable with a controller.
- Quick search from anywhere (Alt+Space by default), tray icon, Jump List and desktop shortcuts.
- Storage: space used by games on each drive and games to uninstall.
- Steam wishlist with sale alerts, without an account.
- Dark, light, OLED black and high contrast themes, five accent colors, interface in English and French.

<!-- Planned screenshots: docs/captures/bibliotheque.png, docs/captures/fiche.png, docs/captures/mode-canape.png -->

## Installation

1. Download `Ludex_<version>_x64-setup.exe` from the [latest release](https://github.com/Nerow75/ludex/releases/latest).
2. Run it. Ludex installs for your Windows account only (in `%LOCALAPPDATA%\Ludex`), without administrator rights.
3. **SmartScreen**: the installer isn't signed yet, so Windows may show "Windows protected your PC". Click **More info**, then **Run anyway**. The warning fades as the file gets downloaded more.

Requirements: 64-bit Windows 10 or 11, with WebView2 (built into Windows 11, downloaded from Microsoft by the installer if missing).

### Updates

Ludex checks for new versions 8 seconds after it opens, then every 12 hours. When one is available, a message offers "Install and restart": the installer is downloaded from the GitHub releases, its signature is checked, then Ludex restarts. Nothing is installed until you click.

## Shortcuts

### Keyboard

| Key | Action |
| --- | --- |
| Type anywhere, Ctrl+K | Search |
| Arrows | Move |
| Enter | Play |
| Space | Open or close the game page |
| Esc | Close, clear the search |
| Ctrl+D | Favorite |
| Ctrl+N | Add a game |
| Menu key, Shift+F10 | Game menu |
| Ctrl+1 to Ctrl+9 | Switch section |
| Ctrl+, | Settings |
| F5 | Rescan the launchers |
| F11 | Couch mode |
| Alt+Space | Quick search, even with Ludex in the background (can be changed) |

### Controller

| Button | Action |
| --- | --- |
| D-pad, left stick | Move |
| A | Play |
| X | Game page |
| Y | Favorite |
| B | Back |
| LB, RB | Previous or next section |
| LT, RT | Previous or next filter |
| View | Next sort |
| Menu | Game menu |
| View + Menu | Couch mode |

## Privacy

Ludex creates no account and sends no telemetry. It only contacts the public services below, without any identifier, always straight from your PC (no Ludex server in between). Each service sees your IP address, as with any connection. The request-by-request detail is in [PRIVACY.en.md](PRIVACY.en.md).

| Service | Why | What is sent |
| --- | --- | --- |
| `github.com` | Ludex updates | Nothing (reads the latest version, then downloads the installer) |
| `raw.githubusercontent.com` | Ludusavi save database | Nothing (downloads the list, once a week at most) |
| `store.steampowered.com` | Covers, system requirements, wishlist | Titles of games that need artwork, Steam numbers (appids), text searched in the wishlist |
| `api.steampowered.com` | Game details, tags, news, achievement rarity, mods | Steam numbers of games and mods, language |
| `displaycatalog.mp.microsoft.com` | Covers of games not on Steam | Titles of those games |
| `howlongtobeat.com` | Time to beat | Game titles |
| Steam and Microsoft Store image CDNs | Covers, artwork, thumbnails | The address of each image |

Never sent: Windows user name, folder paths, Steam account identifier, playtime, favorites, notes, collections, contents of your files. The error log and the diagnostics report stay on your PC (the report is only copied when you click "Copy report", and it is anonymized).

## Development

Requirements: Node.js (LTS), stable Rust (2024 edition), and the [Tauri prerequisites for Windows](https://tauri.app/start/prerequisites/).

```powershell
npm install
npm run tauri dev
```

Checks (the same as CI):

```powershell
npx tsc --noEmit
npm test
npm run build
cd src-tauri
cargo clippy --all-targets -- -D warnings
cargo test
```

Architecture and conventions: see [CLAUDE.md](CLAUDE.md) (in French). Stack: Tauri 2 (Rust) in `src-tauri/`, React 19 and TypeScript in `src/`.

## License

Copyright © 2026 NRW.

Ludex is free software released under the GNU General Public License (GPL), version 3 or (at your option) any later version. Full text in [LICENSE](LICENSE).

## Notices

Ludex is an independent project. It isn't affiliated with any of the launchers or services mentioned (Valve, Epic Games, GOG, Ubisoft, Electronic Arts, Blizzard, Rockstar Games, Microsoft, Riot Games, Amazon, itch.io, HowLongToBeat, Discord…). Their names, logos and trademarks belong to their respective owners and are only used to show where each game comes from.

Launcher logos from [Simple Icons](https://simpleicons.org) (CC0) when available there. Save locations: [Ludusavi Manifest](https://github.com/mtkennerly/ludusavi-manifest). Time to beat: [HowLongToBeat](https://howlongtobeat.com).
