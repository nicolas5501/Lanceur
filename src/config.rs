#![allow(dead_code, unused_imports, unused_variables)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub settings: AppSettings,
    pub containers: Vec<ContainerConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        default_config()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub bar_position: String,          // "Top", "Bottom", "Floating"
    #[serde(default = "default_alignment")]
    pub containers_alignment: String,  // "Left", "Center", "Right"
    pub bar_height: f32,               // Ex: 36.0
    pub item_height: f32,              // Ex: 26.0
    #[serde(default = "default_icon_size")]
    pub icon_size: f32,                // Ex: 18.0
    pub container_font_size: f32,      // Ex: 11.0
    pub item_font_size: f32,           // Ex: 11.0
    #[serde(default = "default_bar_bg")]
    pub bar_bg_color: String,          // Ex: "#0f172af8"
    #[serde(default = "default_bar_text")]
    pub bar_text_color: String,        // Ex: "#f8fafc"
    #[serde(default = "default_dropdown_hover_color")]
    pub dropdown_hover_color: String,  // Ex: "#2563eb"
    #[serde(default = "default_dropdown_hover_style")]
    pub dropdown_hover_style: String,  // "Highlight", "WaveBorder", "Blinking", "Glow", "SlideAccent", "ScalePop", "GlassShimmer", "UnderlineWave"
    pub hotkey_modifiers: Vec<String>, // Ex: ["Control", "Win"]
    pub hotkey_key: String,            // Ex: "Space", "F8"
    pub autostart: bool,
    pub stay_on_top: bool,
    pub bar_x: i32,
    pub bar_y: i32,
    pub bar_width: i32,
    #[serde(default = "default_rows_count")]
    pub rows_count: usize,
}

fn default_rows_count() -> usize { 1 }
fn default_alignment() -> String { "Left".to_string() }
fn default_icon_size() -> f32 { 18.0 }
fn default_bar_bg() -> String { "#0f172af8".to_string() }
fn default_bar_text() -> String { "#f8fafc".to_string() }
fn default_dropdown_hover_color() -> String { "#2563eb".to_string() }
fn default_dropdown_hover_style() -> String { "Highlight".to_string() }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            bar_position: "Top".to_string(),
            containers_alignment: "Left".to_string(),
            bar_height: 36.0,
            item_height: 26.0,
            icon_size: default_icon_size(),
            container_font_size: 11.0,
            item_font_size: 11.0,
            bar_bg_color: default_bar_bg(),
            bar_text_color: default_bar_text(),
            dropdown_hover_color: default_dropdown_hover_color(),
            dropdown_hover_style: default_dropdown_hover_style(),
            hotkey_modifiers: vec!["Control".to_string()],
            hotkey_key: "Space".to_string(),
            autostart: false,
            stay_on_top: false,
            bar_x: 0,
            bar_y: 0,
            bar_width: 0,
            rows_count: 1,
        }
    }
}

use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn generate_id() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let count = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{}", d.as_secs(), d.subsec_nanos(), count)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ContainerConfig {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub width: f32, // 0.0 = automatique
    pub order: usize,
    #[serde(default = "default_display_mode")]
    pub display_mode: String, // "Both", "IconOnly", "NameOnly"
    #[serde(default = "default_icon_type")]
    pub icon_type: String,    // "emoji" ou "extracted"
    #[serde(default)]
    pub icon_path: String,    // Ex: "C:\\Windows\\explorer.exe" ou vide
    #[serde(default)]
    pub bg_color: String,     // Ex: "#1e293b" ou vide
    #[serde(default)]
    pub text_color: String,   // Ex: "#f8fafc" ou vide
    #[serde(default)]
    pub hotkey_modifiers: Vec<String>,
    #[serde(default)]
    pub hotkey_key: String,
    #[serde(default)]
    pub row: usize, // 0 = Ligne 1, 1 = Ligne 2, etc.
    #[serde(default = "default_columns_count")]
    pub columns_count: usize, // 1 à 10 colonnes (1 par défaut)
    #[serde(default)]
    pub items: Vec<LauncherItem>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            id: generate_id(),
            name: "Nouveau".to_string(),
            icon: "📁".to_string(),
            width: 0.0,
            order: 0,
            display_mode: default_display_mode(),
            icon_type: default_icon_type(),
            icon_path: String::new(),
            bg_color: String::new(),
            text_color: String::new(),
            hotkey_modifiers: Vec::new(),
            hotkey_key: String::new(),
            row: 0,
            columns_count: 1,
            items: Vec::new(),
        }
    }
}

fn default_icon_type() -> String {
    "emoji".to_string()
}

fn default_display_mode() -> String {
    "Both".to_string()
}

fn default_columns_count() -> usize {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherItem {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub target: String,
    #[serde(default = "default_icon_type")]
    pub icon_type: String,  // "emoji" ou "extracted"
    #[serde(default = "default_item_icon_value")]
    pub icon_value: String, // emoji char ou chemin d'icône png en cache
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub bg_color: String,   // Couleur personnalisée au survol
    #[serde(default)]
    pub text_color: String, // Couleur du texte
    #[serde(default)]
    pub hotkey_modifiers: Vec<String>,
    #[serde(default)]
    pub hotkey_key: String,
    #[serde(default)]
    pub column: usize,      // 0 = Colonne 1, 1 = Colonne 2, etc.
}

impl Default for LauncherItem {
    fn default() -> Self {
        Self {
            id: generate_id(),
            name: String::new(),
            target: String::new(),
            icon_type: default_icon_type(),
            icon_value: default_item_icon_value(),
            args: String::new(),
            bg_color: String::new(),
            text_color: String::new(),
            hotkey_modifiers: Vec::new(),
            hotkey_key: String::new(),
            column: 0,
        }
    }
}

fn default_item_icon_value() -> String {
    "🚀".to_string()
}

pub fn config_path() -> PathBuf {
    if PathBuf::from("launcher_config.json").exists() {
        return PathBuf::from("launcher_config.json");
    }
    if let Ok(mut exe) = std::env::current_exe() {
        exe.pop();
        let p = exe.join("launcher_config.json");
        return p;
    }
    PathBuf::from("launcher_config.json")
}

pub fn cache_dir() -> PathBuf {
    let dir = if PathBuf::from("launcher_config.json").exists() || PathBuf::from("icon_cache").exists() {
        PathBuf::from("icon_cache")
    } else if let Ok(mut exe) = std::env::current_exe() {
        exe.pop();
        exe.join("icon_cache")
    } else {
        PathBuf::from("icon_cache")
    };
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
}

pub fn default_config() -> AppConfig {
    AppConfig {
        settings: AppSettings::default(),
        containers: vec![
            ContainerConfig {
                id: generate_id(),
                name: "Web & Outils".to_string(),
                icon: "🌐".to_string(),
                width: 0.0,
                order: 0,
                display_mode: "Both".to_string(),
                icon_type: "emoji".to_string(),
                icon_path: String::new(),
                bg_color: "".to_string(),
                text_color: "".to_string(),
                hotkey_modifiers: Vec::new(),
                hotkey_key: String::new(),
                row: 0,
                columns_count: 1,
                items: vec![
                    LauncherItem {
                        id: generate_id(),
                        name: "Google Chrome".to_string(),
                        target: "https://www.google.com".to_string(),
                        icon_type: "emoji".to_string(),
                        icon_value: "🌐".to_string(),
                        args: String::new(),
                        bg_color: "".to_string(),
                        text_color: "".to_string(),
                        hotkey_modifiers: Vec::new(),
                        hotkey_key: String::new(),
                        column: 0,
                    },
                    LauncherItem {
                        id: generate_id(),
                        name: "Explorateur Windows".to_string(),
                        target: "explorer.exe".to_string(),
                        icon_type: "emoji".to_string(),
                        icon_value: "📁".to_string(),
                        args: String::new(),
                        bg_color: "".to_string(),
                        text_color: "".to_string(),
                        hotkey_modifiers: Vec::new(),
                        hotkey_key: String::new(),
                        column: 0,
                    },
                ],
            },
            ContainerConfig {
                id: generate_id(),
                name: "Système".to_string(),
                icon: "⚙️".to_string(),
                width: 0.0,
                order: 1,
                display_mode: "Both".to_string(),
                icon_type: "emoji".to_string(),
                icon_path: String::new(),
                bg_color: "".to_string(),
                text_color: "".to_string(),
                hotkey_modifiers: Vec::new(),
                hotkey_key: String::new(),
                row: 0,
                columns_count: 1,
                items: vec![
                    LauncherItem {
                        id: generate_id(),
                        name: "Calculatrice".to_string(),
                        target: "calc.exe".to_string(),
                        icon_type: "emoji".to_string(),
                        icon_value: "🧮".to_string(),
                        args: String::new(),
                        bg_color: "".to_string(),
                        text_color: "".to_string(),
                        hotkey_modifiers: Vec::new(),
                        hotkey_key: String::new(),
                        column: 0,
                    },
                    LauncherItem {
                        id: generate_id(),
                        name: "PowerShell".to_string(),
                        target: "powershell.exe".to_string(),
                        icon_type: "emoji".to_string(),
                        icon_value: "⚡".to_string(),
                        args: String::new(),
                        bg_color: "".to_string(),
                        text_color: "".to_string(),
                        hotkey_modifiers: Vec::new(),
                        hotkey_key: String::new(),
                        column: 0,
                    },
                ],
            },
        ],
    }
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if path.exists()
        && let Ok(content) = fs::read_to_string(&path) {
            let clean = content.trim_start_matches('\u{feff}');
            match serde_json::from_str::<AppConfig>(clean) {
                Ok(cfg) => return cfg,
                Err(e) => {
                    eprintln!("[CONFIG ERROR] Impossible de parser {:?}: {}", path, e);
                    let backup_path = path.with_extension("corrupted.json");
                    let _ = fs::copy(&path, &backup_path);
                    eprintln!("[CONFIG ERROR] Sauvegarde de secours créée dans {:?}", backup_path);
                }
            }
        }

    let def = default_config();
    save_config(&def);
    def
}

pub fn save_config(config: &AppConfig) {
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let path = config_path();
        let tmp_path = path.with_extension("tmp");
        if fs::write(&tmp_path, &json).is_ok() {
            if fs::rename(&tmp_path, &path).is_err() {
                let _ = fs::write(&path, &json);
                let _ = fs::remove_file(&tmp_path);
            }
        } else {
            let _ = fs::write(&path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_generate_id_uniqueness() {
        let count = 10_000;
        let mut ids = HashSet::with_capacity(count);
        for _ in 0..count {
            let id = generate_id();
            assert!(ids.insert(id), "Duplicate ID generated!");
        }
    }

    #[test]
    fn test_default_config_serde_roundtrip() {
        let cfg = default_config();
        let json = serde_json::to_string_pretty(&cfg).expect("Serialization failed");
        let parsed: AppConfig = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(parsed.settings.bar_position, cfg.settings.bar_position);
        assert_eq!(parsed.containers.len(), cfg.containers.len());
    }

    #[test]
    fn test_legacy_or_sparse_json_deserialization() {
        let sparse_json = r#"{
            "settings": {
                "bar_position": "Bottom"
            },
            "containers": [
                {
                    "id": "c1",
                    "name": "Minimal"
                }
            ]
        }"#;
        let parsed: AppConfig = serde_json::from_str(sparse_json).expect("Sparse JSON should parse with defaults");
        assert_eq!(parsed.settings.bar_position, "Bottom");
        assert_eq!(parsed.settings.rows_count, 1);
        assert_eq!(parsed.settings.containers_alignment, "Left");
        assert_eq!(parsed.containers.len(), 1);
        assert_eq!(parsed.containers[0].name, "Minimal");
        assert_eq!(parsed.containers[0].columns_count, 1);
        assert!(parsed.containers[0].items.is_empty());
    }
}
