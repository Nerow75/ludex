# Privacy

[Français](PRIVACY.md) · **English**

Ludex is a local game launcher. It has no account, no server and no telemetry: no usage statistics, no crash report and no identifier of your PC is ever sent. This document lists everything Ludex contacts on the Internet and what it sends there. It is kept up to date from the code (version 0.2.0).

## What Ludex reads on your PC

To find your games, Ludex reads, read only, the files and registry keys of the installed launchers (Steam libraries, Epic manifests, the local databases of GOG Galaxy, Amazon Games and itch.io, Ubisoft, Rockstar and Xbox registry entries…), the "Jeux" or "Games" folders on your drives, the settings of installed emulators (game folders only) and known save folders. For emulated games, Ludex reads file names and, for a Switch game, a few header bytes (title number); never the rest of a ROM, nor BIOS, firmware or keys. None of this leaves your PC.

What Ludex writes stays in `%APPDATA%\com.ludex.app` (preferences, play history, caches, error log) and, for save backups, in the folder you pick (`Documents\Ludex\Sauvegardes` by default).

## Services contacted

Every request goes straight from your PC to the service, over HTTPS, without cookies or identifiers. Each service sees your IP address and Ludex's `User-Agent` header (`Ludex (lanceur de jeux)`, with the version number for HowLongToBeat, plain `Ludex` for the save database), as with any connection. Redirects are refused for images, HowLongToBeat, news, system requirements, achievements, mods and the wishlist; they are followed for updates (GitHub release files), the save database, the cover search and Steam game details.

### Ludex updates

| Host | When | Sent |
| --- | --- | --- |
| `github.com` (then the GitHub file server it redirects to) | 8 s after opening, then every 12 h until a version is found; the installer is only downloaded after you click "Install and restart" | Nothing: fixed address `releases/latest/download/latest.json`; the Tauri updater's `User-Agent` |

### Save database

| Host | When | Sent |
| --- | --- | --- |
| `raw.githubusercontent.com` | At most once a week, after a scan | Nothing: downloads the [Ludusavi](https://github.com/mtkennerly/ludusavi-manifest) manifest; an unchanged manifest isn't downloaded again (`If-None-Match` with the tag GitHub gave) |

### Emulated game covers

| Host | When | Sent |
| --- | --- | --- |
| `api.github.com` | After a scan, for each console with emulated games, once a month at most (no redirects) | The console's repository name at [libretro-thumbnails](https://github.com/libretro-thumbnails) ("Nintendo_-_Game_Boy_Advance"): reads the list of covers |
| `raw.githubusercontent.com` | For a game whose name matches a cover in the list | The image address (console and cover name) |

### Steam (no account)

| Host and address | When | Sent |
| --- | --- | --- |
| `store.steampowered.com/api/storesearch` | After a scan, for each non-Steam game without a cover | Game title, language, country `FR` |
| `store.steampowered.com/api/storesearch` | When you search for a game in the wishlist | The text you typed (one line, 100 characters at most), language, country `FR` |
| `store.steampowered.com/api/appdetails` | When a game's Overview tab opens (system requirements, kept 30 days) | The game's Steam number (appid), language |
| `store.steampowered.com/api/appdetails` | At most once a day (wishlist prices) | Steam numbers of the wishlist games, country `fr` |
| `api.steampowered.com/IStoreBrowseService/GetItems` | After a scan (game details, image paths), kept 30 days | Steam numbers of the games, language, country `FR` |
| `api.steampowered.com/IStoreService/GetTagList` | At most every 30 days | Language |
| `api.steampowered.com/ISteamNews/GetNewsForApp` | When a game page opens (if its news is over 6 h old), and once a day for "Playing" games | The game's Steam number |
| `api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp` | Showcase and achievement list, kept 7 days | The game's Steam number |
| `api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails` | Mods tab of a Steam game, kept 30 days | Numbers of the installed Workshop mods |

### Microsoft Store (no account)

| Host | When | Sent |
| --- | --- | --- |
| `displaycatalog.mp.microsoft.com` | After a scan, for a game without a cover that isn't on Steam | Game title, then the number of the product found; market `FR`, language `fr-fr` |

### HowLongToBeat

| Host | When | Sent |
| --- | --- | --- |
| `howlongtobeat.com` | After a scan, one request every 1.2 s, each title kept 30 days | A request for a search token (`init`), then the words of the game title; if the search address has changed, the home page and its scripts are read to find it again |

### Images

These addresses are built by Ludex from a Steam number or an image name; Ludex downloads some images to its cache, the interface shows others directly.

| Host | Content | Sent |
| --- | --- | --- |
| `cdn.cloudflare.steamstatic.com`, `shared.akamai.steamstatic.com` | Covers, background art, logos, achievement icons, wishlist images | The image address (with the game's Steam number in it) |
| `store-images.s-microsoft.com` | Covers and background art found in the Microsoft Store | The image address |
| `clan.akamai.steamstatic.com`, `clan.fastly.steamstatic.com` | Steam news images | The image address |
| `images.steamusercontent.com` | Workshop mod thumbnails | The thumbnail address |
| `raw.githubusercontent.com` | Emulated game covers (libretro-thumbnails) | The image address |

## Never sent

- Your Windows user name, your folder paths, your PC's name.
- Your Steam account identifier (read from disk to find your playtime, never transmitted).
- Your playtime, history, favorites, statuses, notes, collections, launch options.
- The contents of your files, saves or screenshots.

## Only sent if you do it

- **Titles**: the title sent to Steam, the Microsoft Store and HowLongToBeat is the one Ludex shows. For an "Other" game it comes from the executable or folder name; if you rename a game, the new title is used. Search aliases stay on your PC.
- **Links**: "Open the repository", "Read on Steam", "View in Steam" open your browser or Steam on an address set by Ludex.
- **Diagnostics report**: copied to the clipboard only when you click "Copy report". It is anonymized (folder and user name replaced by `%USERPROFILE%` and `%USERNAME%`, Steam account hidden); you decide who to send it to.
- **Data export**: a `.zip` archive written where you choose, never sent.
- **Discord status** (off by default): when on, Ludex talks to the Discord client on your PC through a local channel; Discord then shows your friends the game title, its launcher, the start time and the game's Steam image (or just "In game" if you hide the name). Ludex doesn't contact Discord's servers.

## Installer

The NSIS installer downloads WebView2 from Microsoft only if your PC lacks it (Windows 11 already has it). It sends nothing else.

## Contact

Questions or reports: [repository issues](https://github.com/Nerow75/ludex/issues).
