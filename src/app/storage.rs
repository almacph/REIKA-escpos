use super::config::AppConfig;
use super::print_log::PrintLog;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub migrated: Vec<String>,
    pub conflicts: Vec<String>,
    pub invalid: Vec<String>,
}

impl MigrationReport {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.invalid.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    Portable,
    Installed,
}

#[derive(Debug, Clone)]
pub struct StateDirectory {
    root: PathBuf,
    mode: StorageMode,
    available: bool,
    error: Option<String>,
}

impl StateDirectory {
    pub fn resolve() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let installed =
            std::env::var_os("REIKA_INSTALL_MODE").is_some_and(|value| value == "installed");
        if !installed {
            return Self {
                root: exe_dir,
                mode: StorageMode::Portable,
                available: true,
                error: None,
            };
        }
        let Some(base) = std::env::var_os("REIKA_ORIGINAL_USER_LOCALAPPDATA").map(PathBuf::from)
        else {
            return Self {
                root: exe_dir,
                mode: StorageMode::Installed,
                available: false,
                error: Some("Installed mode requires REIKA_ORIGINAL_USER_LOCALAPPDATA from the original interactive user".into()),
            };
        };
        Self {
            root: base.join("REIKA-escpos"),
            mode: StorageMode::Installed,
            available: true,
            error: None,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn mode(&self) -> StorageMode {
        self.mode
    }
    pub fn is_available(&self) -> bool {
        self.available
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    #[cfg(test)]
    pub(crate) fn unavailable_for_test(root: PathBuf) -> Self {
        Self {
            root,
            mode: StorageMode::Installed,
            available: false,
            error: Some("test state handoff unavailable".to_string()),
        }
    }

    pub fn migrate_portable_files(
        &self,
        portable_root: &Path,
        names: &[&str],
    ) -> io::Result<MigrationReport> {
        if self.mode != StorageMode::Installed {
            return Ok(MigrationReport::default());
        }
        if !self.available {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                self.error
                    .clone()
                    .unwrap_or_else(|| "state directory unavailable".into()),
            ));
        }
        fs::create_dir_all(&self.root)?;
        let mut report = MigrationReport::default();
        for name in names {
            let source = portable_root.join(name);
            let destination = self.path(name);
            if !source.exists() {
                continue;
            }
            if destination.exists() {
                report.conflicts.push((*name).to_string());
                continue;
            }
            if !valid_state_file(name, &source) {
                report.invalid.push((*name).to_string());
                continue;
            }
            fs::copy(source, destination)?;
            report.migrated.push((*name).to_string());
        }
        Ok(report)
    }
}

fn valid_state_file(name: &str, path: &Path) -> bool {
    match name {
        "config.toml" => fs::read_to_string(path)
            .ok()
            .is_some_and(|contents| toml::from_str::<AppConfig>(&contents).is_ok()),
        "print_log.json" => fs::read_to_string(path)
            .ok()
            .is_some_and(|contents| serde_json::from_str::<PrintLog>(&contents).is_ok()),
        _ => true,
    }
}

pub fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("state"),
        stamp
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    if path.exists() {
        let _ = fs::copy(path, path.with_extension("last-good"));
        replace_existing(&temp, path)?;
    } else {
        fs::rename(&temp, path)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_existing(temp: &Path, path: &Path) -> io::Result<()> {
    // rename replaces an existing file atomically on Unix-like platforms.
    fs::rename(temp, path)
}

#[cfg(windows)]
fn replace_existing(temp: &Path, path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACE_FILE_FLAGS};

    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let temp_wide: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    // ReplaceFileW keeps the destination continuously present and removes the
    // replacement file only after the atomic replacement succeeds.
    unsafe {
        ReplaceFileW(
            PCWSTR(path_wide.as_ptr()),
            PCWSTR(temp_wide.as_ptr()),
            PCWSTR::null(),
            REPLACE_FILE_FLAGS(0),
            None,
            None,
        )
    }
    .map_err(|error| io::Error::from_raw_os_error((error.code().0 as u32 & 0xffff) as i32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn atomic_write_keeps_last_good_and_migration_does_not_overwrite() {
        let root = std::env::temp_dir().join(format!("reika-storage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let file = root.join("config.toml");
        atomic_write(&file, b"old").unwrap();
        atomic_write(&file, b"new").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(fs::read(file.with_extension("last-good")).unwrap(), b"old");
        let source = root.join("portable");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("config.toml"),
            toml::to_string(&AppConfig::default()).unwrap(),
        )
        .unwrap();
        let destination = root.join("installed");
        let state = StateDirectory {
            root: destination.clone(),
            mode: StorageMode::Installed,
            available: true,
            error: None,
        };
        assert!(state
            .migrate_portable_files(&source, &["config.toml"])
            .unwrap()
            .migrated
            .contains(&"config.toml".to_string()));
        let expected = toml::to_string(&AppConfig::default()).unwrap();
        assert_eq!(
            fs::read(destination.join("config.toml")).unwrap(),
            expected.as_bytes()
        );
        fs::write(destination.join("config.toml"), b"existing").unwrap();
        assert!(state
            .migrate_portable_files(&source, &["config.toml"])
            .unwrap()
            .conflicts
            .contains(&"config.toml".to_string()));
        assert_eq!(
            fs::read(destination.join("config.toml")).unwrap(),
            b"existing"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn migration_rejects_invalid_toml_and_reports_conflict() {
        let root = std::env::temp_dir().join(format!("reika-migration-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let source = root.join("portable");
        let destination = root.join("installed");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("config.toml"), "not = [valid").unwrap();
        fs::write(source.join("print_log.json"), "[]").unwrap();
        fs::write(destination.join("print_log.json"), "[]").unwrap();
        let state = StateDirectory {
            root: destination,
            mode: StorageMode::Installed,
            available: true,
            error: None,
        };
        let report = state
            .migrate_portable_files(&source, &["config.toml", "print_log.json"])
            .unwrap();
        assert_eq!(report.invalid, vec!["config.toml"]);
        assert_eq!(report.conflicts, vec!["print_log.json"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn migration_rejects_json_that_is_syntactically_valid_but_not_print_log() {
        let root =
            std::env::temp_dir().join(format!("reika-typed-migration-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let source = root.join("portable");
        let destination = root.join("installed");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("print_log.json"), "{}").unwrap();
        let state = StateDirectory {
            root: destination,
            mode: StorageMode::Installed,
            available: true,
            error: None,
        };
        let report = state
            .migrate_portable_files(&source, &["print_log.json"])
            .unwrap();
        assert_eq!(report.invalid, vec!["print_log.json"]);
        assert!(!state.path("print_log.json").exists());
        let _ = fs::remove_dir_all(root);
    }
}
