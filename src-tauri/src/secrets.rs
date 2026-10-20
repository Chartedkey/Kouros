//! Secrets live in the OS credential store (Windows Credential Manager),
//! never in files next to the data. The UI can save, replace or delete a
//! secret and ask whether it exists, but never read one back.

use keyring::Entry;

const SERVICE: &str = "Kouros";
const DB_KEY: &str = "db_key";

/// Secrets the user may manage from Settings.
pub const USER_SECRETS: [&str; 4] = [
    "strava_client_id",
    "strava_client_secret",
    "freddy_api_key",
    "anthropic_api_key",
];

fn entry(name: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, name).map_err(|e| format!("Хранилище ключей Windows недоступно: {e}"))
}

pub fn is_user_secret(name: &str) -> bool {
    USER_SECRETS.contains(&name)
}

pub fn get(name: &str) -> Result<Option<String>, String> {
    match entry(name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Не удалось прочитать ключ «{name}»: {e}")),
    }
}

pub fn has(name: &str) -> bool {
    matches!(get(name), Ok(Some(_)))
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    entry(name)?
        .set_password(value)
        .map_err(|e| format!("Не удалось сохранить ключ «{name}»: {e}"))
}

pub fn delete(name: &str) -> Result<(), String> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Не удалось удалить ключ «{name}»: {e}")),
    }
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
