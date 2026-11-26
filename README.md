# The Isle Version Manager

A small Windows x64 Tauri launcher for keeping multiple releases of The Isle and its dedicated server on disk.

## Run

Open **The Isle Version Manager.exe** from the portable build. Keep the adjacent `tools` directory with the executable. The launcher uses Microsoft Edge WebView2.

1. Choose a builds folder in **Settings** if you want to use another drive.
2. Under **Downloads**, add a release and use **Download** in the Game or Server column. **Resume** validates the files in a partial installation. For game downloads, sign in with a Steam account that owns The Isle.
3. **Play** lists completed game installations in the configured builds folder, including builds whose original profile was changed. With Steam running, click **Play** next to the instance you want.
4. Under **Dedicated server**, click **Configure** for the installed server. Edit its `Game.ini`, or enter your SteamID64 and click **Add admin to config**, then **Save configuration**. **Use my Steam ID** fills in the account currently active in the local Steam client when available. Stop the server before editing and start it again to apply your changes. **Arguments** sets per-profile launch options. Profiles using the same server manifest share its installed files and configuration. [Evrima server settings](https://steamcommunity.com/sharedfiles/filedetails/?id=2952501611) · [Developer’s Legacy admin instructions](https://steamcommunity.com/app/376210/discussions/16/1814296907961200359/)

In **Add version**, pick a release to fill in its branch and Windows manifest IDs. The launcher looks up current branches through the public SteamCMD API and fills the server ID when the same branch publishes one. **Refresh** checks for new builds. Previously loaded releases and a small selection of older builds remain available offline. [SteamCMD API](https://www.steamcmd.net/) · [Historical game manifest source](https://www.reddit.com/r/theisle/comments/1qgj053/access_to_all_old_isle_maps_except_v4/)

For other releases, **Browse game history** and **Browse server history** open SteamDB inside the launcher. Paste the ID or the copied Steam console command into the corresponding field, and choose the server build for the same release. Steam controls which historical manifests are downloadable. [Windows game depot](https://steamdb.info/depot/376212/) · [Windows server depot](https://steamdb.info/depot/412681/)

**Evrima · Isla Spiro updates** contains 66 Windows game builds and 10 additional dedicated-server revisions, from the June 2020 launch through October 2023. All 29 named releases have both manifest IDs. Builds without an announced version number use their UTC timestamp. Game and server manifests are paired by their branch rollout times. Entries marked **server only** identify additional server revisions. Only the final build uses the preserved `spiro0.11.59.04` branch. The catalog is bundled and works offline. [Official Steam news](https://store.steampowered.com/news/app/376210) · [Windows game history](https://steamdb.info/depot/376212/manifests/) · [Windows server history](https://steamdb.info/depot/412681/manifests/)

The January 2021 patch and Update 6.5 were first recorded on testing branches. Their public Evrima promotions confirm the game IDs used here. [January 2021 promotion](https://steamdb.info/app/376210/history/?changeid=10585954) · [Update 6.5 promotion](https://steamdb.info/app/376210/history/?changeid=19281731)

If a saved Spiro release has one matching ID, the launcher fills the missing ID. If a saved ID differs, select **Use preset manifests** to update the selected entry. Existing downloads stay in their manifest folders. For historical Spiro servers the download window requests a Steam sign-in. Steam decides whether the signed-in account can fetch each older manifest.

## Storage and downloads

For older builds with an empty in-game server list, start the server and choose **Dedicated server → Steam Favorites**. The launcher checks the server’s Steam query address and reads its advertised game port. Choose **Add to Steam Favorites**, then launch the matching client from **Play**. For a server on this PC, the query address usually starts with `127.0.0.1`. On another PC, use the server’s reachable IPv4 address. Steam also lists the entry under **View → Game Servers → Favorites**. [Steam matchmaking favorites](https://partner.steamgames.com/doc/api/ISteamMatchmaking#AddFavoriteGame)

By default the launcher saves its settings and builds under `%LOCALAPPDATA%\local.theisle.versionmanager`. Each component has its own directory under `builds\game\<manifest>` or `builds\server\<manifest>`. Multiple profiles referencing the same component manifest reuse that installation. Steam's normal installation is separate. To use an isolated settings directory, set `ISLE_MANAGER_HOME` before starting the launcher.

The download window shows Steam authentication and transfer progress. You can close the launcher while a download continues, or close the download window to interrupt it. **Resume download** checks existing files. An installation becomes launchable after DepotDownloader succeeds and the expected executable exists.

## Build from source

Install stable Rust for Windows MSVC, Microsoft C++ Build Tools with the Windows SDK, and WebView2. Build the portable release from PowerShell at the repository root. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

```powershell
.\build.ps1
```

Output is in `dist\The Isle Version Manager\`. The script verifies the Windows x64 DepotDownloader archive against its published SHA-256 before extracting it. [Upstream release](https://github.com/SteamRE/DepotDownloader/releases/tag/DepotDownloader_3.4.0) · [WinGet checksum](https://github.com/microsoft/winget-pkgs/blob/master/manifests/s/SteamRE/DepotDownloader/3.4.0/SteamRE.DepotDownloader.installer.yaml)

For development, use the commands below. The frontend is plain HTML, CSS, and JavaScript embedded by Tauri. A development build can find DepotDownloader on PATH or through its executable path in Settings.

```powershell
cargo run --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
```
