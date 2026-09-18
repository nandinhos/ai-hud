//! MiniMax usage adapter (official API & Token Plan).
//! Endpoints:
//!   - Primary: GET https://api.minimax.io/v1/token_plan/remains
//!   - Fallback: GET https://www.minimax.io/v1/token_plan/remains
//! Headers: Authorization: Bearer <token>
//!
//! Queries the token plan remains info from MiniMax Platform:
//!   - current_interval_usage_count / current_interval_total_count (session window)
//!   - current_weekly_usage_count / current_weekly_total_count (weekly window)
//!   - model_name
//!
//! When no API key is provided, the provider reports status = "absent".

use crate::usage::{LimitWindow, UsageSnapshot};
use crate::AppState;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const PRIMARY_ENDPOINT: &str = "https://api.minimax.io/v1/token_plan/remains";
const FALLBACK_ENDPOINT: &str = "https://www.minimax.io/v1/token_plan/remains";
const POLL_SECS: u64 = 300;

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn store_path() -> PathBuf {
    crate::config::config_path().with_file_name("minimax.json")
}

pub fn load_persisted() -> UsageSnapshot {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|t| serde_json::from_str::<UsageSnapshot>(&t).ok())
        .map(|mut s| {
            if !s.windows.is_empty() {
                s.status = "stale".into();
            }
            s
        })
        .unwrap_or_default()
}

fn persist(s: &UsageSnapshot) {
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(store_path(), t);
    }
}

#[derive(Deserialize, Default, Clone)]
pub struct ModelRemain {
    #[serde(default)]
    pub start_time: Option<u64>,
    #[serde(default)]
    pub end_time: Option<u64>,
    #[serde(default)]
    pub remains_time: Option<u64>,
    #[serde(default)]
    pub current_interval_total_count: Option<f64>,
    #[serde(default)]
    pub current_interval_usage_count: Option<f64>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub current_weekly_total_count: Option<f64>,
    #[serde(default)]
    pub current_weekly_usage_count: Option<f64>,
    #[serde(default)]
    pub weekly_start_time: Option<u64>,
    #[serde(default)]
    pub weekly_end_time: Option<u64>,
    #[serde(default)]
    pub weekly_remains_time: Option<u64>,
}

#[derive(Deserialize, Default)]
pub struct BaseResp {
    #[serde(default)]
    pub status_code: Option<i32>,
    #[serde(default)]
    pub status_msg: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct RemainsResponse {
    #[serde(default)]
    pub model_remains: Vec<ModelRemain>,
    #[serde(default)]
    pub base_resp: Option<BaseResp>,
}

pub fn resolve_api_key() -> Option<String> {
    // 1. Environment variable
    if let Ok(k) = std::env::var("MINIMAX_API_KEY") {
        let trimmed = k.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    // 2. ~/.config/minimax or ~/.minimax
    if let Some(home) = dirs::home_dir() {
        let paths = [
            home.join(".config").join("minimax").join("key.txt"),
            home.join(".config").join("minimax").join("auth.json"),
            home.join(".minimax").join("credentials.json"),
            home.join(".minimax").join("key.txt"),
        ];
        for p in paths {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(key) = v.get("api_key").or_else(|| v.get("key")).and_then(|k| k.as_str()) {
                        let trimmed = key.trim().to_string();
                        if !trimmed.is_empty() {
                            return Some(trimmed);
                        }
                    }
                } else {
                    let trimmed = content.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
    }

    // 3. User config (%APPDATA%\codenotch\minimax.key)
    let key_file = crate::config::config_path().with_file_name("minimax.key");
    if let Ok(content) = std::fs::read_to_string(&key_file) {
        let trimmed = content.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    None
}

fn fetch_with_url(endpoint: &str, token: &str) -> Result<RemainsResponse, String> {
    let resp = ureq::get(endpoint)
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", "Codenotch/1.12.0")
        .set("Accept", "application/json")
        .timeout(Duration::from_secs(15))
        .call();

    match resp {
        Ok(r) => r.into_json::<RemainsResponse>().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            Err("auth_rejected".into())
        }
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code}")),
        Err(e) => Err(e.to_string()),
    }
}

pub fn poll_once() -> UsageSnapshot {
    let key = match resolve_api_key() {
        Some(k) => k,
        None => {
            return UsageSnapshot {
                status: "absent".into(),
                windows: vec![],
                fetched_at: now_ms(),
                note: "Configure MINIMAX_API_KEY or %APPDATA%\\codenotch\\minimax.key to see usage.".into(),
                ..Default::default()
            };
        }
    };

    let resp = match fetch_with_url(PRIMARY_ENDPOINT, &key) {
        Ok(r) => r,
        Err(ref e) if e == "auth_rejected" => {
            return UsageSnapshot {
                status: "needsAuth".into(),
                windows: vec![],
                fetched_at: now_ms(),
                note: "MiniMax API key rejected — check your API key".into(),
                ..Default::default()
            };
        }
        Err(_) => match fetch_with_url(FALLBACK_ENDPOINT, &key) {
            Ok(r) => r,
            Err(e) => {
                return UsageSnapshot {
                    status: "error".into(),
                    windows: vec![],
                    fetched_at: now_ms(),
                    note: format!("MiniMax API error: {e}"),
                    ..Default::default()
                };
            }
        },
    };

    parse_remains_response(&resp)
}

pub fn parse_remains_response(resp: &RemainsResponse) -> UsageSnapshot {
    let mut windows = Vec::new();
    let now = now_ms();

    if let Some(item) = resp.model_remains.first() {
        let model = item.model_name.clone().unwrap_or_else(|| "MiniMax".into());

        // 1. Current interval usage (session window)
        if let (Some(total), Some(used_count)) = (item.current_interval_total_count, item.current_interval_usage_count) {
            if total > 0.0 {
                let ratio = (used_count / total).clamp(0.0, 1.0);
                windows.push(LimitWindow {
                    id: "interval".into(),
                    label: format!("Interval limit ({model})"),
                    used: ratio,
                    resets_at: item.end_time,
                    count: Some(used_count as i64),
                    derived: false,
                    group: Some(model.clone()),
                });
            }
        }

        // 2. Weekly quota window
        if let (Some(w_total), Some(w_used)) = (item.current_weekly_total_count, item.current_weekly_usage_count) {
            if w_total > 0.0 {
                let ratio = (w_used / w_total).clamp(0.0, 1.0);
                windows.push(LimitWindow {
                    id: "weekly".into(),
                    label: "Weekly limit".into(),
                    used: ratio,
                    resets_at: item.weekly_end_time,
                    count: Some(w_used as i64),
                    derived: false,
                    group: Some(model.clone()),
                });
            }
        }

        let note = if windows.is_empty() {
            format!("{model} · Pay-As-You-Go (No Token Plan active)")
        } else {
            format!("{model} Token Plan active")
        };

        UsageSnapshot {
            status: "ok".into(),
            windows,
            fetched_at: now,
            note,
            ..Default::default()
        }
    } else {
        // Chave válida, mas sem planos de token ativos (Pay-As-You-Go)
        windows.push(LimitWindow {
            id: "minimax-account".into(),
            label: "MiniMax Pay-As-You-Go".into(),
            used: 0.0,
            resets_at: None,
            count: None,
            derived: true,
            group: Some("MiniMax".into()),
        });

        UsageSnapshot {
            status: "ok".into(),
            windows,
            fetched_at: now,
            note: "MiniMax Connected · Pay-As-You-Go".into(),
            ..Default::default()
        }
    }
}

fn sleep_interruptible(secs: u64) {
    for _ in 0..secs {
        if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

pub fn start(app: AppHandle) {
    spawn_poller(app);
}

pub fn spawn_poller(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            let snap = poll_once();
            if snap.status != "absent" {
                persist(&snap);
            }
            {
                let st = app.state::<AppState>();
                let mut slot = st.minimax.lock().unwrap();
                *slot = snap.clone();
            }
            let _ = app.emit("minimax", &snap);

            sleep_interruptible(POLL_SECS);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_plan_remains_json() {
        let json = r#"{
            "model_remains": [
                {
                    "start_time": 1778616000000,
                    "end_time": 1778630400000,
                    "remains_time": 12241883,
                    "current_interval_total_count": 1500,
                    "current_interval_usage_count": 150,
                    "model_name": "MiniMax-Text-01",
                    "current_weekly_total_count": 10000,
                    "current_weekly_usage_count": 500,
                    "weekly_start_time": 1778457600000,
                    "weekly_end_time": 1779062400000,
                    "weekly_remains_time": 444241883
                }
            ],
            "base_resp": {
                "status_code": 0,
                "status_msg": "success"
            }
        }"#;

        let resp: RemainsResponse = serde_json::from_str(json).expect("valid json");
        let snap = parse_remains_response(&resp);

        assert_eq!(snap.status, "ok");
        assert_eq!(snap.windows.len(), 2);
        // Interval: 150 / 1500 = 0.10 (10%)
        assert!((snap.windows[0].used - 0.10).abs() < 1e-4);
        assert_eq!(snap.windows[0].resets_at, Some(1778630400000));
        // Weekly: 500 / 10000 = 0.05 (5%)
        assert!((snap.windows[1].used - 0.05).abs() < 1e-4);
        assert_eq!(snap.windows[1].resets_at, Some(1779062400000));
    }

    #[test]
    fn handles_empty_token_plan_gracefully() {
        let resp = RemainsResponse::default();
        let snap = parse_remains_response(&resp);
        assert_eq!(snap.status, "ok");
        assert_eq!(snap.windows.len(), 1);
        assert!(snap.windows[0].derived);
        assert_eq!(snap.windows[0].used, 0.0);
    }
}
