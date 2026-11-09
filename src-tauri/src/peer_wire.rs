//! Authenticated encryption on every LAN message, including errors and attachments.
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{aead, digest};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
pub const MAX_WIRE: usize = 32 * 1024 * 1024;
pub const MAX_PLAIN: u64 = 96 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
pub struct Envelope {
    pub v: u8,
    pub nonce: String,
    pub data: String,
}
fn bytes(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Неверный код сопряжения".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "Неверный код сопряжения".into()))
        .collect()
}
pub fn normalized_code(code: &str) -> Result<String, String> {
    let s = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_lowercase();
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Введите 32 знака кода с компьютера".into());
    }
    Ok(s)
}
fn key(code: &str) -> Result<aead::LessSafeKey, String> {
    let b = bytes(&normalized_code(code)?)?;
    let h = digest::digest(&digest::SHA256, &b);
    let k = aead::UnboundKey::new(&aead::AES_256_GCM, h.as_ref()).map_err(|_| "Ключ недоступен")?;
    Ok(aead::LessSafeKey::new(k))
}
pub fn seal(code: &str, aad: &str, plain: &[u8]) -> Result<Envelope, String> {
    if plain.len() as u64 > MAX_PLAIN {
        return Err("Слишком большой пакет".into());
    }
    let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(plain).map_err(|e| e.to_string())?;
    let mut data = z.finish().map_err(|e| e.to_string())?;
    let mut nonce = [0; 12];
    getrandom::getrandom(&mut nonce).map_err(|e| e.to_string())?;
    key(code)?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut data,
        )
        .map_err(|_| "Ошибка шифрования")?;
    Ok(Envelope {
        v: 1,
        nonce: nonce.iter().map(|b| format!("{b:02x}")).collect(),
        data: STANDARD.encode(data),
    })
}
pub fn open(code: &str, aad: &str, e: &Envelope) -> Result<Vec<u8>, String> {
    if e.v != 1 || e.data.len() > MAX_WIRE || e.nonce.len() != 24 {
        return Err("Неверный пакет".into());
    }
    let n: [u8; 12] = bytes(&e.nonce)?.try_into().map_err(|_| "Неверный пакет")?;
    let mut data = STANDARD.decode(&e.data).map_err(|_| "Неверный пакет")?;
    let plain = key(code)?
        .open_in_place(
            aead::Nonce::assume_unique_for_key(n),
            aead::Aad::from(aad),
            &mut data,
        )
        .map_err(|_| "Код не подошёл или пакет повреждён")?;
    let mut result = Vec::new();
    flate2::read::GzDecoder::new(&*plain)
        .take(MAX_PLAIN + 1)
        .read_to_end(&mut result)
        .map_err(|_| "Повреждённый пакет")?;
    if result.len() as u64 > MAX_PLAIN {
        return Err("Слишком большой пакет".into());
    }
    Ok(result)
}
pub fn request_aad() -> &'static str {
    "kouros-wifi/1/request"
}
pub fn response_aad(nonce: &str) -> String {
    format!("kouros-wifi/1/response/{nonce}")
}
pub fn private_ipv4(s: &str) -> Result<std::net::Ipv4Addr, String> {
    let ip: std::net::Ipv4Addr = s
        .parse()
        .map_err(|_| "Введите локальный IPv4-адрес компьютера, например 192.168.1.10")?;
    if !(ip.is_private() || ip.is_loopback()) || ip.is_unspecified() {
        return Err("Синхронизация доступна только по локальной сети".into());
    }
    Ok(ip)
}

/// Strictly bounded HTTP framing; only encrypted POST bodies are accepted.
pub fn read_http(stream: &mut std::net::TcpStream) -> Result<Envelope, String> {
    use std::io::{BufRead, BufReader};
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let mut r = BufReader::new(stream);
    let mut line = Vec::new();
    let n = r
        .by_ref()
        .take(1025)
        .read_until(b'\n', &mut line)
        .map_err(|e| e.to_string())?;
    if n > 1024 || line != b"POST /kouros-sync/v1 HTTP/1.1\r\n" {
        return Err("Неверный запрос".into());
    }
    let mut length = None;
    let mut total = n;
    loop {
        line.clear();
        let n = r
            .by_ref()
            .take(8193)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        total += n;
        if n == 0 || total > 8192 {
            return Err("Слишком большие заголовки".into());
        }
        if line == b"\r\n" {
            break;
        }
        let text = std::str::from_utf8(&line).map_err(|_| "Неверные заголовки")?;
        let (k, v) = text.split_once(':').ok_or("Неверные заголовки")?;
        if k.eq_ignore_ascii_case("transfer-encoding") {
            return Err("Неверный формат запроса".into());
        }
        if k.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("Повторный размер запроса".into());
            }
            length = Some(
                v.trim()
                    .parse::<usize>()
                    .map_err(|_| "Неверный размер запроса")?,
            );
        }
    }
    let length = length.ok_or("Нет размера запроса")?;
    if length > MAX_WIRE {
        return Err("Слишком большой запрос".into());
    }
    let mut bytes = vec![0; length];
    r.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "Неверный пакет".into())
}
