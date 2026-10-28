use crate::language::Language;
use serde_json::{Map, Value};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub fn path() -> io::Result<PathBuf> {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|s| !s.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .ok_or_else(|| io::Error::other("HOME / XDG_CONFIG_HOME"))?;
    Ok(root.join("tui-atlas/config.json"))
}

fn read(path: &Path) -> io::Result<Map<String, Value>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Map<String, Value>>(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error),
    }
}

pub fn load(path: &Path) -> io::Result<Option<Language>> {
    Ok(read(path)?
        .get("language")
        .and_then(Value::as_str)
        .and_then(Language::parse))
}

pub fn save(path: &Path, language: Language) -> io::Result<()> {
    let mut data = read(path)?;
    data.insert("language".into(), Value::String(language.code().into()));
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("config.json"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let temporary = directory.join(format!(".config-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        serde_json::to_writer_pretty(&mut file, &data)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(directory)?.sync_all()
    })();
    // Удаляем только собственный временный файл, не затрагивая исходные настройки.
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_language_privately_preserves_unknown_fields_and_refuses_corrupt_data() {
        let root = std::env::temp_dir().join(format!("tui-atlas-settings-{}", std::process::id()));
        let path = root.join("config.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, r#"{"future":{"enabled":true},"language":"ru"}"#).unwrap();
        assert_eq!(load(&path).unwrap(), Some(Language::Ru));
        save(&path, Language::En).unwrap();
        assert_eq!(load(&path).unwrap(), Some(Language::En));
        assert_eq!(read(&path).unwrap()["future"]["enabled"], true);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let temporary = root.join(format!(".config-{}.tmp", std::process::id()));
        fs::write(&temporary, b"another writer").unwrap();
        assert!(save(&path, Language::Ru).is_err());
        assert_eq!(fs::read(&temporary).unwrap(), b"another writer");
        assert_eq!(load(&path).unwrap(), Some(Language::En));
        fs::remove_file(temporary).unwrap();
        fs::write(&path, b"{broken").unwrap();
        assert!(load(&path).is_err());
        assert!(save(&path, Language::Ru).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken");
        fs::remove_dir_all(root).unwrap();
    }
}
