//! Appearance file I/O belongs to the terminal session, never to lookup workers.
use crate::{Appearance, AppearanceOverrides};
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) struct ConfigStore {
    path: Option<PathBuf>,
    saved: Appearance,
    values: Map<String, Value>,
    pending: AppearanceOverrides,
}

impl ConfigStore {
    pub(crate) fn load(
        path: Option<PathBuf>,
        overrides: AppearanceOverrides,
    ) -> Result<(Self, Appearance)> {
        let values = match path.as_deref() {
            Some(path) => match fs::read(path) {
                Ok(bytes) => {
                    serde_json::from_slice::<Map<String, Value>>(&bytes).with_context(|| {
                        format!("Invalid appearance configuration at {}", path.display())
                    })?
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Map::new(),
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("Cannot read appearance configuration at {}", path.display())
                    });
                }
            },
            None => Map::new(),
        };
        let saved: Appearance = serde_json::from_value(Value::Object(values.clone()))
            .with_context(|| {
                format!(
                    "Invalid appearance configuration at {}",
                    path.as_deref()
                        .unwrap_or(Path::new("config.json"))
                        .display()
                )
            })?;
        Ok((
            Self {
                path,
                saved,
                values,
                pending: AppearanceOverrides::default(),
            },
            overrides.apply(saved),
        ))
    }

    /// Persist only fields the user changed, rather than unrelated CLI overrides.
    /// Mutate in-memory saved preferences only after the atomic write succeeds.
    pub(crate) fn save_change(&mut self, before: Appearance, after: Appearance) -> Result<bool> {
        let Some(path) = &self.path else {
            return Ok(false);
        };
        if before.color_theme != after.color_theme {
            self.pending.color_theme = Some(after.color_theme);
        }
        if before.theme_background != after.theme_background {
            self.pending.theme_background = Some(after.theme_background);
        }
        if before.truecolor != after.truecolor {
            self.pending.truecolor = Some(after.truecolor);
        }
        let saved = self.pending.apply(self.saved);
        let mut values = self.values.clone();
        values.extend(
            serde_json::to_value(saved)?
                .as_object()
                .context("Appearance must be an object")?
                .clone(),
        );
        let mut bytes = serde_json::to_vec_pretty(&values)?;
        bytes.push(b'\n');
        atomic_write(path, &bytes).with_context(|| {
            format!("Cannot save appearance configuration at {}", path.display())
        })?;
        self.saved = saved;
        self.values = values;
        self.pending = AppearanceOverrides::default();
        Ok(true)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .context("Configuration path must name a file")?
        .to_string_lossy();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..32 {
        let temporary = parent.join(format!(
            ".{name}.{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&temporary) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    bail!("Cannot create a temporary configuration file")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThemePreset;

    #[test]
    fn first_run_does_not_write_and_changes_restore_on_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/config.json");
        let (mut store, initial) =
            ConfigStore::load(Some(path.clone()), AppearanceOverrides::default()).unwrap();
        assert_eq!(initial, Appearance::default());
        assert!(!path.exists());
        let changed = Appearance {
            color_theme: ThemePreset::Orange,
            ..initial
        };
        assert!(store.save_change(initial, changed).unwrap());
        assert_eq!(
            ConfigStore::load(Some(path.clone()), AppearanceOverrides::default())
                .unwrap()
                .1,
            changed
        );
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn cli_overrides_are_not_saved_by_unrelated_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"color_theme":"whiteout","extra":"keep me"}"#).unwrap();
        let (mut store, initial) = ConfigStore::load(
            Some(path.clone()),
            AppearanceOverrides {
                color_theme: Some(ThemePreset::Orange),
                truecolor: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(initial.color_theme, ThemePreset::Orange);
        assert!(!initial.truecolor);
        let changed = Appearance {
            theme_background: false,
            ..initial
        };
        store.save_change(initial, changed).unwrap();
        let values: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(values["extra"], "keep me");
        let (_, restored) = ConfigStore::load(Some(path), AppearanceOverrides::default()).unwrap();
        assert_eq!(restored.color_theme, ThemePreset::Whiteout);
        assert!(restored.truecolor);
        assert!(!restored.theme_background);
    }

    #[test]
    fn invalid_files_are_reported_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        for content in [
            "not json",
            "[]",
            r#"{"color_theme":"missing"}"#,
            r#"{"truecolor":"yes"}"#,
        ] {
            fs::write(&path, content).unwrap();
            let error = ConfigStore::load(Some(path.clone()), AppearanceOverrides::default())
                .err()
                .unwrap();
            assert!(error.to_string().contains(path.to_str().unwrap()));
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
        }
    }

    #[test]
    fn failed_saves_keep_saved_preferences_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let (mut store, initial) =
            ConfigStore::load(Some(path.clone()), AppearanceOverrides::default()).unwrap();
        fs::create_dir(&path).unwrap();
        let changed = Appearance {
            color_theme: ThemePreset::Orange,
            ..initial
        };
        assert!(store.save_change(initial, changed).is_err());
        assert_eq!(store.saved, initial);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        fs::remove_dir(&path).unwrap();
        let next = Appearance {
            theme_background: false,
            ..changed
        };
        assert!(store.save_change(changed, next).unwrap());
        assert_eq!(store.saved, next);
    }

    #[test]
    fn absent_config_path_is_session_only() {
        let (mut store, initial) = ConfigStore::load(None, AppearanceOverrides::default()).unwrap();
        assert!(
            !store
                .save_change(
                    initial,
                    Appearance {
                        theme_background: false,
                        ..initial
                    }
                )
                .unwrap()
        );
    }

    #[test]
    fn reading_preferences_save_retry_and_preserve_cli_overrides() {
        use crate::{ReadingLayout, ReadingPreferences};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"color_theme":"whiteout","other":123}"#).unwrap();
        let (mut store, appearance) = ConfigStore::load(
            Some(path.clone()),
            AppearanceOverrides {
                color_theme: Some(ThemePreset::Orange),
                ..Default::default()
            },
        )
        .unwrap();
        let before = store.reading_preferences();
        let after = ReadingPreferences {
            reading_layout: ReadingLayout::Focus,
            expand_examples: true,
            ..before
        };
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(
            store
                .save_preferences(appearance, appearance, before, after)
                .is_err()
        );
        assert_eq!(store.reading_preferences(), before);
        fs::remove_dir(&path).unwrap();
        let next = ReadingPreferences {
            expand_ipa: true,
            ..after
        };
        assert!(
            store
                .save_preferences(appearance, appearance, after, next)
                .unwrap()
        );
        let (restored, palette) =
            ConfigStore::load(Some(path.clone()), AppearanceOverrides::default()).unwrap();
        assert_eq!(restored.reading_preferences(), next);
        assert_eq!(palette.color_theme, ThemePreset::Whiteout);
        let values: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(values["other"], 123);
        assert!(values.get("input").is_none());
        assert!(values.get("history").is_none());
    }
}
