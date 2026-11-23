//! Imported audio is validated as a batch and stays reversible until its
//! profile has been saved. Backup names never become playable sound files.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use base64::Engine as _;

pub const MAX_SOUND_BYTES: usize = 10 << 20;
pub static WRITER: Mutex<()> = Mutex::new(());
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct SoundFile {
    key: String,
    ext: String,
    bytes: Vec<u8>,
}

impl SoundFile {
    pub fn from_bytes(key: &str, ext: &str, bytes: Vec<u8>) -> Result<Self, String> {
        if !crate::sound_key(key) || !crate::SOUND_EXTS.iter().any(|(e, _)| *e == ext) {
            return Err(format!("Invalid sound: {key}.{ext}"));
        }
        if bytes.len() > MAX_SOUND_BYTES { return Err(format!("Sound {key} is larger than 10 MiB")); }
        Ok(Self { key: key.into(), ext: ext.into(), bytes })
    }

    pub fn decode(key: &str, ext: &str, encoded: &str) -> Result<Self, String> {
        if !crate::sound_key(key) || !crate::SOUND_EXTS.iter().any(|(e, _)| *e == ext) {
            return Err(format!("Invalid imported sound: {key}.{ext}"));
        }
        // Refuse oversized input before allocating its decoded representation.
        if encoded.len() > MAX_SOUND_BYTES.div_ceil(3) * 4 {
            return Err(format!("Imported sound {key} is larger than 10 MiB"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| format!("Invalid audio data for {key}: {e}"))?;
        if bytes.len() > MAX_SOUND_BYTES {
            return Err(format!("Imported sound {key} is larger than 10 MiB"));
        }
        Self::from_bytes(key, ext, bytes)
    }
}

pub struct SoundTransaction {
    _writer: MutexGuard<'static, ()>,
    directory: PathBuf,
    stage: Option<PathBuf>,
    replacements: Vec<(PathBuf, PathBuf)>,
    backups: Vec<(PathBuf, PathBuf)>,
    installed: Vec<PathBuf>,
    finished: bool,
}

impl SoundTransaction {
    pub fn begin(dir: &Path, sounds: &[SoundFile]) -> Result<Self, String> {
        let mut transaction = Self::stage(dir, sounds)?;
        if let Err(error) = transaction.replace(dir, sounds) {
            return Err(transaction.abort(error));
        }
        Ok(transaction)
    }

    fn stage(dir: &Path, sounds: &[SoundFile]) -> Result<Self, String> {
        let writer = WRITER.lock().unwrap_or_else(|e| e.into_inner());
        let mut transaction = Self {
            _writer: writer,
            directory: dir.into(),
            stage: None,
            replacements: Vec::new(),
            backups: Vec::new(),
            installed: Vec::new(),
            finished: false,
        };
        if sounds.is_empty() {
            return Ok(transaction);
        }
        let mut keys = std::collections::HashSet::new();
        for sound in sounds {
            if !keys.insert(sound.key.as_str()) {
                return Err(format!("Duplicate imported sound: {}", sound.key));
            }
            for (ext, _) in crate::SOUND_EXTS {
                let path = dir.join(format!("{}.{ext}", sound.key));
                match std::fs::symlink_metadata(&path) {
                    Ok(meta) if !meta.is_file() => {
                        return Err(format!("Not a sound file: {}", path.display()))
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("Cannot inspect {}: {e}", path.display())),
                }
            }
        }
        let stage = transaction.ensure_stage()?;
        for (index, sound) in sounds.iter().enumerate() {
            let staged = stage.join(format!("new-{index}"));
            Self::write_staged(&staged, &sound.bytes)?;
            transaction
                .replacements
                .push((staged, dir.join(format!("{}.{}", sound.key, sound.ext))));
        }
        Ok(transaction)
    }

    fn ensure_stage(&mut self) -> Result<PathBuf, String> {
        if let Some(stage) = &self.stage {
            return Ok(stage.clone());
        }
        std::fs::create_dir_all(&self.directory)
            .map_err(|e| format!("Cannot create sounds directory: {e}"))?;
        let stage = loop {
            let candidate = self.directory.join(format!(
                ".import-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Cannot stage imported sounds: {e}")),
            }
        };
        self.stage = Some(stage.clone());
        Ok(stage)
    }

    fn write_staged(path: &Path, bytes: &[u8]) -> Result<(), String> {
        let mut file = std::fs::File::create(path)
            .map_err(|e| format!("Cannot stage {}: {e}", path.display()))?;
        std::io::Write::write_all(&mut file, bytes)
            .map_err(|e| format!("Cannot stage {}: {e}", path.display()))?;
        file.sync_data()
            .map_err(|e| format!("Cannot flush {}: {e}", path.display()))
    }

    /// Enlist a profile JSON file in the same rollback as its audio. The
    /// caller holds the settings and collection writer guards until commit.
    pub fn replace_file(&mut self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let existed = match std::fs::symlink_metadata(path) {
            Ok(meta) if !meta.is_file() => {
                return Err(format!("Not a profile file: {}", path.display()))
            }
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(format!("Cannot inspect {}: {e}", path.display())),
        };
        let stage = self.ensure_stage()?;
        let index = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let staged = stage.join(format!("profile-{index}.new"));
        let backup = stage.join(format!("profile-{index}.old"));
        Self::write_staged(&staged, bytes)?;
        if existed {
            // Keep the live JSON name present for readers that need no writer
            // guard. The final replacement is the same atomic rename used by
            // ordinary settings saves.
            std::fs::copy(path, &backup)
                .map_err(|e| format!("Cannot back up {}: {e}", path.display()))?;
        }
        std::fs::rename(&staged, path)
            .map_err(|e| format!("Cannot install {}: {e}", path.display()))?;
        if existed {
            self.backups.push((path.into(), backup));
        }
        self.installed.push(path.into());
        Ok(())
    }

    fn replace(&mut self, dir: &Path, sounds: &[SoundFile]) -> Result<(), String> {
        self.back_up(dir, sounds)?;
        self.install()
    }

    fn back_up(&mut self, dir: &Path, sounds: &[SoundFile]) -> Result<(), String> {
        // Retain every old extension until the complete profile has committed.
        for sound in sounds {
            self.back_up_key(dir, &sound.key)?;
        }
        Ok(())
    }

    fn back_up_key(&mut self, dir: &Path, key: &str) -> Result<(), String> {
        let stage = self.ensure_stage()?;
        for (ext, _) in crate::SOUND_EXTS {
            let original = dir.join(format!("{key}.{ext}"));
            let backup = stage.join(format!(
                "old-{}-{ext}",
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::symlink_metadata(&original) {
                Ok(meta) if !meta.is_file() => {
                    return Err(format!("Not a sound file: {}", original.display()))
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("Cannot inspect {}: {e}", original.display())),
            }
            match std::fs::rename(&original, &backup) {
                Ok(()) => self.backups.push((original, backup)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Cannot back up {}: {e}", original.display())),
            }
        }
        Ok(())
    }

    pub fn remove_sound(&mut self, key: &str) -> Result<(), String> {
        if !crate::sound_key(key) {
            return Err(format!("Invalid imported sound key: {key}"));
        }
        let dir = self.directory.clone();
        self.back_up_key(&dir, key)
    }

    fn install(&mut self) -> Result<(), String> {
        for (staged, destination) in &self.replacements {
            std::fs::rename(staged, destination)
                .map_err(|e| format!("Cannot install {}: {e}", destination.display()))?;
            self.installed.push(destination.clone());
        }
        Ok(())
    }

    pub fn commit(mut self) {
        self.finished = true;
        self.cleanup();
    }

    pub fn abort(&mut self, error: String) -> String {
        match self.rollback() {
            Ok(()) => error,
            Err(rollback) => format!("{error}; {rollback}"),
        }
    }

    fn rollback(&mut self) -> Result<(), String> {
        self.finished = true;
        let mut errors = Vec::new();
        for installed in self.installed.iter().rev() {
            if let Err(e) = std::fs::remove_file(installed) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    errors.push(format!("cannot remove {}: {e}", installed.display()));
                }
            }
        }
        for (original, backup) in self.backups.iter().rev() {
            if let Err(e) = std::fs::rename(backup, original) {
                errors.push(format!("cannot restore {}: {e}", original.display()));
            }
        }
        if errors.is_empty() {
            self.cleanup();
            Ok(())
        } else {
            Err(format!(
                "Sound rollback incomplete ({}); recovery files kept in {}",
                errors.join("; "),
                self.stage
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ))
        }
    }

    fn cleanup(&mut self) {
        let Some(stage) = self.stage.take() else {
            return;
        };
        // Only our flat staging directory is removed; no recursive deletion.
        if let Ok(entries) = std::fs::read_dir(&stage) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        if let Err(e) = std::fs::remove_dir(&stage) {
            crate::log::warn(format!(
                "Cannot clean sound import staging {}: {e}",
                stage.display()
            ));
        }
    }
}

impl Drop for SoundTransaction {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(e) = self.rollback() {
                crate::log::error(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "hs-sound-import-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }

    fn sound(key: &str, ext: &str, bytes: &[u8]) -> SoundFile {
        SoundFile::decode(
            key,
            ext,
            &base64::engine::general_purpose::STANDARD.encode(bytes),
        )
        .unwrap()
    }

    fn remove_directory(path: &Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                std::fs::remove_dir(path).unwrap();
            } else {
                std::fs::remove_file(path).unwrap();
            }
        }
        std::fs::remove_dir(path).unwrap();
    }

    #[test]
    fn validates_audio_before_any_files_change() {
        assert!(SoundFile::decode("../satanic", "wav", "YQ==").is_err());
        assert!(SoundFile::decode("satanic", "exe", "YQ==").is_err());
        assert!(SoundFile::decode("satanic", "wav", "bad base64").is_err());
        let too_large =
            base64::engine::general_purpose::STANDARD.encode(vec![0; MAX_SOUND_BYTES + 1]);
        assert!(SoundFile::decode("satanic", "wav", &too_large).is_err());
        let limit = base64::engine::general_purpose::STANDARD.encode(vec![0; MAX_SOUND_BYTES]);
        assert!(SoundFile::decode("satanic", "wav", &limit).is_ok());
    }

    #[test]
    fn committed_import_replaces_all_old_extensions() {
        let dir = directory();
        std::fs::write(dir.join("satanic.mp3"), b"old mp3").unwrap();
        std::fs::write(dir.join("satanic.flac"), b"old flac").unwrap();
        SoundTransaction::begin(&dir, &[sound("satanic", "wav", b"new")])
            .unwrap()
            .commit();
        assert_eq!(std::fs::read(dir.join("satanic.wav")).unwrap(), b"new");
        assert!(!dir.join("satanic.mp3").exists());
        assert!(!dir.join("satanic.flac").exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        remove_directory(&dir);
    }

    #[test]
    fn a_failed_profile_save_rolls_audio_back_on_drop() {
        let dir = directory();
        std::fs::write(dir.join("satanic.mp3"), b"original").unwrap();
        {
            let _transaction = SoundTransaction::begin(
                &dir,
                &[
                    sound("satanic", "wav", b"replacement"),
                    sound("list-abc", "ogg", b"new list"),
                ],
            )
            .unwrap();
            assert!(dir.join("satanic.wav").exists());
            // A caller returning Err before commit drops this transaction.
        }
        assert_eq!(std::fs::read(dir.join("satanic.mp3")).unwrap(), b"original");
        assert!(!dir.join("satanic.wav").exists());
        assert!(!dir.join("list-abc.ogg").exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        remove_directory(&dir);
    }

    #[test]
    fn failed_final_rename_restores_the_old_sound() {
        let dir = directory();
        std::fs::write(dir.join("satanic.mp3"), b"original").unwrap();
        let sounds = [sound("satanic", "wav", b"replacement")];
        let mut transaction = SoundTransaction::stage(&dir, &sounds).unwrap();
        transaction.back_up(&dir, &sounds).unwrap();
        // A directory at the final destination makes the last rename fail on
        // Windows and Unix, after the old extension has been backed up.
        std::fs::create_dir(dir.join("satanic.wav")).unwrap();
        let failure = transaction.install().unwrap_err();
        assert!(transaction.abort(failure).contains("Cannot install"));
        assert_eq!(std::fs::read(dir.join("satanic.mp3")).unwrap(), b"original");
        drop(transaction);
        remove_directory(&dir);
    }

    #[test]
    fn a_later_failure_restores_already_installed_sounds() {
        let dir = directory();
        std::fs::write(dir.join("satanic.mp3"), b"original").unwrap();
        let sounds = [
            sound("satanic", "wav", b"replacement"),
            sound("list-abc", "ogg", b"other"),
        ];
        let mut transaction = SoundTransaction::stage(&dir, &sounds).unwrap();
        transaction.back_up(&dir, &sounds).unwrap();
        std::fs::create_dir(dir.join("list-abc.ogg")).unwrap();
        let failure = transaction.install().unwrap_err();
        transaction.abort(failure);
        assert_eq!(std::fs::read(dir.join("satanic.mp3")).unwrap(), b"original");
        assert!(!dir.join("satanic.wav").exists());
        drop(transaction);
        remove_directory(&dir);
    }

    #[test]
    fn a_profile_file_failure_restores_settings_and_audio() {
        let dir = directory();
        let settings = dir.join("settings.json");
        let collection = dir.join("collection.json");
        std::fs::write(&settings, b"old settings").unwrap();
        std::fs::write(dir.join("satanic.mp3"), b"old sound").unwrap();
        std::fs::create_dir(&collection).unwrap();
        let mut transaction =
            SoundTransaction::begin(&dir, &[sound("satanic", "wav", b"new sound")]).unwrap();
        transaction
            .replace_file(&settings, b"new settings")
            .unwrap();
        let failure = transaction
            .replace_file(&collection, b"new collection")
            .unwrap_err();
        transaction.abort(failure);
        assert_eq!(std::fs::read(&settings).unwrap(), b"old settings");
        assert_eq!(
            std::fs::read(dir.join("satanic.mp3")).unwrap(),
            b"old sound"
        );
        assert!(!dir.join("satanic.wav").exists());
        drop(transaction);
        remove_directory(&dir);
    }

    #[test]
    fn committed_profile_keeps_settings_collection_and_audio_together() {
        let dir = directory();
        let settings = dir.join("settings.json");
        let collection = dir.join("collection.json");
        std::fs::write(&settings, b"old settings").unwrap();
        std::fs::write(&collection, b"old collection").unwrap();
        let mut transaction =
            SoundTransaction::begin(&dir, &[sound("satanic", "wav", b"sound")]).unwrap();
        transaction
            .replace_file(&settings, b"new settings")
            .unwrap();
        transaction
            .replace_file(&collection, b"new collection")
            .unwrap();
        transaction.commit();
        assert_eq!(std::fs::read(&settings).unwrap(), b"new settings");
        assert_eq!(std::fs::read(&collection).unwrap(), b"new collection");
        assert_eq!(std::fs::read(dir.join("satanic.wav")).unwrap(), b"sound");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 3);
        remove_directory(&dir);
    }

    #[test]
    fn restoring_a_builtin_sound_is_reversible_until_commit() {
        let dir = directory();
        std::fs::write(dir.join("satanic.mp3"), b"old custom").unwrap();
        {
            let mut transaction = SoundTransaction::begin(&dir, &[]).unwrap();
            transaction.remove_sound("satanic").unwrap();
            assert!(!dir.join("satanic.mp3").exists());
        }
        assert_eq!(
            std::fs::read(dir.join("satanic.mp3")).unwrap(),
            b"old custom"
        );
        let mut transaction = SoundTransaction::begin(&dir, &[]).unwrap();
        transaction.remove_sound("satanic").unwrap();
        transaction.commit();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        remove_directory(&dir);
    }
}
