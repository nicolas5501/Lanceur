#![allow(dead_code, unused_imports, unused_variables)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub settings: AppSettings,
    pub containers: Vec<ContainerConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub items: Vec<LauncherItem>,
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
pub struct LauncherItem {
    pub id: String,
    pub name: String,
    pub target: String,
    pub icon_type: String,  // "emoji" ou "extracted"
    pub icon_value: String, // emoji char ou chemin d'icône png en cache
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

pub fn generate_id() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}-{}", d.as_secs(), d.subsec_nanos())
}

pub fn config_path() -> PathBuf {
    PathBuf::from("launcher_config.json")
}

pub fn cache_dir() -> PathBuf {
    let dir = PathBuf::from("icon_cache");
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
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<AppConfig>(&content) {
                return cfg;
            }
        }
    }

    let def = default_config();
    save_config(&def);
    def
}

pub fn save_config(config: &AppConfig) {
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(config_path(), json);
    }
}
