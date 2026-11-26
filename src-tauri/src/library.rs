use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Component {
    Game,
    Server,
}

impl Component {
    pub fn name(self) -> &'static str {
        match self {
            Self::Game => "game",
            Self::Server => "server",
        }
    }
    pub fn depot(self) -> (&'static str, &'static str) {
        match self {
            Self::Game => ("376210", "376212"),
            Self::Server => ("412680", "412681"),
        }
    }
    pub fn executable(self) -> &'static str {
        match self {
            Self::Game => "TheIsle.exe",
            Self::Server => "TheIsleServer.exe",
        }
    }
    pub fn history(self) -> &'static str {
        match self {
            Self::Game => "https://steamdb.info/depot/376212/manifests/",
            Self::Server => "https://steamdb.info/depot/412681/manifests/",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Version {
    pub name: String,
    pub branch: String,
    pub game: Option<String>,
    pub server: Option<String>,
    #[serde(default)]
    pub arguments: String,
}

#[derive(Deserialize, Serialize)]
pub struct SpiroPreset {
    pub date: String,
    #[serde(flatten)]
    pub version: Version,
}

pub fn spiro_catalog() -> &'static [SpiroPreset] {
    static CATALOG: OnceLock<Vec<SpiroPreset>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../spiro.json")).expect("invalid bundled Spiro catalog")
    })
}

impl Version {
    pub fn spiro_preset(&self) -> Option<&'static Version> {
        spiro_catalog()
            .iter()
            .map(|p| &p.version)
            .find(|p| p.branch == self.branch && p.name == self.name)
    }

    fn complete_spiro_pair(&mut self) {
        if let Some(preset) = self.spiro_preset() {
            if self.game == preset.game && self.server.is_none() {
                self.server.clone_from(&preset.server);
            } else if self.server == preset.server && self.game.is_none() {
                self.game.clone_from(&preset.game);
            }
        }
    }
}

pub fn manifest(value: &str) -> Result<&str> {
    if !value.starts_with('0')
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<u64>().is_ok()
    {
        Ok(value)
    } else {
        Err("Enter a SteamDB manifest ID as a positive decimal number.".into())
    }
}

impl Version {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err("Give this version a name of 1 to 80 characters.".into());
        }
        if self.branch.is_empty()
            || self.branch.len() > 80
            || !self
                .branch
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || self.branch.starts_with('-')
        {
            return Err("Enter a Steam branch such as public or evrima.".into());
        }
        if self.game.is_none() && self.server.is_none() {
            return Err("Enter a game or server manifest ID.".into());
        }
        if let Some(id) = &self.game {
            manifest(id)?;
        }
        if let Some(id) = &self.server {
            manifest(id)?;
        }
        Ok(())
    }
    pub fn id(&self, component: Component) -> Result<&str> {
        match component {
            Component::Game => manifest(
                self.game
                    .as_deref()
                    .ok_or("This release has no game manifest.")?,
            ),
            Component::Server => manifest(
                self.server
                    .as_deref()
                    .ok_or("This version has no server manifest.")?,
            ),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Library {
    pub versions: Vec<Version>,
    pub selected: usize,
    pub folder: PathBuf,
    pub downloader: String,
    pub username: String,
}

#[derive(Serialize)]
pub struct Installation {
    pub manifest: String,
    pub name: String,
    pub branch: Option<String>,
    pub profile: Option<usize>,
}

impl Library {
    pub fn load(home: &Path) -> Result<Self> {
        match fs::read(home.join("library.json")) {
            Ok(data) => {
                let mut library: Self = serde_json::from_slice(&data)
                    .map_err(|e| format!("Cannot read library.json: {e}"))?;
                library.current()?.validate()?;
                for version in &mut library.versions {
                    version.validate()?;
                    version.complete_spiro_pair();
                }
                Ok(library)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self {
                versions: vec![
                    Version {
                        name: "Evrima · November 2026".into(),
                        branch: "evrima".into(),
                        game: Some("7997973843710229014".into()),
                        server: Some("8463360902485159842".into()),
                        arguments: "-log".into(),
                    },
                    Version {
                        name: "Legacy · February 2022".into(),
                        branch: "public".into(),
                        game: Some("923669276770277096".into()),
                        server: Some("2602016430604302144".into()),
                        arguments: "-log".into(),
                    },
                ],
                selected: 0,
                folder: home.join("builds"),
                downloader: String::new(),
                username: String::new(),
            }),
            Err(e) => Err(format!("Cannot open library.json: {e}")),
        }
    }
    pub fn current(&self) -> Result<&Version> {
        self.versions
            .get(self.selected)
            .ok_or_else(|| "The selected version is missing from the library.".into())
    }
    pub fn save(&self, home: &Path) -> Result<()> {
        let temp = tempfile::NamedTempFile::new_in(home).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(temp.as_file(), self).map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(home.join("library.json"))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn directory(&self, version: &Version, component: Component) -> Result<PathBuf> {
        Ok(self
            .folder
            .join(component.name())
            .join(version.id(component)?))
    }

    pub fn installations(&self, component: Component) -> Result<Vec<Installation>> {
        let root = self.folder.join(component.name());
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("Cannot list {}: {e}", root.display())),
        };
        let mut installed = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if manifest(&id).is_err()
                || !entry.path().is_dir()
                || status(&entry.path(), component)? != Status::Installed
            {
                continue;
            }
            let profiles: Vec<_> = self
                .versions
                .iter()
                .enumerate()
                .filter(|(_, v)| v.id(component) == Ok(id.as_str()))
                .collect();
            let instance = |version: Option<&Version>, profile| Installation {
                manifest: id.clone(),
                profile,
                name: version
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| format!("Manifest {id}")),
                branch: version.map(|v| v.branch.clone()),
            };
            if profiles.is_empty() {
                let known = spiro_catalog()
                    .iter()
                    .map(|p| &p.version)
                    .find(|v| v.id(component) == Ok(id.as_str()));
                installed.push(instance(known, None));
            } else {
                for (index, version) in profiles {
                    installed.push(instance(Some(version), Some(index)));
                    if component == Component::Game {
                        break;
                    }
                }
            }
        }
        installed.sort_by(|a, b| a.name.cmp(&b.name).then(a.manifest.cmp(&b.manifest)));
        Ok(installed)
    }
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Unavailable,
    Available,
    Downloading,
    Incomplete,
    Installed,
}

pub fn status(dir: &Path, component: Component) -> Result<Status> {
    if !dir.exists() {
        return Ok(Status::Available);
    }
    match fs::File::options()
        .read(true)
        .write(true)
        .open(dir.join(".download-lock"))
    {
        Ok(lock) => match lock.try_lock() {
            Ok(()) => (),
            Err(std::fs::TryLockError::WouldBlock) => return Ok(Status::Downloading),
            Err(e) => return Err(format!("Cannot inspect download: {e}")),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.to_string()),
    }
    if dir.join(".installed").is_file() && dir.join(component.executable()).is_file() {
        Ok(Status::Installed)
    } else {
        Ok(Status::Incomplete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiro_catalog_has_unique_names_and_pinned_windows_manifests() {
        let mut names = std::collections::HashSet::new();
        let mut pairs = std::collections::HashSet::new();
        for preset in spiro_catalog() {
            let version = &preset.version;
            version.validate().unwrap();
            assert!(names.insert(&version.name));
            assert!(pairs.insert((&version.game, &version.server)));
            assert!(version.server.is_some());
            assert_eq!(
                version.branch,
                if version.name == "Isla Spiro · 0.11.59.04" {
                    "spiro0.11.59.04"
                } else {
                    "evrima"
                }
            );
        }
        assert_eq!(
            spiro_catalog()
                .iter()
                .filter(|p| p.version.game.is_some())
                .count(),
            66
        );
    }

    #[test]
    fn saved_spiro_versions_fill_missing_ids_and_preserve_conflicts() {
        let home = tempfile::tempdir().unwrap();
        let mut library = Library::load(home.path()).unwrap();
        let mut entry = spiro_catalog()
            .iter()
            .find(|p| p.version.name.contains("Update 1 ·"))
            .unwrap()
            .version
            .clone();
        assert_eq!(entry.game.as_deref(), Some("4421421704277639686"));
        assert_eq!(entry.server.as_deref(), Some("5067025485064045609"));
        entry.arguments = "-log -custom".into();
        entry.server = None;
        library.versions.push(entry.clone());
        entry.game = Some("3358203269288704404".into());
        library.versions.push(entry);
        library.selected = 3;
        library.save(home.path()).unwrap();
        let restored = Library::load(home.path()).unwrap();
        assert_eq!(
            restored.versions[2].server.as_deref(),
            Some("5067025485064045609")
        );
        let custom = restored.current().unwrap();
        assert!(custom.server.is_none());
        assert_eq!(custom.game.as_deref(), Some("3358203269288704404"));
        assert_eq!(custom.arguments, "-log -custom");
    }

    #[test]
    fn version_switch_preserves_distinct_paths_and_exact_ids() {
        let home = tempfile::tempdir().unwrap();
        let mut library = Library::load(home.path()).unwrap();
        let first = library
            .directory(library.current().unwrap(), Component::Game)
            .unwrap();
        library.selected = 1;
        library.save(home.path()).unwrap();
        let restored = Library::load(home.path()).unwrap();
        let version = restored.current().unwrap();
        assert_eq!(version.game.as_deref(), Some("923669276770277096"));
        assert_ne!(first, restored.directory(version, Component::Game).unwrap());
        assert_ne!(
            restored.directory(version, Component::Game).unwrap(),
            restored.directory(version, Component::Server).unwrap()
        );
        for invalid in ["", "../23", "1e8", "18446744073709551616", "+99", "0"] {
            assert!(manifest(invalid).is_err());
        }
    }

    #[test]
    fn finds_installed_games_after_a_profile_changes_without_modifying_files() {
        let home = tempfile::tempdir().unwrap();
        let mut library = Library::load(home.path()).unwrap();
        let game = Component::Game;
        let first = library.directory(library.current().unwrap(), game).unwrap();
        let orphan = library.folder.join("game/3358203269288704404");
        for dir in [&first, &orphan] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join(".installed"), []).unwrap();
            fs::write(dir.join(game.executable()), b"existing files").unwrap();
        }
        let mut alias = library.current().unwrap().clone();
        alias.name = "Second profile".into();
        library.versions.push(alias);
        let partial = library.folder.join("game/1234");
        fs::create_dir_all(&partial).unwrap();
        fs::write(partial.join(game.executable()), []).unwrap();
        let instances = library.installations(game).unwrap();
        assert_eq!(instances.len(), 2);
        assert!(instances.iter().any(|i| i.profile == Some(0)));
        assert!(instances
            .iter()
            .any(|i| i.profile.is_none() && i.name.contains("Isla Spiro")));
        assert_eq!(
            fs::read(orphan.join(game.executable())).unwrap(),
            b"existing files"
        );
    }

    #[test]
    fn partial_and_locked_installs_cannot_launch() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path();
        let component = Component::Game;
        fs::write(dir.join(component.executable()), []).unwrap();
        assert_eq!(status(dir, component).unwrap(), Status::Incomplete);
        let lock = fs::File::create(dir.join(".download-lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(status(dir, component).unwrap(), Status::Downloading);
        drop(lock);
        fs::write(dir.join(".installed"), []).unwrap();
        assert_eq!(status(dir, component).unwrap(), Status::Installed);
        fs::remove_file(dir.join(component.executable())).unwrap();
        assert_eq!(status(dir, component).unwrap(), Status::Incomplete);
    }
}
