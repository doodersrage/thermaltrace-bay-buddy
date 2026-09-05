//! Claim-puck serial + ThermalTrace claim/follow helpers.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tauri::AppHandle;

use crate::auth::{api_post_json, load_session, SessionTokens};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialPortInfo {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimPuckResult {
    pub device_id: String,
    pub bay_id: String,
    pub space_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BayMoodPayload {
    pub bay_id: String,
    pub mood: String,
    pub space_name: Option<String>,
    pub source: Option<String>,
}

#[derive(Deserialize)]
struct ClaimStartBody {
    nonce_hex: String,
}

#[derive(Deserialize)]
struct ClaimFinishBody {
    bay_id: String,
    space_name: String,
}

#[derive(Deserialize)]
struct BayMoodApi {
    bay_id: String,
    mood: String,
    space_name: Option<String>,
    source: Option<String>,
}

fn open_serial(path: &str) -> Result<Box<dyn serialport::SerialPort>, String> {
    serialport::new(path, 115_200)
        .timeout(Duration::from_millis(200))
        .open()
        .map_err(|e| format!("open {path}: {e}"))
}

fn read_response_line(
    port: &mut dyn serialport::SerialPort,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while Instant::now() < deadline {
        match port.read(&mut byte) {
            Ok(0) => continue,
            Ok(_) => {
                if byte[0] == b'\n' || byte[0] == b'\r' {
                    if !buf.is_empty() {
                        let line = String::from_utf8_lossy(&buf).trim().to_string();
                        if line.starts_with("OK")
                            || line.starts_with("ERR")
                            || line.starts_with("RESPONSE")
                        {
                            return Ok(line);
                        }
                        buf.clear();
                    }
                    continue;
                }
                buf.push(byte[0]);
                if buf.len() > 512 {
                    return Err("serial line too long".into());
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(format!("serial read: {e}")),
        }
    }
    Err("device timed out".into())
}

fn puck_command(path: &str, line: &str, timeout: Duration) -> Result<String, String> {
    let mut port = open_serial(path)?;
    // Drain boot chatter briefly.
    let drain_until = Instant::now() + Duration::from_millis(250);
    let mut discard = [0u8; 64];
    while Instant::now() < drain_until {
        let _ = port.read(&mut discard);
    }
    let payload = format!("{}\n", line.trim());
    port.write_all(payload.as_bytes())
        .map_err(|e| format!("serial write: {e}"))?;
    port.flush().map_err(|e| format!("serial flush: {e}"))?;
    read_response_line(port.as_mut(), timeout)
}

fn info_field(info: &str, key: &str) -> Option<String> {
    info.split_whitespace().find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k == key {
            Some(v.to_string())
        } else {
            None
        }
    })
}

fn random_secret_hex() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn hmac_hex(secret_hex: &str, nonce_hex: &str) -> Result<String, String> {
    let secret = hex::decode(secret_hex).map_err(|e| e.to_string())?;
    let nonce = hex::decode(nonce_hex).map_err(|e| e.to_string())?;
    let mut mac =
        HmacSha256::new_from_slice(&secret).map_err(|e| format!("hmac key: {e}"))?;
    mac.update(&nonce);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

pub fn normalize_bay_id(raw: &str) -> Result<String, String> {
    let mut out = String::new();
    for c in raw.trim().chars() {
        let mapped = if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-') {
            c.to_ascii_lowercase()
        } else if c.is_whitespace() {
            '_'
        } else {
            continue;
        };
        out.push(mapped);
        if out.len() >= 32 {
            break;
        }
    }
    if out.is_empty() {
        return Err("bay id is empty".into());
    }
    if !out
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
    {
        return Err("invalid bay id".into());
    }
    Ok(out)
}

#[tauri::command]
pub fn list_serial_ports() -> Result<Vec<SerialPortInfo>, String> {
    let ports = serialport::available_ports().map_err(|e| e.to_string())?;
    let mut out: Vec<SerialPortInfo> = ports
        .into_iter()
        .map(|p| {
            let name = match &p.port_type {
                serialport::SerialPortType::UsbPort(usb) => {
                    let product = usb.product.clone().unwrap_or_default();
                    if product.is_empty() {
                        p.port_name.clone()
                    } else {
                        format!("{} ({})", p.port_name, product)
                    }
                }
                _ => p.port_name.clone(),
            };
            SerialPortInfo {
                path: p.port_name,
                name,
            }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[tauri::command]
pub fn puck_info(port: String) -> Result<String, String> {
    puck_command(&port, "INFO", Duration::from_secs(4))
}

#[tauri::command]
pub fn push_puck_mood(port: String, mood: String) -> Result<String, String> {
    let mood = mood.trim().to_ascii_lowercase();
    const VALID: &[&str] = &["cozy", "drafty", "shiver", "panic", "offline", "hero"];
    if !VALID.contains(&mood.as_str()) {
        return Err(format!("bad mood: {mood}"));
    }
    puck_command(&port, &format!("MOOD {mood}"), Duration::from_secs(4))
}

async fn register_device(
    tokens: &SessionTokens,
    device_id: &str,
    secret_hex: &str,
) -> Result<(), String> {
    let _: serde_json::Value = api_post_json(
        tokens,
        "/api/pucks/register",
        &serde_json::json!({
            "device_id": device_id,
            "secret_hex": secret_hex,
        }),
    )
    .await?;
    Ok(())
}

fn provision_device(port: &str) -> Result<(String, String), String> {
    let secret_hex = random_secret_hex();
    let resp = puck_command(
        port,
        &format!("PROVISION {secret_hex}"),
        Duration::from_secs(5),
    )?;
    if !resp.starts_with("OK") {
        return Err(resp);
    }
    let device_id = info_field(&resp, "device")
        .filter(|d| d.len() == 32)
        .or_else(|| {
            puck_command(port, "INFO", Duration::from_secs(4))
                .ok()
                .and_then(|info| info_field(&info, "device"))
        })
        .ok_or_else(|| "provision ok but device id missing".to_string())?;
    if device_id.len() != 32 {
        return Err("bad device id from puck".into());
    }
    Ok((device_id, secret_hex))
}

#[tauri::command]
pub async fn claim_puck(
    app: AppHandle,
    port: String,
    bay_id: String,
    space_name: Option<String>,
) -> Result<ClaimPuckResult, String> {
    let tokens = load_session(&app).ok_or_else(|| "Not connected".to_string())?;
    let bay_id = normalize_bay_id(&bay_id)?;
    let space_name = space_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| bay_id.clone());

    let port_for_info = port.clone();
    let info = tauri::async_runtime::spawn_blocking(move || {
        puck_command(&port_for_info, "INFO", Duration::from_secs(4))
    })
    .await
    .map_err(|e| e.to_string())??;

    let provisioned = info_field(&info, "provisioned").as_deref() == Some("yes");
    let mut device_id = info_field(&info, "device").filter(|d| d.len() == 32);
    let mut secret_hex: Option<String> = None;
    let mut pending_nonce: Option<String> = None;

    if !provisioned || device_id.is_none() {
        let port_prov = port.clone();
        let (id, secret) = tauri::async_runtime::spawn_blocking(move || provision_device(&port_prov))
            .await
            .map_err(|e| e.to_string())??;
        register_device(&tokens, &id, &secret).await?;
        device_id = Some(id);
        secret_hex = Some(secret);
    } else if let Some(ref id) = device_id {
        match api_post_json::<ClaimStartBody, _>(
            &tokens,
            "/api/pucks/claim/start",
            &serde_json::json!({
                "device_id": id,
                "bay_id": bay_id,
            }),
        )
        .await
        {
            Ok(start) => {
                pending_nonce = Some(start.nonce_hex);
            }
            Err(_) => {
                let port_wipe = port.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    let _ = puck_command(&port_wipe, "WIPE", Duration::from_secs(4));
                })
                .await;
                let port_prov = port.clone();
                let (id, secret) =
                    tauri::async_runtime::spawn_blocking(move || provision_device(&port_prov))
                        .await
                        .map_err(|e| e.to_string())??;
                register_device(&tokens, &id, &secret).await?;
                device_id = Some(id);
                secret_hex = Some(secret);
            }
        }
    }

    let device_id = device_id.ok_or_else(|| "missing device id".to_string())?;

    let nonce = if let Some(n) = pending_nonce {
        n
    } else {
        let start: ClaimStartBody = api_post_json(
            &tokens,
            "/api/pucks/claim/start",
            &serde_json::json!({
                "device_id": device_id,
                "bay_id": bay_id,
            }),
        )
        .await?;
        start.nonce_hex
    };
    let port_chal = port.clone();
    let challenge_line = format!("CHALLENGE {nonce}");
    let response_line = tauri::async_runtime::spawn_blocking(move || {
        puck_command(&port_chal, &challenge_line, Duration::from_secs(35))
    })
    .await
    .map_err(|e| e.to_string())??;

    if !response_line.starts_with("RESPONSE ") {
        return Err(response_line);
    }
    let response_hex = response_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "empty RESPONSE".to_string())?
        .to_ascii_lowercase();

    if let Some(secret) = secret_hex.as_deref() {
        let expect = hmac_hex(secret, &nonce)?;
        if expect != response_hex {
            return Err(format!("local verify failed expected={expect}"));
        }
    }

    let finish: ClaimFinishBody = api_post_json(
        &tokens,
        "/api/pucks/claim/finish",
        &serde_json::json!({
            "device_id": device_id,
            "bay_id": bay_id,
            "nonce_hex": nonce,
            "response_hex": response_hex,
            "space_name": space_name,
        }),
    )
    .await?;

    let port_cfg = port.clone();
    let bay_cfg = finish.bay_id.clone();
    let space_cfg = finish.space_name.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = puck_command(
            &port_cfg,
            &format!("CONFIG bay={bay_cfg} space={space_cfg}"),
            Duration::from_secs(4),
        )?;
        if !cfg.starts_with("OK") {
            return Err(cfg);
        }
        let mood = puck_command(&port_cfg, "MOOD cozy", Duration::from_secs(4))?;
        if !mood.starts_with("OK") {
            return Err(mood);
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(ClaimPuckResult {
        device_id,
        bay_id: finish.bay_id,
        space_name: finish.space_name,
        message: "Claimed — press was accepted. Mood light set to cozy.".into(),
    })
}

#[tauri::command]
pub async fn fetch_bay_mood(app: AppHandle, bay_id: String) -> Result<BayMoodPayload, String> {
    let tokens = load_session(&app).ok_or_else(|| "Not connected".to_string())?;
    let bay_id = normalize_bay_id(&bay_id)?;
    let path = format!("/api/bays/{bay_id}/mood");
    let body: BayMoodApi = crate::auth::api_get_json_pub(&tokens, &path).await?;
    Ok(BayMoodPayload {
        bay_id: body.bay_id,
        mood: body.mood,
        space_name: body.space_name,
        source: body.source,
    })
}
