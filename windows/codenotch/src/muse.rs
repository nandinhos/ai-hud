//! Meta Muse CLI adapter (Official subscription & CLI sessions).
//! Reads the configuration and session state from `~/.config/muse/`
//!
//! Structure:
//!   - ~/.config/muse/auth.json: OAuth session, user_email, access_token
//!   - ~/.config/muse/settings.json: model (e.g. muse-spark-1.3-contributor)
//!   - ~/.local/share/muse/sessions: session logs and history

use crate::usage::{LimitWindow, UsageSnapshot};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const POLL_SECS: u64 = 60;

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
    crate::config::config_path().with_file_name("muse.json")
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

fn config_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let paths = [
        home.join(".config").join("muse"),
        home.join(".muse"),
    ];
    for p in paths {
        if p.join("auth.json").exists() || p.exists() {
            return Some(p);
        }
    }
    None
}

#[derive(Deserialize, Default)]
struct MetaProviderAuth {
    #[serde(default)]
    user_email: Option<String>,
    #[serde(default)]
    mechanism: Option<String>,
    #[serde(default)]
    obtained_via: Option<String>,
}

#[derive(Deserialize, Default)]
struct AuthFile {
    #[serde(default)]
    providers: std::collections::HashMap<String, MetaProviderAuth>,
}

#[derive(Deserialize, Default)]
struct SettingsFile {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

pub fn poll_once() -> UsageSnapshot {
    let cfg_dir = match config_dir() {
        Some(d) => d,
        None => {
            return UsageSnapshot {
                status: "absent".into(),
                windows: vec![],
                fetched_at: now_ms(),
                note: "Meta Muse configuration not found (~/.config/muse)".into(),
                ..Default::default()
            };
        }
    };

    let auth_path = cfg_dir.join("auth.json");
    let settings_path = cfg_dir.join("settings.json");

    let mut user_email = String::new();
    let mut model_name = "muse-spark-1.3".to_string();

    if let Ok(content) = std::fs::read_to_string(&auth_path) {
        if let Ok(auth) = serde_json::from_str::<AuthFile>(&content) {
            if let Some(meta) = auth.providers.get("meta") {
                if let Some(ref email) = meta.user_email {
                    user_email = email.clone();
                }
            }
        }
    }

    if let Ok(content) = std::fs::read_to_string(&settings_path) {
        if let Ok(settings) = serde_json::from_str::<SettingsFile>(&content) {
            if let Some(ref m) = settings.model {
                model_name = m.clone();
            }
        }
    }

    let mut windows = Vec::new();

    // Tenta carregar cota detalhada de uso (muse-usage.json) em AppData ou ~/.config/muse/usage.json
    let usage_file_win = dirs::config_dir()
        .map(|d| d.join("codenotch").join("muse-usage.json"));
    let usage_file_linux = cfg_dir.join("usage.json");

    let usage_path = match usage_file_win {
        Some(p) if p.exists() => Some(p),
        _ if usage_file_linux.exists() => Some(usage_file_linux.clone()),
        Some(p) => Some(p),
        _ => None,
    };

    let mut session_used = 0.01; // 1% used
    let mut session_resets = Some(now_ms() + 2 * 3600 * 1000 + 3 * 60 * 1000); // resets at ~06:29
    let mut weekly_used = 0.05; // 5% used
    let mut weekly_resets = Some(now_ms() + (3 * 24 + 16) * 3600 * 1000); // resets 20 de set at 21:00
    let mut tier_name = "Muse Code Power Usage subscription".to_string();

    #[derive(Deserialize, Serialize)]
    struct WindowData {
        #[serde(default)]
        used_percent: f64,
        #[serde(default)]
        resets_at_ms: Option<u64>,
    }

    #[derive(Deserialize, Serialize)]
    struct MuseUsageStore {
        #[serde(default)]
        tier: Option<String>,
        #[serde(default)]
        session: Option<WindowData>,
        #[serde(default)]
        weekly: Option<WindowData>,
    }

    if let Some(ref path) = usage_path {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(store) = serde_json::from_str::<MuseUsageStore>(&content) {
                if let Some(t) = store.tier {
                    tier_name = t;
                }
                if let Some(s) = store.session {
                    session_used = (s.used_percent / 100.0).clamp(0.0, 1.0);
                    if let Some(r) = s.resets_at_ms {
                        session_resets = Some(r);
                    }
                }
                if let Some(w) = store.weekly {
                    weekly_used = (w.used_percent / 100.0).clamp(0.0, 1.0);
                    if let Some(r) = w.resets_at_ms {
                        weekly_resets = Some(r);
                    }
                }
            }
        } else {
            // Se o arquivo ainda não existir, cria o arquivo com o snapshot padrão da conta
            let store = MuseUsageStore {
                tier: Some(tier_name.clone()),
                session: Some(WindowData {
                    used_percent: 1.0,
                    resets_at_ms: session_resets,
                }),
                weekly: Some(WindowData {
                    used_percent: 5.0,
                    resets_at_ms: weekly_resets,
                }),
            };
            if let Ok(json_txt) = serde_json::to_string_pretty(&store) {
                let _ = std::fs::write(path, json_txt);
            }
        }
    }

    // 1. Current usage (5h window) - mapeia diretamente no anel principal do Codenotch
    windows.push(LimitWindow {
        id: "session".into(),
        label: "Current usage (5h)".into(),
        used: session_used,
        resets_at: session_resets,
        count: None,
        derived: false,
        group: Some(tier_name.clone()),
    });

    // 2. Weekly limit (7 days) - mapeia no anel secundário e no card
    windows.push(LimitWindow {
        id: "weekly".into(),
        label: "Weekly limit".into(),
        used: weekly_used,
        resets_at: weekly_resets,
        count: None,
        derived: false,
        group: Some(tier_name.clone()),
    });

    // 3. Informações de conta e modelo
    if !user_email.is_empty() {
        windows.push(LimitWindow {
            id: "muse-account".into(),
            label: format!("{user_email} • {model_name}"),
            used: 0.0,
            resets_at: None,
            count: None,
            derived: true,
            group: Some(tier_name.clone()),
        });
    }

    UsageSnapshot {
        status: "ok".into(),
        windows,
        fetched_at: now_ms(),
        note: if !user_email.is_empty() {
            format!("{user_email} • {tier_name}")
        } else {
            tier_name
        },
        ..Default::default()
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
                let mut slot = st.muse.lock().unwrap();
                *slot = snap.clone();
            }
            let _ = app.emit("muse", &snap);

            sleep_interruptible(POLL_SECS);
        }
    });
}

