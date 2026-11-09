//! Secrets live in the OS credential store (Windows Credential Manager),
//! never in files next to the data. The UI can save, replace or delete a
//! secret and ask whether it exists, but never read one back.
//! Android: AES-GCM vault in noBackupFilesDir; encryption key is non-exportable in Android Keystore.

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

    pub fn set_dir(_dir: std::path::PathBuf) -> Result<(), String> { Ok(()) }
}

#[cfg(target_os = "android")]
mod store {
    pub fn set_dir(dir: std::path::PathBuf) -> Result<(), String> {
        // Resume-safe migration from the old Android prototype. Never overwrite a conflicting key.
        let path = dir.join("secrets.json");
        if !path.exists() { return Ok(()); }
        let text = std::fs::read_to_string(&path).map_err(|_| "Не удалось прочитать старое хранилище")?;
        let old: std::collections::BTreeMap<String, String> = serde_json::from_str(&text)
            .map_err(|_| "Старое хранилище повреждено; данные не сброшены")?;
        for (name, value) in old {
            match get(&name)? {
                Some(current) if current != value => return Err("Конфликт ключей при переносе хранилища".into()),
                Some(_) => {},
                None => set(&name, &value)?,
            }
            if get(&name)?.as_deref() != Some(&value) { return Err("Не удалось проверить перенос ключа".into()); }
        }
        std::fs::remove_file(path).map_err(|_| "Ключи перенесены, но старый файл не удалён".into())
    }
    pub fn get(name: &str) -> Result<Option<String>, String> { crate::mobile::secret("get", name, None) }
    pub fn set(name: &str, value: &str) -> Result<(), String> { crate::mobile::secret("set", name, Some(value)).map(|_| ()) }
    pub fn delete(name: &str) -> Result<(), String> { crate::mobile::secret("delete", name, None).map(|_| ()) }
}

#[cfg(target_os = "ios")]
compile_error!("iOS secure storage has not been implemented; do not ship the Android adapter on iOS.");

/// Where the phone keeps its secrets (the app's private folder). No-op on the computer.
pub fn set_dir(dir: std::path::PathBuf) -> Result<(), String> {
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
