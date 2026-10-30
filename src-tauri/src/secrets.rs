//! Secrets live in the OS credential store (Windows Credential Manager),
//! never in files next to the data. The UI can save, replace or delete a
//! secret and ask whether it exists, but never read one back.
//! 2.0, the phone: Android keeps each app's private folder closed to other apps, and the app is
//! unlocked with a fingerprint — secrets are a file there (`secrets.json`), outside the data folder.

#[allow(dead_code)]
const SERVICE: &str = "Kouros";
const DB_KEY: &str = "db_key";

/// Secrets the user may manage from Settings.
pub const USER_SECRETS: [&str; 4] = [
    "strava_client_id",
    "strava_client_secret",
    "freddy_api_key",
    "anthropic_api_key",
];

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod store {
    use keyring::Entry;

    fn entry(name: &str) -> Result<Entry, String> {
        Entry::new(super::SERVICE, name).map_err(|e| format!("Хранилище ключей Windows недоступно: {e}"))
    }

    pub fn get(name: &str) -> Result<Option<String>, String> {
        match entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Не удалось прочитать ключ «{name}»: {e}")),
        }
    }

    pub fn set(name: &str, value: &str) -> Result<(), String> {
        entry(name)?.set_password(value).map_err(|e| format!("Не удалось сохранить ключ «{name}»: {e}"))
    }

    pub fn delete(name: &str) -> Result<(), String> {
        match entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Не удалось удалить ключ «{name}»: {e}")),
        }
    }

    pub fn set_dir(_dir: std::path::PathBuf) {}
}

#[cfg(any(target_os = "android", target_os = "ios"))]
mod store {
    use serde_json::{Map, Value};
    use std::path::PathBuf;
    use std::sync::Mutex;

    static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

    pub fn set_dir(dir: PathBuf) {
        *DIR.lock().unwrap() = Some(dir);
    }

    fn path() -> Result<PathBuf, String> {
        DIR.lock().unwrap().clone().map(|d| d.join("secrets.json")).ok_or_else(|| "Хранилище ключей ещё не готово".to_string())
    }

    fn load() -> Result<Map<String, Value>, String> {
        let p = path()?;
        match std::fs::read_to_string(&p) {
            Ok(t) => Ok(serde_json::from_str::<Value>(&t).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
            Err(e) => Err(format!("Не удалось прочитать ключи: {e}")),
        }
    }

    fn save(m: &Map<String, Value>) -> Result<(), String> {
        let p = path()?;
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, Value::Object(m.clone()).to_string()).map_err(|e| format!("Не удалось сохранить ключи: {e}"))?;
        std::fs::rename(&tmp, &p).map_err(|e| format!("Не удалось сохранить ключи: {e}"))
    }

    pub fn get(name: &str) -> Result<Option<String>, String> {
        Ok(load()?.get(name).and_then(|v| v.as_str()).map(String::from))
    }

    pub fn set(name: &str, value: &str) -> Result<(), String> {
        let mut m = load()?;
        m.insert(name.to_string(), Value::String(value.to_string()));
        save(&m)
    }

    pub fn delete(name: &str) -> Result<(), String> {
        let mut m = load()?;
        if m.remove(name).is_some() {
            save(&m)?;
        }
        Ok(())
    }
}

/// Where the phone keeps its secrets (the app's private folder). No-op on the computer.
pub fn set_dir(dir: std::path::PathBuf) {
    store::set_dir(dir)
}

pub fn is_user_secret(name: &str) -> bool {
    USER_SECRETS.contains(&name)
}

pub fn get(name: &str) -> Result<Option<String>, String> {
    store::get(name)
}

pub fn has(name: &str) -> bool {
    matches!(get(name), Ok(Some(_)))
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    store::set(name, value)
}

pub fn delete(name: &str) -> Result<(), String> {
    store::delete(name)
}

/// The 256-bit database key, hex-encoded. Created only for a brand-new database:
/// if a database already exists but the key is missing, we refuse rather than
/// silently making a new key that could never open the old data.
pub fn db_key(allow_create: bool) -> Result<String, String> {
    if let Some(k) = get(DB_KEY)? {
        return Ok(k);
    }
    if !allow_create {
        return Err("Ключ базы данных не найден в хранилище Windows. Если это новый компьютер, \
                    восстановите его из ключа восстановления."
            .into());
    }
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("Не удалось создать ключ: {e}"))?;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    set(DB_KEY, &hex)?;
    Ok(hex)
}
