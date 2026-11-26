use crate::{home, installed_directory, Component, Library, Result};
use serde::Serialize;
use std::{
    env, fs,
    io::{self, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::AppHandle;

#[derive(Serialize)]
pub struct ServerConfig {
    path: PathBuf,
    text: String,
    exists: bool,
    steam_id: Option<String>,
}

#[derive(Clone, Copy)]
enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

fn read(path: &Path) -> Result<(String, Encoding, bool)> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok((String::new(), Encoding::Utf8, false))
        }
        Err(e) => return Err(format!("Cannot read Game.ini: {e}")),
    };
    let (text, encoding) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let (words, remainder) = bytes[2..].as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("Game.ini has an incomplete UTF-16 character.".into());
        }
        let little = bytes[0] == 0xff;
        let decode = if little {
            u16::from_le_bytes
        } else {
            u16::from_be_bytes
        };
        let words: Vec<u16> = words.iter().copied().map(decode).collect();
        (
            String::from_utf16(&words).map_err(|e| e.to_string())?,
            if little {
                Encoding::Utf16Le
            } else {
                Encoding::Utf16Be
            },
        )
    } else {
        let content = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]);
        (
            String::from_utf8(content.unwrap_or(&bytes).to_vec())
                .map_err(|_| "Game.ini needs UTF-8 or UTF-16 encoding to edit here.")?,
            if content.is_some() {
                Encoding::Utf8Bom
            } else {
                Encoding::Utf8
            },
        )
    };
    Ok((text, encoding, true))
}

fn write_config(path: &Path, original: &str, existed: bool, edited: &str) -> Result<()> {
    let parent = path.parent().ok_or("Missing config directory.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let lock = fs::File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(".launcher-config-lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let (current, encoding, exists) = read(path)?;
    if current != original || exists != existed {
        return Err(
            "Game.ini changed after you opened it. Reopen the editor to load the current settings."
                .into(),
        );
    }
    let normalized = edited.replace("\r\n", "\n");
    if normalized == current.replace("\r\n", "\n") {
        return Ok(());
    }
    let text = if !exists || current.contains("\r\n") {
        normalized.replace('\n', "\r\n")
    } else {
        normalized
    };
    let bytes = match encoding {
        Encoding::Utf8 => text.into_bytes(),
        Encoding::Utf8Bom => [b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let mut out = Vec::new();
            for word in std::iter::once(0xfeff).chain(text.encode_utf16()) {
                out.extend(if matches!(encoding, Encoding::Utf16Le) {
                    word.to_le_bytes()
                } else {
                    word.to_be_bytes()
                });
            }
            out
        }
    };
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(&bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    temp.persist(path)
        .map_err(|e| format!("Cannot save Game.ini: {e}"))?;
    Ok(())
}

fn config_path(app: &AppHandle, manifest: &str) -> Result<PathBuf> {
    let lib = Library::load(&home(app)?)?;
    Ok(installed_directory(&lib, Component::Server, manifest)?
        .join("TheIsle/Saved/Config/WindowsServer/Game.ini"))
}

#[tauri::command]
pub fn local_query_port(app: AppHandle, manifest: String, index: Option<usize>) -> Result<u16> {
    let lib = Library::load(&home(&app)?)?;
    let dir = installed_directory(&lib, Component::Server, &manifest)?
        .join("TheIsle/Saved/Config/WindowsServer");
    let port = |s: &str| s.trim().parse::<u16>().ok().filter(|p| *p != 0);
    if let Some(profile) = index
        .and_then(|i| lib.versions.get(i))
        .filter(|v| v.server.as_ref() == Some(&manifest))
    {
        for token in profile.arguments.lines().flat_map(|l| l.split('?')).rev() {
            if let Some((key, value)) = token.trim().trim_start_matches('-').split_once('=') {
                if key.eq_ignore_ascii_case("QueryPort")
                    || key.eq_ignore_ascii_case("GameServerQueryPort")
                {
                    if let Some(p) = port(value) {
                        return Ok(p);
                    }
                }
            }
        }
    }
    for (file, section, key) in [
        ("Game.ini", "/Script/TheIsle.TIGameSession", "QueryPort"),
        ("Engine.ini", "OnlineSubsystemSteam", "GameServerQueryPort"),
    ] {
        let (text, _, _) = read(&dir.join(file))?;
        let mut active = false;
        let mut configured = None;
        for line in text.lines().map(str::trim) {
            if let Some(header) = line.strip_prefix('[').and_then(|s| s.split_once(']')) {
                active = header.0.eq_ignore_ascii_case(section);
            } else if let Some((k, value)) = line.split_once('=') {
                if active && k.trim().eq_ignore_ascii_case(key) {
                    configured = port(value.split([';', '/']).next().unwrap_or(""));
                }
            }
        }
        if let Some(p) = configured {
            return Ok(p);
        }
    }
    Ok(27015)
}

fn active_steam_id() -> Option<String> {
    let reg = PathBuf::from(env::var_os("SystemRoot")?).join("System32/reg.exe");
    let result = Command::new(reg)
        .args([
            "query",
            r"HKCU\Software\Valve\Steam\ActiveProcess",
            "/v",
            "ActiveUser",
        ])
        .creation_flags(0x08000000)
        .output()
        .ok()?;
    if !result.status.success() {
        return None;
    }
    let output = String::from_utf8_lossy(&result.stdout);
    let value = output.split_whitespace().last()?.strip_prefix("0x")?;
    let account = u32::from_str_radix(value, 16).ok()?;
    (account != 0).then(|| (76561197960265728u64 + u64::from(account)).to_string())
}

#[tauri::command]
pub fn read_server_config(app: AppHandle, manifest: String) -> Result<ServerConfig> {
    let path = config_path(&app, &manifest)?;
    let (text, _, exists) = read(&path)?;
    Ok(ServerConfig {
        path,
        text,
        exists,
        steam_id: active_steam_id(),
    })
}

#[tauri::command]
pub fn save_server_config(
    app: AppHandle,
    manifest: String,
    path: PathBuf,
    original: String,
    existed: bool,
    edited: String,
) -> Result<()> {
    if path != config_path(&app, &manifest)? {
        return Err("The builds folder changed. Reopen the editor.".into());
    }
    write_config(&path, &original, existed, &edited)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_save_preserves_encoding_and_rejects_stale_edits() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("Game.ini");
        let original = "; café\r\n[Other]\r\nValue=42\r\n";
        let bytes: Vec<_> = std::iter::once(0xfeff)
            .chain(original.encode_utf16())
            .flat_map(u16::to_le_bytes)
            .collect();
        fs::write(&path, &bytes).unwrap();
        write_config(&path, original, true, &original.replace("\r\n", "\n")).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        write_config(&path, original, true, &format!("{original}Name=Isle\n")).unwrap();
        let (new, encoding, _) = read(&path).unwrap();
        assert!(matches!(encoding, Encoding::Utf16Le));
        assert_eq!(new, format!("{original}Name=Isle\r\n"));
        assert!(write_config(&path, original, true, "overwrite").is_err());
        assert_eq!(read(&path).unwrap().0, new);
    }
}
