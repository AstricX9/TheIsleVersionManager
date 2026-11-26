#![windows_subsystem = "windows"]

mod download;
mod favorites;
mod library;
mod server_config;

use download::{resolve_downloader, Download};
use library::{spiro_catalog, status, Component, Library, Result, SpiroPreset, Status, Version};
use serde::Serialize;
use std::{
    env, fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::{AppHandle, Manager};

fn home(app: &AppHandle) -> Result<PathBuf> {
    let folder = match env::var_os("ISLE_MANAGER_HOME") {
        Some(path) => PathBuf::from(path),
        None => app.path().app_local_data_dir().map_err(|e| e.to_string())?,
    };
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok(folder)
}

fn update(app: &AppHandle, change: impl FnOnce(&mut Library) -> Result<()>) -> Result<()> {
    let home = home(app)?;
    let lock = fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .read(true)
        .open(home.join("library.lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let mut library = Library::load(&home)?;
    change(&mut library)?;
    library.save(&home)
}

#[derive(Serialize)]
struct Snapshot {
    library: Library,
    games: Vec<library::Installation>,
    servers: Vec<library::Installation>,
    statuses: Vec<[Status; 2]>,
    tool_available: bool,
}

#[tauri::command]
fn snapshot(app: AppHandle) -> Result<Snapshot> {
    let library = Library::load(&home(&app)?)?;
    let mut statuses = Vec::new();
    for version in &library.versions {
        let game = if version.game.is_some() {
            status(
                &library.directory(version, Component::Game)?,
                Component::Game,
            )?
        } else {
            Status::Unavailable
        };
        let server = if version.server.is_some() {
            status(
                &library.directory(version, Component::Server)?,
                Component::Server,
            )?
        } else {
            Status::Unavailable
        };
        statuses.push([game, server]);
    }
    let tool_available = resolve_downloader(&library.downloader).is_ok();
    Ok(Snapshot {
        games: library.installations(Component::Game)?,
        servers: library.installations(Component::Server)?,
        library,
        statuses,
        tool_available,
    })
}

fn version_at(library: &Library, index: usize) -> Result<&Version> {
    library
        .versions
        .get(index)
        .ok_or_else(|| "This version is missing. Reopen the library.".into())
}

#[tauri::command]
fn spiro_presets() -> &'static [SpiroPreset] {
    spiro_catalog()
}

#[tauri::command]
fn use_preset_manifests(app: AppHandle, index: usize, name: String) -> Result<()> {
    update(&app, |lib| {
        let version = lib
            .versions
            .get_mut(index)
            .filter(|v| v.name == name)
            .ok_or("The selected version has changed. Select it again.")?;
        let preset = version.spiro_preset().ok_or("No matching Spiro preset.")?;
        version.game.clone_from(&preset.game);
        version.server.clone_from(&preset.server);
        Ok(())
    })
}

#[tauri::command]
fn select_version(app: AppHandle, index: usize) -> Result<()> {
    update(&app, |lib| {
        version_at(lib, index)?;
        lib.selected = index;
        Ok(())
    })
}

#[tauri::command]
fn add_version(
    app: AppHandle,
    name: String,
    branch: String,
    game: Option<String>,
    server: Option<String>,
) -> Result<()> {
    let version = Version {
        name: name.trim().into(),
        branch,
        game,
        server,
        arguments: "-log".into(),
    };
    version.validate()?;
    update(&app, |lib| {
        if lib
            .versions
            .iter()
            .any(|v| v.name.eq_ignore_ascii_case(&version.name))
        {
            return Err("A version already has that name.".into());
        }
        lib.selected = lib.versions.len();
        lib.versions.push(version);
        Ok(())
    })
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    folder: PathBuf,
    downloader: String,
    username: String,
) -> Result<()> {
    if !folder.is_absolute() {
        return Err("Enter an absolute path for the builds folder.".into());
    }
    if !downloader.is_empty() {
        resolve_downloader(&downloader)?;
    }
    if username.starts_with('-') || username.chars().any(char::is_whitespace) {
        return Err("Enter a Steam account name without spaces.".into());
    }
    fs::create_dir_all(&folder).map_err(|e| format!("Cannot use this folder: {e}"))?;
    update(&app, |lib| {
        lib.folder = folder;
        lib.downloader = downloader;
        lib.username = username;
        Ok(())
    })
}

#[tauri::command]
fn save_arguments(app: AppHandle, index: usize, arguments: String) -> Result<()> {
    update(&app, |lib| {
        let version = lib.versions.get_mut(index).ok_or("Version is missing.")?;
        version.arguments = arguments;
        Ok(())
    })
}

#[tauri::command]
async fn download_build(
    app: AppHandle,
    index: usize,
    component: Component,
    manifest: Option<String>,
) -> Result<()> {
    let lib = Library::load(&home(&app)?)?;
    let version = version_at(&lib, index)?;
    let id = version.id(component)?;
    if manifest.as_deref().is_some_and(|expected| expected != id) {
        return Err("The release changed. Refresh Downloads and try again.".into());
    }
    let request = Download {
        directory: lib.directory(version, component)?,
        component,
        branch: version.branch.clone(),
        manifest: id.into(),
        tool: resolve_downloader(&lib.downloader)?,
        username: lib.username,
    };
    let payload = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let exit = Command::new(env::current_exe().map_err(|e| e.to_string())?)
            .args(["--install", &payload])
            .status()
            .map_err(|e| e.to_string())?;
        if exit.success() {
            Ok(())
        } else {
            Err(
                "Download interrupted or failed. Retry to resume the files already downloaded."
                    .into(),
            )
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn installed_directory(lib: &Library, component: Component, id: &str) -> Result<PathBuf> {
    let dir = lib
        .folder
        .join(component.name())
        .join(library::manifest(id)?);
    if status(&dir, component)? != Status::Installed {
        return Err("Download this version first.".into());
    }
    Ok(dir)
}

#[tauri::command]
fn launch(
    app: AppHandle,
    index: Option<usize>,
    component: Component,
    manifest: Option<String>,
) -> Result<()> {
    let lib = Library::load(&home(&app)?)?;
    let version = index.map(|i| version_at(&lib, i)).transpose()?;
    let profile_id = version.map(|v| v.id(component)).transpose()?;
    if manifest.is_some() && profile_id.is_some() && manifest.as_deref() != profile_id {
        return Err("The release changed. Refresh the list and try again.".into());
    }
    let id = manifest
        .as_deref()
        .or(profile_id)
        .ok_or("Select an installation.")?;
    let dir = installed_directory(&lib, component, id)?;
    let mut command = Command::new(dir.join(component.executable()));
    command.current_dir(&dir);
    match component {
        Component::Game => {
            command
                .env("SteamAppId", "376210")
                .env("SteamGameId", "376210");
        }
        Component::Server => {
            command.args(
                version
                    .map_or("-log", |v| v.arguments.as_str())
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
            );
            command.creation_flags(0x00000010); // CREATE_NEW_CONSOLE
        }
    }
    command
        .spawn()
        .map_err(|e| format!("Cannot launch {}: {e}", component.executable()))?;
    Ok(())
}

#[tauri::command]
fn open_installation(
    app: AppHandle,
    index: Option<usize>,
    component: Component,
    manifest: Option<String>,
) -> Result<()> {
    let lib = Library::load(&home(&app)?)?;
    let id = match manifest.as_deref() {
        Some(id) => id,
        None => version_at(&lib, index.ok_or("Select an installation.")?)?.id(component)?,
    };
    let dir = installed_directory(&lib, component, id)?;
    // Supply a directory argument directly to Explorer.
    let windows = env::var_os("WINDIR").ok_or("Cannot locate Windows Explorer.")?;
    Command::new(Path::new(&windows).join("explorer.exe"))
        .arg(dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn open_history(app: AppHandle, component: Component) -> Result<()> {
    let label = format!("{}-history", component.name());
    let url = tauri::Url::parse(component.history()).map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window(&label) {
        window.navigate(url).map_err(|e| e.to_string())?;
        window.unminimize().map_err(|e| e.to_string())?;
        return window.set_focus().map_err(|e| e.to_string());
    }
    let parent = app
        .get_webview_window("main")
        .ok_or("Launcher window is missing.")?;
    tauri::WebviewWindowBuilder::new(&app, label, tauri::WebviewUrl::External(url))
        .title(format!(
            "SteamDB · The Isle {} manifest history",
            component.name()
        ))
        .inner_size(940.0, 720.0)
        .parent(&parent)
        .map_err(|e| e.to_string())?
        .on_navigation(|url| {
            url.scheme() == "https"
                && matches!(
                    url.host_str(),
                    Some("steamdb.info" | "steamcommunity.com" | "login.steampowered.com")
                )
        })
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    let mut args = env::args().skip(1);
    if args.next().as_deref() == Some("--install") {
        std::process::exit(download::worker(&args.next().unwrap_or_default()));
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            snapshot,
            favorites::check_isle_server,
            favorites::add_steam_favorite,
            server_config::local_query_port,
            spiro_presets,
            use_preset_manifests,
            select_version,
            add_version,
            save_settings,
            save_arguments,
            download_build,
            launch,
            open_history,
            open_installation,
            server_config::read_server_config,
            server_config::save_server_config
        ])
        .run(tauri::generate_context!())
        .expect("could not start The Isle Version Manager");
}
