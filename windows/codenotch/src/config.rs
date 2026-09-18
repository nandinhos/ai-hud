use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The Mac's notch sizes, as multiples of the designed size: Small, Medium, Large.
pub const SIZES: [f64; 3] = [0.8, 1.0, 1.25];

/// The nearest of `SIZES`, so a scale saved by the old 40–100 % slider still lands on a size that
/// exists. 0.9, halfway between Small and Medium, counts as Medium.
pub fn snap_scale(scale: f64) -> f64 {
    if scale < 0.9 {
        SIZES[0]
    } else if scale < 1.125 || !scale.is_finite() {
        SIZES[1]
    } else {
        SIZES[2]
    }
}

/// One half of the tray icon, or one ring on the notch: which provider. It shows that provider's
/// ring, so the tray and the notch can never disagree. (A `window` key from older builds is ignored.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraySlot {
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_port")]
    pub port: u16,
    /// "auto" | "zh" | "en" | "ja" | "ko" | "ru" | "uk"
    #[serde(default = "default_lang")]
    pub lang: String,
    #[serde(default)]
    pub bar_x: Option<i32>,
    #[serde(default)]
    pub bar_y: Option<i32>,
    /// Logical width of the bar (wheel-adjustable, 220-520); None = default 360
    #[serde(default)]
    pub bar_w: Option<u32>,
    /// Allow dragging + wheel resizing (tray toggle, off by default to prevent accidental drags)
    #[serde(default)]
    pub drag_enabled: bool,
    /// Vertical position of the notch: the window centre as a fraction of the primary monitor's height (0 = top, 1 = bottom), default 0.5; saved after a drag
    #[serde(default = "default_notch_y")]
    pub notch_y: f64,
    /// Notch size as a multiple of the designed size, one of `SIZES`. The whole notch scales: the
    /// window grows and its WebView zooms, so the rings, text and hover card keep their proportions.
    #[serde(default = "default_scale")]
    pub scale: f64,
    /// Where the weekly limit gets a ring of its own: "off", "inside" or "outside".
    #[serde(default = "default_weekly_ring")]
    pub weekly_ring: String,
    /// What the tray icon draws: "off" (the plain mark, the previous behaviour and the default),
    /// "numbers" (up to two readings as digits) or "bars" (a column per reading).
    #[serde(default = "default_tray_mode")]
    pub tray_mode: String,
    /// Which providers the tray icon covers, in the order they are drawn. Ids match the page:
    /// "claude", "codex", "cursor", "gemini". Superseded by `tray_slots`; kept so an existing
    /// config still upgrades cleanly, and migrated in `load()`.
    #[serde(default = "default_tray_providers")]
    pub tray_providers: Vec<String>,
    /// What each part of the tray icon shows, in drawing order: the first entry is the top half of
    /// the digit layout, the second the bottom half, and the bar layout uses them all in order.
    #[serde(default)]
    pub tray_slots: Vec<TraySlot>,
    /// Which providers the notch itself shows, in order. Empty means every provider that has
    /// something to report — the original behaviour, and the default. Superseded by `notch_slots`,
    /// kept so an existing config migrates cleanly.
    #[serde(default)]
    pub notch_providers: Vec<String>,
    /// Which providers get a ring on the notch, in order. An empty list means every provider.
    #[serde(default)]
    pub notch_slots: Vec<TraySlot>,
    /// Antigravity's lane on the ring, as the Mac app's "Notch reads": "automatic", "5h" or "weekly"
    #[serde(default = "default_antigravity_limit")]
    pub antigravity_limit: String,
    /// The model family that choice looks at, as the Mac app's "Model data": "gemini" or "3p"
    #[serde(default = "default_antigravity_model")]
    pub antigravity_model: String,
    /// false = the pill is kept off the screen edge entirely; the tray icon is then the only way in
    #[serde(default = "yes")]
    pub notch_visible: bool,
    /// false = the tray icon is hidden. Refused while the notch is also hidden, because that would
    /// leave the app running with no way to reach it.
    #[serde(default = "yes")]
    pub tray_visible: bool,
    /// Opacity of the notch pill when idling (0.2 - 1.0, default 1.0)
    #[serde(default = "default_notch_opacity")]
    pub notch_opacity: f64,
}

fn default_notch_opacity() -> f64 {
    1.0
}

fn default_notch_y() -> f64 {
    0.5
}
fn default_scale() -> f64 {
    1.0
}
fn default_weekly_ring() -> String {
    "off".into()
}

/// A second arc changes how every reading looks, so an unreadable value means off rather than a
/// guess at what was meant.
pub fn weekly_ring_or_off(value: &str) -> String {
    match value {
        "inside" | "outside" => value.to_string(),
        _ => default_weekly_ring(),
    }
}
fn yes() -> bool {
    true
}
/// A fresh install shows the two readings straight away — a tray icon nobody knows to look for is
/// a feature nobody finds. An install that predates this setting is handled in `load()` instead:
/// it keeps the plain mark it already has, so upgrading never changes anyone's icon unasked.
fn default_tray_mode() -> String {
    "numbers".into()
}
fn default_tray_providers() -> Vec<String> {
    vec!["claude".into(), "codex".into()]
}
fn default_antigravity_limit() -> String {
    "automatic".into()
}
fn default_antigravity_model() -> String {
    "gemini".into()
}

fn default_port() -> u16 {
    48666
}
fn default_lang() -> String {
    "auto".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            lang: default_lang(),
            bar_x: None,
            bar_y: None,
            bar_w: None,
            drag_enabled: false,
            notch_y: default_notch_y(),
            scale: default_scale(),
            weekly_ring: default_weekly_ring(),
            tray_mode: default_tray_mode(),
            tray_providers: default_tray_providers(),
            tray_slots: Vec::new(), // filled in by load(), from tray_providers
            notch_providers: Vec::new(), // empty = show them all
            notch_slots: Vec::new(),     // filled in by load(), from notch_providers
            antigravity_limit: default_antigravity_limit(),
            antigravity_model: default_antigravity_model(),
            notch_visible: true,
            tray_visible: true,
            notch_opacity: default_notch_opacity(),
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("codenotch")
        .join("config.json")
}

pub fn load() -> Config {
    let path = config_path();
    let raw = std::fs::read_to_string(&path).ok();
    let mut cfg: Config = raw
        .as_deref()
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default();

    // Discoverability without surprising anyone. `default_tray_mode` gives a NEW install the
    // numbers icon, but serde applies that same default to an EXISTING config that simply predates
    // the setting — which would silently change the tray icon of everyone who upgrades. So an
    // existing file with no `tray_mode` key is pinned to the plain mark it already has; only a
    // machine with no config at all gets the new default.
    let upgrading = raw
        .as_deref()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
        .map(|v| v.get("tray_mode").is_none())
        .unwrap_or(false);
    if upgrading {
        cfg.tray_mode = "off".into();
    }

    // Migration: before slots existed the icon was a plain provider list, one reading each. That
    // is exactly a list of slots, so nobody's choice is lost and nobody has to reconfigure anything.
    if cfg.tray_slots.is_empty() {
        cfg.tray_slots = cfg
            .tray_providers
            .iter()
            .map(|p| TraySlot { provider: p.clone() })
            .collect();
    }

    // Same migration for the notch.
    if cfg.notch_slots.is_empty() {
        cfg.notch_slots = cfg
            .notch_providers
            .iter()
            .map(|p| TraySlot { provider: p.clone() })
            .collect();
    }

    // Both hidden would leave the app unreachable: no pill, no tray icon, no way to open settings.
    if !cfg.notch_visible && !cfg.tray_visible {
        cfg.tray_visible = true;
    }

    // The old slider's 40–100 %, or a hand-edited file, lands on one of the three sizes
    cfg.scale = snap_scale(cfg.scale);
    cfg.weekly_ring = weekly_ring_or_off(&cfg.weekly_ring);
    cfg
}

pub fn save(cfg: &Config) {
    let path = config_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(txt) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, txt);
    }
}

#[cfg(test)]
mod tests {
    use super::{snap_scale, weekly_ring_or_off};

    #[test]
    fn a_saved_scale_snaps_to_the_nearest_size() {
        assert_eq!(snap_scale(0.4), 0.8);
        assert_eq!(snap_scale(0.85), 0.8);
        assert_eq!(snap_scale(0.9), 1.0);
        assert_eq!(snap_scale(1.0), 1.0);
        assert_eq!(snap_scale(1.2), 1.25);
        assert_eq!(snap_scale(3.0), 1.25);
    }

    #[test]
    fn only_the_two_placements_are_kept() {
        assert_eq!(weekly_ring_or_off("inside"), "inside");
        assert_eq!(weekly_ring_or_off("outside"), "outside");
        assert_eq!(weekly_ring_or_off("Inside"), "off");
        assert_eq!(weekly_ring_or_off(""), "off");
    }
}
