use crate::library::{manifest, spiro_catalog, Component, Result};
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Serialize, Deserialize)]
pub struct Download {
    pub directory: PathBuf,
    pub component: Component,
    pub branch: String,
    pub manifest: String,
    pub tool: PathBuf,
    pub username: String,
}

pub fn resolve_downloader(setting: &str) -> Result<PathBuf> {
    let tool = if setting.is_empty() {
        let bundled = env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("tools")));
        bundled.into_iter().chain(env::split_paths(&env::var_os("PATH").unwrap_or_default()))
            .filter(|p| p.is_absolute())
            .map(|p| p.join("DepotDownloader.exe"))
            .find(|p| p.is_file())
            .ok_or("Install DepotDownloader, then restart the launcher, or set its executable path in Settings.")?
    } else {
        PathBuf::from(setting)
    };
    if !tool.is_absolute()
        || !tool.is_file()
        || !tool
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("DepotDownloader.exe"))
    {
        return Err("Choose the full path to DepotDownloader.exe in Settings.".into());
    }
    Ok(tool)
}

impl Download {
    fn command(&self) -> Command {
        let (app, depot) = self.component.depot();
        let mut cmd = Command::new(&self.tool);
        cmd.args([
            "-app",
            app,
            "-depot",
            depot,
            "-manifest",
            &self.manifest,
            "-branch",
            &self.branch,
            "-validate",
            "-dir",
        ])
        .arg(&self.directory);
        if !self.username.is_empty() {
            cmd.args(["-username", &self.username]);
        } else if self.component == Component::Game
            || spiro_catalog().iter().any(|p| {
                p.version.branch == "evrima" && p.version.server.as_deref() == Some(&self.manifest)
            })
        {
            cmd.arg("-qr");
        }
        cmd
    }

    // The worker owns the lock and receipt so closing the launcher does not interrupt a transfer.
    fn install(&self) -> Result<()> {
        manifest(&self.manifest)?;
        fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join(".download-lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|e| format!("Cannot start a second download to this folder: {e}"))?;
        let receipt = self.directory.join(".installed");
        let executable = self.directory.join(self.component.executable());
        if receipt.is_file() && executable.is_file() {
            return Ok(());
        }
        if receipt.exists() {
            fs::remove_file(&receipt).map_err(|e| e.to_string())?;
        }

        let status = self
            .command()
            .stdin(Stdio::from(
                File::open("CONIN$").map_err(|e| e.to_string())?,
            ))
            .stdout(console_output()?)
            .stderr(console_output()?)
            .status()
            .map_err(|e| format!("Cannot start DepotDownloader: {e}"))?;
        if !status.success() {
            return Err(format!("DepotDownloader ended with {status}."));
        }
        if !executable.is_file() {
            return Err(format!(
                "This manifest did not contain {}.",
                self.component.executable()
            ));
        }
        File::create(receipt).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn console_output() -> Result<Stdio> {
    File::options()
        .write(true)
        .open(Path::new("CONOUT$"))
        .map(Stdio::from)
        .map_err(|e| e.to_string())
}

pub fn worker(request: &str) -> i32 {
    // A GUI-subsystem process needs its own console for Steam Guard, QR codes and transfer progress.
    unsafe {
        windows_sys::Win32::System::Console::AllocConsole();
    }
    let result = serde_json::from_str::<Download>(request)
        .map_err(|e| e.to_string())
        .and_then(|job| job.install());
    match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("\n{message}\nPress Enter to close this download window.");
            let _ = io::stdin().read_line(&mut String::new());
            1
        }
    }
}
