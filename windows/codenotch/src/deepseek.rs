//! DeepSeek usage adapter (official API).
//! Endpoint: GET https://api.deepseek.com/user/balance
//! Headers: Authorization: Bearer <token>
//!
//! Queries the balance info from DeepSeek Platform:
//!   - total_balance: combined available amount
//!   - topped_up_balance: manually funded amount
//!   - granted_balance: promotional / trial grants
//!
//! When no API key is provided, the provider reports status = "absent" so no cell is rendered.

use crate::usage::{LimitWindow, UsageSnapshot};
use crate::AppState;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const ENDPOINT: &str = "https://api.deepseek.com/user/balance";
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
    crate::config::config_path().with_file_name("deepseek.json")
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

#[derive(Deserialize)]
struct BalanceInfo {
    currency: String,
    total_balance: String,
    granted_balance: String,
    topped_up_balance: String,
}

#[derive(Deserialize)]
struct BalanceResponse {
    is_available: bool,
    #[serde(default)]
    balance_infos: Vec<BalanceInfo>,
}

fn resolve_api_key() -> Option<String> {
    // 1. Environment variable
    if let Ok(k) = std::env::var("DEEPSEEK_API_KEY") {
        let trimmed = k.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    // 2. ~/.deepseek/credentials.json or ~/.deepseek/config.json
    if let Some(home) = dirs::home_dir() {
        let paths = [
            home.join(".deepseek").join("credentials.json"),
            home.join(".deepseek").join("config.json"),
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
                }
            }
        }
    }

    // 3. User config (%APPDATA%\codenotch\deepseek.key or config.json)
    let key_file = crate::config::config_path().with_file_name("deepseek.key");
    if let Ok(content) = std::fs::read_to_string(&key_file) {
        let trimmed = content.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    None
}

pub fn poll_once() -> UsageSnapshot {
    let key = match resolve_api_key() {
        Some(k) => k,
        None => {
            return UsageSnapshot {
                status: "absent".into(),
                windows: vec![],
                fetched_at: now_ms(),
                note: "No DEEPSEEK_API_KEY or %APPDATA%\\codenotch\\deepseek.key found".into(),
                ..Default::default()
            };
        }
    };

    let resp = match ureq::get(ENDPOINT)
        .set("Authorization", &format!("Bearer {key}"))
        .timeout(Duration::from_secs(15))
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            let note = match &e {
                ureq::Error::Status(401, _) | ureq::Error::Status(403, _) => "DeepSeek API key invalid (401/403)",
                ureq::Error::Status(429, _) => "DeepSeek API rate limited (429)",
                _ => "Failed to reach DeepSeek API",
            };
            return UsageSnapshot {
                status: if matches!(&e, ureq::Error::Status(401, _) | ureq::Error::Status(403, _)) {
                    "needsAuth".into()
                } else {
                    "error".into()
                },
                windows: vec![],
                fetched_at: now_ms(),
                note: format!("{note}: {e}"),
                ..Default::default()
            };
        }
    };

    let parsed: BalanceResponse = match resp.into_json() {
        Ok(b) => b,
        Err(e) => {
            return UsageSnapshot {
                status: "error".into(),
                windows: vec![],
                fetched_at: now_ms(),
                note: format!("Malformed DeepSeek balance payload: {e}"),
                ..Default::default()
            };
        }
    };

    if parsed.balance_infos.is_empty() {
        return UsageSnapshot {
            status: "error".into(),
            windows: vec![],
            fetched_at: now_ms(),
            note: "DeepSeek account balance unavailable".into(),
            ..Default::default()
        };
    }

    let mut windows = Vec::new();
    for info in &parsed.balance_infos {
        let total: f64 = info.total_balance.parse().unwrap_or(0.0);
        let topped: f64 = info.topped_up_balance.parse().unwrap_or(0.0);
        let granted: f64 = info.granted_balance.parse().unwrap_or(0.0);

        // Fractional calculation: if total balance > 0, we treat it as healthy (used: 0.0)
        // or show depletion if total <= 0
        let used = if total <= 0.0 { 1.0 } else { 0.0 };

        windows.push(LimitWindow {
            id: format!("deepseek-{}", info.currency.to_lowercase()),
            label: format!("Saldo {}", info.currency),
            used,
            resets_at: None,
            count: Some((total * 100.0).round() as i64),
            derived: false,
            group: Some("DeepSeek".into()),
        });

        if topped > 0.0 {
            windows.push(LimitWindow {
                id: format!("deepseek-{}-topped", info.currency.to_lowercase()),
                label: format!("Recarregado ({})", info.currency),
                used: 0.0,
                resets_at: None,
                count: Some((topped * 100.0).round() as i64),
                derived: false,
                group: Some("DeepSeek".into()),
            });
        }

        // Detail window for topped up vs granted
        if granted > 0.0 {
            windows.push(LimitWindow {
                id: format!("deepseek-{}-granted", info.currency.to_lowercase()),
                label: format!("Concedido ({})", info.currency),
                used: 0.0,
                resets_at: None,
                count: Some((granted * 100.0).round() as i64),
                derived: false,
                group: Some("DeepSeek".into()),
            });
        }
    }

    UsageSnapshot {
        status: "ok".into(),
        windows,
        fetched_at: now_ms(),
        note: format!(
            "Total: {} {}",
            parsed.balance_infos[0].currency, parsed.balance_infos[0].total_balance
        ),
        ..Default::default()
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
                let mut slot = st.deepseek.lock().unwrap();
                *slot = snap.clone();
            }
            let _ = app.emit("deepseek", &snap);

            // Sleep interruptible
            for _ in 0..POLL_SECS {
                if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}
