use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::open_url::open_https_url;

const API_BASE: &str = "https://thermaltrace.dev";
const STALE_AFTER_SECS: u64 = 30 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTokens {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveBuddyPayload {
    pub connected: bool,
    pub space_name: String,
    pub temperature_f: Option<f64>,
    pub freeze_threshold_f: f64,
    pub freeze_margin_f: Option<f64>,
    pub time_to_freeze_hours: Option<f64>,
    pub door_open: bool,
    pub wet_contact: bool,
    pub feed_healthy: bool,
    pub spaces: Vec<String>,
    pub last_updated: String,
}

fn session_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("config dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create config dir: {e}"))?;
    Ok(dir.join("session.json"))
}

pub fn load_session(app: &AppHandle) -> Option<SessionTokens> {
    let path = session_path(app).ok()?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_session(app: &AppHandle, tokens: &SessionTokens) -> Result<(), String> {
    let path = session_path(app)?;
    let raw = serde_json::to_string_pretty(tokens).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

pub fn clear_session(app: &AppHandle) -> Result<(), String> {
    let path = session_path(app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &value[i + 1..i + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_exchange_from_request(request: &str) -> Option<String> {
    let (headers, body) = request
        .split_once("\r\n\r\n")
        .or_else(|| request.split_once("\n\n"))
        .unwrap_or((request, ""));

    let first = headers.lines().next()?;
    let path = first.split_whitespace().nth(1)?;

    // GET /oauth?exchange=...
    if let Some(query) = path.split('?').nth(1) {
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or("");
            if key == "exchange" {
                return Some(percent_decode(value));
            }
        }
    }

    // POST application/x-www-form-urlencoded body: exchange=...
    for pair in body.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next().unwrap_or("");
        if key == "exchange" {
            return Some(percent_decode(value.trim()));
        }
    }

    None
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .ok();

    let mut data = Vec::with_capacity(64 * 1024);
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if data.len() > 512 * 1024 {
            return Err("Browser return request too large".into());
        }
        let text = String::from_utf8_lossy(&data);
        if let Some(header_end) = text.find("\r\n\r\n").or_else(|| text.find("\n\n")) {
            let sep_len = if text[header_end..].starts_with("\r\n\r\n") {
                4
            } else {
                2
            };
            let headers = &text[..header_end];
            let mut content_length = 0usize;
            for line in headers.lines().skip(1) {
                let lower = line.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    content_length = v.trim().parse().unwrap_or(0);
                }
            }
            let body_start = header_end + sep_len;
            if data.len() >= body_start + content_length {
                break;
            }
        }
    }
    Ok(String::from_utf8_lossy(&data).into_owned())
}

fn wait_for_loopback_exchange(listener: TcpListener) -> Result<String, String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;

    let started = std::time::Instant::now();
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(conn) => break conn,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed() > Duration::from_secs(300) {
                    return Err("Timed out waiting for browser sign-in".into());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(err) => return Err(format!("waiting for browser return: {err}")),
        }
    };

    stream
        .set_nonblocking(false)
        .map_err(|e| e.to_string())?;

    let request = read_http_request(&mut stream)?;
    let exchange = parse_exchange_from_request(&request).ok_or_else(|| {
        format!(
            "Browser returned without an exchange token (received {} bytes)",
            request.len()
        )
    })?;

    let body = "<!doctype html><html><body style=\"font-family:system-ui;background:#090b0f;color:#f3f6fb;padding:2rem\"><h1>Bay Buddy connected</h1><p>You can close this tab and return to the app.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    Ok(exchange)
}

async fn exchange_token(exchange: &str) -> Result<SessionTokens, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{API_BASE}/api/auth/mobile/exchange"))
        .header("Accept", "application/json")
        .json(&serde_json::json!({ "exchange_token": exchange }))
        .send()
        .await
        .map_err(|e| format!("exchange request failed: {e}"))?;

    if !res.status().is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("exchange failed: {body}"));
    }

    #[derive(Deserialize)]
    struct ExchangeResponse {
        access_token: String,
        refresh_token: String,
    }

    let body: ExchangeResponse = res
        .json()
        .await
        .map_err(|e| format!("exchange parse failed: {e}"))?;

    Ok(SessionTokens {
        access_token: body.access_token,
        refresh_token: body.refresh_token,
    })
}

/// Open ThermalTrace companion sign-in and wait for the loopback handoff.
#[tauri::command]
pub async fn start_companion_login(
    app: AppHandle,
    provider: Option<String>,
) -> Result<LiveBuddyPayload, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let loopback = format!("http://127.0.0.1:{port}/oauth");

    let mut start_url = format!(
        "{API_BASE}/api/auth/companion/start?client=baybuddy&loopback={}",
        percent_encode(&loopback)
    );
    if let Some(provider) = provider {
        let p = provider.trim().to_lowercase();
        if matches!(p.as_str(), "google" | "github" | "discord") {
            start_url.push_str("&provider=");
            start_url.push_str(&p);
        }
    }

    open_https_url(&start_url)?;

    let exchange = tauri::async_runtime::spawn_blocking(move || wait_for_loopback_exchange(listener))
        .await
        .map_err(|e| format!("login wait task failed: {e}"))??;

    let tokens = exchange_token(&exchange).await?;
    save_session(&app, &tokens)?;
    let _ = app.emit("auth://connected", ());
    fetch_live_buddy_with_tokens(&tokens, None).await
}

#[tauri::command]
pub fn disconnect_companion(app: AppHandle) -> Result<(), String> {
    clear_session(&app)?;
    let _ = app.emit("auth://disconnected", ());
    Ok(())
}

#[tauri::command]
pub fn has_companion_session(app: AppHandle) -> bool {
    load_session(&app).is_some()
}

#[derive(Debug, Deserialize)]
struct ReadingsResponse {
    sensors: Option<Vec<SensorCard>>,
    spaces: Option<Vec<String>>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(rename = "updatedAt", default)]
    updated_at_camel: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SensorCard {
    space: Option<String>,
    kind: Option<String>,
    value_num: Option<f64>,
    value_bool: Option<bool>,
    value_text: Option<String>,
    recorded_at: Option<String>,
    temp: Option<TempReading>,
}

#[derive(Debug, Deserialize)]
struct TempReading {
    f: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct InsightsResponse {
    freeze_threshold_f: Option<f64>,
    time_to_freeze: Option<TimeToFreeze>,
}

#[derive(Debug, Deserialize)]
struct TimeToFreeze {
    hours: Option<f64>,
}

async fn api_get_json<T: for<'de> Deserialize<'de>>(
    tokens: &SessionTokens,
    path: &str,
) -> Result<T, String> {
    api_json::<T, ()>(tokens, reqwest::Method::GET, path, None).await
}

pub(crate) async fn api_get_json_pub<T: for<'de> Deserialize<'de>>(
    tokens: &SessionTokens,
    path: &str,
) -> Result<T, String> {
    api_get_json(tokens, path).await
}

pub(crate) async fn api_post_json<T: for<'de> Deserialize<'de>, B: Serialize>(
    tokens: &SessionTokens,
    path: &str,
    body: &B,
) -> Result<T, String> {
    api_json(tokens, reqwest::Method::POST, path, Some(body)).await
}

async fn api_json<T: for<'de> Deserialize<'de>, B: Serialize>(
    tokens: &SessionTokens,
    method: reqwest::Method,
    path: &str,
    body: Option<&B>,
) -> Result<T, String> {
    if tokens.access_token.is_empty() || tokens.refresh_token.is_empty() {
        return Err("Missing session tokens after exchange".into());
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client
        .request(method, format!("{API_BASE}{path}"))
        .header("Accept", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", tokens.access_token),
        )
        .header("X-SB-Refresh-Token", tokens.refresh_token.as_str())
        .header("X-SB-MFA-Required", "0");

    if let Some(payload) = body {
        req = req
            .header("Content-Type", "application/json")
            .json(payload);
    }

    let res = req
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = res.status().as_u16();
    if status == 401 {
        let body = res.text().await.unwrap_or_default();
        if body.contains("MFA required") {
            return Err(
                "MFA required — finish MFA in the browser on thermaltrace.dev, then connect again"
                    .into(),
            );
        }
        return Err(format!(
            "Session rejected by ThermalTrace (HTTP 401). access_len={} refresh_len={} body={}",
            tokens.access_token.len(),
            tokens.refresh_token.len(),
            body.chars().take(180).collect::<String>()
        ));
    }
    if !(200..300).contains(&status) {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("API error HTTP {status}: {body}"));
    }

    res.json::<T>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

fn recorded_age_secs(recorded_at: &str) -> Option<u64> {
    // Expect RFC3339 / ISO-8601 UTC timestamps from the API.
    let trimmed = recorded_at.trim().trim_end_matches('Z');
    let (date, time) = trimmed.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u64 = date_parts.next()?.parse().ok()?;
    let day: u64 = date_parts.next()?.parse().ok()?;
    let time = time.split(['+', '-', '.']).next()?;
    let mut time_parts = time.split(':');
    let hour: u64 = time_parts.next()?.parse().ok()?;
    let minute: u64 = time_parts.next()?.parse().ok()?;
    let second: u64 = time_parts.next()?.parse().ok()?;

    // Days from civil date (Howard Hinnant algorithm) → unix seconds
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (153 * mp + 2) / 5 + day - 1;
    let days = (era * 146097) as i64 + doe as i64 - 719468;
    let unix = days * 86400 + (hour as i64) * 3600 + (minute as i64) * 60 + second as i64;
    let then = UNIX_EPOCH + Duration::from_secs(unix.max(0) as u64);
    SystemTime::now().duration_since(then).ok().map(|d| d.as_secs())
}

fn is_stale(recorded_at: Option<&str>) -> bool {
    match recorded_at {
        Some(raw) => recorded_age_secs(raw)
            .map(|age| age > STALE_AFTER_SECS)
            .unwrap_or(false),
        None => true,
    }
}

pub async fn fetch_live_buddy(
    app: &AppHandle,
    space: Option<String>,
) -> Result<LiveBuddyPayload, String> {
    let tokens = load_session(app).ok_or_else(|| "Not connected".to_string())?;
    fetch_live_buddy_with_tokens(&tokens, space).await
}

pub async fn fetch_live_buddy_with_tokens(
    tokens: &SessionTokens,
    space: Option<String>,
) -> Result<LiveBuddyPayload, String> {
    let space_q = space
        .as_deref()
        .map(|s| format!("&space={}", percent_encode(s)))
        .unwrap_or_default();

    let readings: ReadingsResponse =
        api_get_json(tokens, &format!("/api/home/readings?save=0{space_q}")).await?;
    let insights: InsightsResponse = api_get_json(tokens, "/api/user/home-insights").await?;

    let sensors = readings.sensors.unwrap_or_default();
    let spaces = readings.spaces.unwrap_or_default();

    let mut space_counts: HashMap<String, usize> = HashMap::new();
    for sensor in &sensors {
        if let Some(space_name) = sensor.space.as_deref() {
            *space_counts.entry(space_name.to_string()).or_default() += 1;
        }
    }
    let space_name = space
        .or_else(|| spaces.first().cloned())
        .or_else(|| {
            space_counts
                .into_iter()
                .max_by_key(|(_, n)| *n)
                .map(|(name, _)| name)
        })
        .unwrap_or_else(|| "Garage".into());

    let in_space = |sensor: &SensorCard| {
        sensor
            .space
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case(&space_name))
            .unwrap_or(spaces.len() <= 1)
    };

    let mut temps: Vec<f64> = Vec::new();
    let mut door_open = false;
    let mut wet_contact = false;
    let mut any_fresh = false;

    for sensor in sensors.iter().filter(|s| in_space(s)) {
        let kind = sensor.kind.as_deref().unwrap_or("");
        if !is_stale(sensor.recorded_at.as_deref()) {
            any_fresh = true;
        }
        match kind {
            "temperature" => {
                if let Some(f) = sensor.temp.as_ref().and_then(|t| t.f).or(sensor.value_num) {
                    temps.push(f);
                }
            }
            "door" => {
                if sensor.value_bool == Some(true)
                    || sensor
                        .value_text
                        .as_deref()
                        .is_some_and(|v| v.eq_ignore_ascii_case("open"))
                {
                    door_open = true;
                }
            }
            "flood" | "leak" => {
                if sensor.value_bool == Some(true)
                    || sensor.value_text.as_deref().is_some_and(|v| {
                        matches!(
                            v.to_ascii_lowercase().as_str(),
                            "wet" | "leak" | "true" | "1"
                        )
                    })
                {
                    wet_contact = true;
                }
            }
            _ => {}
        }
    }

    let temperature_f = temps.iter().copied().reduce(f64::min);
    let freeze_threshold_f = insights.freeze_threshold_f.unwrap_or(35.0);
    let freeze_margin_f = temperature_f.map(|t| t - freeze_threshold_f);
    let time_to_freeze_hours = insights.time_to_freeze.and_then(|t| t.hours);
    let feed_healthy = any_fresh || temperature_f.is_some();

    Ok(LiveBuddyPayload {
        connected: true,
        space_name,
        temperature_f,
        freeze_threshold_f,
        freeze_margin_f,
        time_to_freeze_hours,
        door_open,
        wet_contact,
        feed_healthy,
        spaces,
        last_updated: readings
            .updated_at_camel
            .or(readings.updated_at)
            .unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs().to_string())
                    .unwrap_or_default()
            }),
    })
}

#[tauri::command]
pub async fn fetch_buddy_state(
    app: AppHandle,
    space: Option<String>,
) -> Result<LiveBuddyPayload, String> {
    fetch_live_buddy(&app, space).await
}
