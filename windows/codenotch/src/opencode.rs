//! OpenCode Go usage adapter (official API).
//! Endpoint: GET https://opencode.ai/zen/go/v1/usage
//! Headers: Authorization: Bearer <token>
//!
//! Real API response:
//! {
//!   "usage": {
//!     "rolling": { "status": "ok", "percent": 0, "resetsAt": "2026-09-17T12:51:14.292Z" },
//!     "weekly":  { "status": "ok", "percent": 4, "resetsAt": "2026-09-21T00:00:00.292Z" },
//!     "monthly": { "status": "ok", "percent": 2, "resetsAt": "2026-10-12T14:06:44.292Z" }
//!   }
//! }
//!
//! Extra API Balance in USD (Zen Balance) is read from API or %APPDATA%\codenotch\opencode-balance.txt

use crate::usage::{LimitWindow, UsageSnapshot};
use crate::AppState;
use serde::Deserialize;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const ENDPOINT: &str = "https://opencode.ai/zen/go/v1/usage";
const POLL_SECS: u64 = 180;

static REFRESH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn request_refresh() {
    REFRESH.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn sleep_interruptible(total_secs: u64) {
    for _ in 0..total_secs {
        if REFRESH.swap(false, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_iso_ms(s: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis() as u64)
}

fn store_path() -> PathBuf {
    crate::config::config_path().with_file_name("opencode.json")
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

#[derive(Deserialize, Debug, Default, Clone)]
pub struct UsageWindowPayload {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, alias = "usagePercent", alias = "usage_percent", alias = "used_percent")]
    pub percent: Option<f64>,
    #[serde(default, alias = "resetsAt", alias = "resets_at")]
    pub resets_at: Option<String>,
    #[serde(default, alias = "resetInSec", alias = "reset_in_sec")]
    pub reset_in_sec: Option<u64>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct UsageGroup {
    pub rolling: Option<UsageWindowPayload>,
    pub weekly: Option<UsageWindowPayload>,
    pub monthly: Option<UsageWindowPayload>,
}

#[derive(Deserialize, Debug, Default)]
pub struct OpenCodeUsageResponse {
    pub usage: Option<UsageGroup>,
    pub rolling: Option<UsageWindowPayload>,
    pub weekly: Option<UsageWindowPayload>,
    pub monthly: Option<UsageWindowPayload>,
    #[serde(alias = "extraDollars", alias = "extra_dollars", alias = "balanceDollars", alias = "balance_dollars", alias = "zenBalance", alias = "zen_balance", alias = "balance")]
    pub extra_balance: Option<f64>,
    pub note: Option<String>,
}

fn resolve_api_key() -> Option<String> {
    // 1. Environment variables
    for var in &["OPENCODE_API_KEY", "OPENCODE_GO_KEY", "OPENCODE_KEY"] {
        if let Ok(k) = std::env::var(var) {
            let trimmed = k.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }

    // 2. User config (%APPDATA%\codenotch\opencode.key or opencode.token)
    let key_file = crate::config::config_path().with_file_name("opencode.key");
    if let Ok(content) = std::fs::read_to_string(&key_file) {
        let trimmed = content.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    // 3. Local share opencode auth.json or account.json
    if let Some(home) = dirs::home_dir() {
        let paths = [
            home.join(".local").join("share").join("opencode").join("auth.json"),
            home.join(".local").join("share").join("opencode").join("account.json"),
            home.join(".config").join("opencode").join("auth.json"),
            home.join("AppData").join("Roaming").join("OpenCode").join("auth.json"),
            home.join("AppData").join("Local").join("opencode").join("auth.json"),
        ];

        for p in paths {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(go) = v.get("opencode-go") {
                        if let Some(k) = go.get("key").or_else(|| go.get("apiKey")).and_then(|x| x.as_str()) {
                            let trimmed = k.trim().to_string();
                            if !trimmed.is_empty() {
                                return Some(trimmed);
                            }
                        }
                    }
                    if let Some(accounts) = v.get("accounts").and_then(|a| a.as_object()) {
                        for (_, acc) in accounts {
                            if acc.get("serviceID").and_then(|s| s.as_str()) == Some("opencode-go") {
                                if let Some(k) = acc.get("credential").and_then(|c| c.get("key")).and_then(|x| x.as_str()) {
                                    let trimmed = k.trim().to_string();
                                    if !trimmed.is_empty() {
                                        return Some(trimmed);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn resolve_extra_balance() -> Option<f64> {
    let file = crate::config::config_path().with_file_name("opencode-balance.txt");
    if let Ok(content) = std::fs::read_to_string(&file) {
        let clean = content.trim().replace('$', "");
        if let Ok(val) = clean.parse::<f64>() {
            return Some(val);
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
                note: "Configure OPENCODE_API_KEY or %APPDATA%\\codenotch\\opencode.key".into(),
                ..Default::default()
            };
        }
    };

    let resp = match ureq::get(ENDPOINT)
        .set("Authorization", &format!("Bearer {key}"))
        .set("User-Agent", "codenotch/1.0")
        .timeout(Duration::from_secs(15))
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::Status(401, _)) => {
            return UsageSnapshot {
                status: "needsAuth".into(),
                windows: vec![],
                fetched_at: now_ms(),
                note: "OpenCode API key invalid (401)".into(),
                ..Default::default()
            };
        }
        Err(ureq::Error::Status(403, r)) => {
            let text = r.into_string().unwrap_or_default();
            let note = if text.contains("subscription required") || text.contains("EntitlementError") {
                "OpenCode Go subscription required. Ative em opencode.ai/auth".to_string()
            } else {
                "OpenCode API access denied (403)".to_string()
            };
            return UsageSnapshot {
                status: "needsAuth".into(),
                windows: vec![],
                fetched_at: now_ms(),
                note,
                ..Default::default()
            };
        }
        Err(ureq::Error::Status(429, _)) => {
            let prev = load_persisted();
            let windows = prev.windows;
            return UsageSnapshot {
                status: if !windows.is_empty() { "stale".into() } else { "error".into() },
                windows,
                fetched_at: now_ms(),
                note: "OpenCode API rate limited (429)".into(),
                ..Default::default()
            };
        }
        Err(e) => {
            let prev = load_persisted();
            let windows = prev.windows;
            return UsageSnapshot {
                status: if !windows.is_empty() { "stale".into() } else { "error".into() },
                windows,
                fetched_at: now_ms(),
                note: format!("Failed to reach OpenCode Go API: {e}"),
                ..Default::default()
            };
        }
    };

    let parsed: OpenCodeUsageResponse = match resp.into_json() {
        Ok(b) => b,
        Err(e) => {
            return UsageSnapshot {
                status: "error".into(),
                windows: vec![],
                fetched_at: now_ms(),
                note: format!("Malformed OpenCode usage payload: {e}"),
                ..Default::default()
            };
        }
    };

    let mut windows = Vec::new();
    let current_ms = now_ms();

    let usage_group = parsed.usage.clone().unwrap_or_default();
    let rolling_w = usage_group.rolling.or(parsed.rolling);
    let weekly_w = usage_group.weekly.or(parsed.weekly);
    let monthly_w = usage_group.monthly.or(parsed.monthly);

    // Bloco 1: Cotas da Assinatura Go
    // 1. Rolling 5h limit
    if let Some(r) = rolling_w {
        let raw_pct = r.percent.unwrap_or(0.0);
        let used = if raw_pct > 1.0 { raw_pct / 100.0 } else { raw_pct };
        let resets_at = r.resets_at.as_deref().and_then(parse_iso_ms).or_else(|| {
            r.reset_in_sec.map(|s| current_ms + (s * 1000))
        });
        windows.push(LimitWindow {
            id: "opencode-session".into(),
            label: "Sessão 5h".into(),
            used: used.clamp(0.0, 1.0),
            resets_at,
            count: None,
            derived: false,
            group: Some("OpenCode Go — Cotas".into()),
        });
    }

    // 2. Weekly limit
    if let Some(w) = weekly_w {
        let raw_pct = w.percent.unwrap_or(0.0);
        let used = if raw_pct > 1.0 { raw_pct / 100.0 } else { raw_pct };
        let resets_at = w.resets_at.as_deref().and_then(parse_iso_ms).or_else(|| {
            w.reset_in_sec.map(|s| current_ms + (s * 1000))
        });
        windows.push(LimitWindow {
            id: "opencode-weekly".into(),
            label: "Semanal".into(),
            used: used.clamp(0.0, 1.0),
            resets_at,
            count: None,
            derived: false,
            group: Some("OpenCode Go — Cotas".into()),
        });
    }

    // 3. Monthly limit
    if let Some(m) = monthly_w {
        let raw_pct = m.percent.unwrap_or(0.0);
        let used = if raw_pct > 1.0 { raw_pct / 100.0 } else { raw_pct };
        let resets_at = m.resets_at.as_deref().and_then(parse_iso_ms).or_else(|| {
            m.reset_in_sec.map(|s| current_ms + (s * 1000))
        });
        windows.push(LimitWindow {
            id: "opencode-monthly".into(),
            label: "Mensal".into(),
            used: used.clamp(0.0, 1.0),
            resets_at,
            count: None,
            derived: false,
            group: Some("OpenCode Go — Cotas".into()),
        });
    }

    // Bloco 2: Saldo Extra de API (USD / Zen Balance)
    let extra_val = parsed.extra_balance.or_else(resolve_extra_balance);
    if let Some(extra) = extra_val {
        let cents = (extra * 100.0).round() as i64;
        windows.push(LimitWindow {
            id: "opencode-extra-usd".into(),
            label: "Saldo Extra API (USD)".into(),
            used: if extra <= 0.0 { 1.0 } else { 0.0 },
            resets_at: None,
            count: Some(cents),
            derived: false,
            group: Some("OpenCode — Saldo Extra".into()),
        });
    }

    let snap = UsageSnapshot {
        status: "ok".into(),
        windows,
        fetched_at: current_ms,
        note: parsed.note.unwrap_or_else(|| "OpenCode Go ativo".into()),
        ..Default::default()
    };

    if snap.status != "absent" && (!snap.windows.is_empty() || snap.status == "needsAuth") {
        persist(&snap);
    }
    snap
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            let snap = poll_once();
            {
                let st = app.state::<AppState>();
                *st.opencode.lock().unwrap() = snap.clone();
            }

            let _ = app.emit("opencode_usage", &snap);
            let _ = app.emit("usage", ());

            sleep_interruptible(POLL_SECS);
        }
    });
}
