#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code, unused_imports, unused_variables)]

mod config;
mod win32_utils;

use config::*;
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

slint::include_modules!();

// Memory trimming disabled as SetProcessWorkingSetSize invalidates GDI buffers in Slint
fn trim_process_memory() {}

fn parse_hex_color(hex_str: &str, default: Color) -> Color {
    let s = hex_str.trim().trim_start_matches('#');
    if s.len() == 6 {
        if let Ok(val) = u32::from_str_radix(s, 16) {
            let r = ((val >> 16) & 0xFF) as u8;
            let g = ((val >> 8) & 0xFF) as u8;
            let b = (val & 0xFF) as u8;
            return Color::from_argb_u8(255, r, g, b);
        }
    } else if s.len() == 8 {
        if let Ok(val) = u32::from_str_radix(s, 16) {
            let r = ((val >> 24) & 0xFF) as u8;
            let g = ((val >> 16) & 0xFF) as u8;
            let b = ((val >> 8) & 0xFF) as u8;
            let a = (val & 0xFF) as u8;
            return Color::from_argb_u8(a, r, g, b);
        }
    }
    default
}

fn format_hotkey_display(mods: &[String], key: &str) -> String {
    if key.trim().is_empty() {
        return String::new();
    }
    let mut parts: Vec<&str> = Vec::new();
    for m in mods {
        match m.to_lowercase().as_str() {
            "control" | "ctrl" => parts.push("Ctrl"),
            "alt" => parts.push("Alt"),
            "shift" => parts.push("Shift"),
            "win" | "windows" => parts.push("Win"),
            _ => {}
        }
    }
    parts.push(key);
    parts.join("+")
}

fn get_total_bar_height(cfg: &AppConfig) -> i32 {
    let max_row = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0);
    let rows_count = (max_row + 1).max(1);
    (cfg.settings.bar_height as i32) * (rows_count as i32)
}

fn get_max_allowed_rows(cfg: &AppConfig) -> usize {
    #[cfg(windows)]
    let (_, _, _, work_h) = win32_utils::win32::get_work_area();
    #[cfg(not(windows))]
    let work_h = 1080;

    let bar_h = (cfg.settings.bar_height as i32).max(16);
    let max_by_screen = ((work_h / bar_h) as usize).max(1);
    let max_in_cfg = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0) + 1;
    max_by_screen.max(max_in_cfg).max(1)
}

fn build_available_rows_list(cfg: &AppConfig) -> Vec<SharedString> {
    let max_rows = get_max_allowed_rows(cfg);
    let mut rows = Vec::with_capacity(max_rows);
    for r in 0..max_rows {
        if r == 0 {
            rows.push("Ligne 1 (Haut)".into());
        } else {
            rows.push(format!("Ligne {}", r + 1).into());
        }
    }
    rows
}

fn update_bar_window_geometry(cfg: &AppConfig) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::HWND;
        let bar_hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst) as HWND;
        if !bar_hwnd.is_null() {
            let total_h = get_total_bar_height(cfg);
            win32_utils::win32::set_desktop_parent(bar_hwnd, cfg.settings.stay_on_top);
            win32_utils::win32::setup_bar_window_styles(
                bar_hwnd,
                cfg.settings.stay_on_top,
                cfg.settings.bar_position == "Floating",
            );
            win32_utils::win32::position_bar_window(
                bar_hwnd,
                &cfg.settings.bar_position,
                total_h,
                cfg.settings.bar_x,
                cfg.settings.bar_y,
                cfg.settings.bar_width,
                false,
                cfg.settings.stay_on_top,
            );
            win32_utils::win32::bring_to_foreground(bar_hwnd, cfg.settings.stay_on_top);
        }
    }
}

// Helper to convert AppConfig to BarWindow UI models
fn refresh_bar_ui(bar: &BarWindow, cfg: &AppConfig) {
    bar.set_bar_position(cfg.settings.bar_position.clone().into());
    bar.set_containers_align(cfg.settings.containers_alignment.clone().into());
    bar.set_bar_h(cfg.settings.bar_height);
    bar.set_item_h(cfg.settings.item_height);
    bar.set_container_font_sz(cfg.settings.container_font_size);
    bar.set_item_font_sz(cfg.settings.item_font_size);

    let bar_bg = parse_hex_color(&cfg.settings.bar_bg_color, Color::from_argb_u8(255, 15, 23, 42));
    let bar_text = parse_hex_color(&cfg.settings.bar_text_color, Color::from_argb_u8(255, 248, 250, 252));
    bar.set_bar_bg_color(bar_bg);
    bar.set_bar_text_color(bar_text);

    let cache = cache_dir();
    let max_row = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0);
    let rows_count = (max_row + 1).max(1);
    bar.set_rows_count(rows_count as i32);

    let mut rows_data: Vec<BarRowData> = Vec::new();

    for r in 0..rows_count {
        let mut row_containers: Vec<BarContainerData> = Vec::new();

        for (flat_idx, cont) in cfg.containers.iter().enumerate() {
            if cont.row == r {
                let mut items_data: Vec<BarItemData> = Vec::new();
                for itm in &cont.items {
                    let mut icon_img = slint::Image::default();
                    let mut icon_type = itm.icon_type.clone();

                    if icon_type == "extracted" {
                        let icon_path = win32_utils::win32::extract_and_cache_icon(&itm.target, &cache);
                        if let Some(p) = icon_path {
                            if let Ok(img) = slint::Image::load_from_path(&p) {
                                icon_img = img;
                            } else {
                                icon_type = "emoji".to_string();
                            }
                        } else {
                            icon_type = "emoji".to_string();
                        }
                    }

                    let emoji_val = if itm.icon_value.is_empty() {
                        "🚀".to_string()
                    } else {
                        itm.icon_value.clone()
                    };

                    let item_hk_str = format_hotkey_display(&itm.hotkey_modifiers, &itm.hotkey_key);
                    let itm_bg = parse_hex_color(&itm.bg_color, Color::from_argb_u8(0, 0, 0, 0));
                    let itm_text = parse_hex_color(&itm.text_color, Color::from_argb_u8(0, 0, 0, 0));

                    items_data.push(BarItemData {
                        id: itm.id.clone().into(),
                        name: itm.name.clone().into(),
                        target: itm.target.clone().into(),
                        icon_type: icon_type.into(),
                        icon_emoji: emoji_val.into(),
                        icon_image: icon_img,
                        container_id: cont.id.clone().into(),
                        hotkey_display: item_hk_str.into(),
                        bg_color: itm_bg,
                        text_color: itm_text,
                    });
                }

                let cont_hk_str = format_hotkey_display(&cont.hotkey_modifiers, &cont.hotkey_key);
                let cont_bg = parse_hex_color(&cont.bg_color, Color::from_argb_u8(0, 0, 0, 0));
                let cont_text = parse_hex_color(&cont.text_color, Color::from_argb_u8(0, 0, 0, 0));

                row_containers.push(BarContainerData {
                    flat_idx: flat_idx as i32,
                    id: cont.id.clone().into(),
                    name: cont.name.clone().into(),
                    icon: cont.icon.clone().into(),
                    width_val: cont.width,
                    display_mode: cont.display_mode.clone().into(),
                    bg_color: cont_bg,
                    text_color: cont_text,
                    items: ModelRc::new(VecModel::from(items_data)),
                    hotkey_display: cont_hk_str.into(),
                });
            }
        }

        rows_data.push(BarRowData {
            row_idx: r as i32,
            containers: ModelRc::new(VecModel::from(row_containers)),
        });
    }

    bar.set_rows_list(ModelRc::new(VecModel::from(rows_data)));
}



#[cfg(windows)]
fn hide_bar_window() {
    win32_utils::win32::BAR_EXPLICITLY_HIDDEN.store(true, Ordering::SeqCst);
    win32_utils::win32::BAR_WINDOW_VISIBLE.store(false, Ordering::SeqCst);
    let hwnd = win32_utils::win32::find_bar_hwnd();
    if !hwnd.is_null() {
        win32_utils::win32::unregister_appbar(hwnd);
        unsafe {
            // Déplace hors-écran sans détruire la surface graphique de Slint/Winit
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::HWND_BOTTOM,
                -30000,
                -30000,
                0,
                0,
                windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(not(windows))]
fn hide_bar_window() {}

#[cfg(windows)]
fn show_bar_window(bar: &BarWindow, is_expanded: bool) {
    win32_utils::win32::BAR_EXPLICITLY_HIDDEN.store(false, Ordering::SeqCst);
    win32_utils::win32::BAR_WINDOW_VISIBLE.store(true, Ordering::SeqCst);
    let hwnd = win32_utils::win32::find_bar_hwnd();
    if !hwnd.is_null() {
        let previous_foreground = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow()
        };
        let cfg = load_config();
        let total_h = get_total_bar_height(&cfg);
        win32_utils::win32::set_desktop_parent(hwnd, cfg.settings.stay_on_top);

        // Applique les styles et synchronise le mode bureau persistant
        win32_utils::win32::setup_bar_window_styles(
            hwnd,
            cfg.settings.stay_on_top,
            cfg.settings.bar_position == "Floating",
        );

        // Repositionne à l'écran (Top/Bottom/Floating) sans voler le focus
        win32_utils::win32::position_bar_window(
            hwnd,
            &cfg.settings.bar_position,
            total_h,
            cfg.settings.bar_x,
            cfg.settings.bar_y,
            cfg.settings.bar_width,
            is_expanded,
            cfg.settings.stay_on_top,
        );

        win32_utils::win32::bring_to_foreground(hwnd, cfg.settings.stay_on_top);

        // Force le rafraîchissement immédiat de Slint pour repeindre instantanément
        bar.window().request_redraw();
        let bw = bar.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(16), move || {
            if let Some(b) = bw.upgrade() {
                b.window().request_redraw();
            }
        });
    }
}

#[cfg(not(windows))]
fn show_bar_window(bar: &BarWindow, _is_expanded: bool) {
    bar.window().request_redraw();
}

// Helper to refresh SettingsWindow UI models
fn refresh_settings_ui(settings_win: &SettingsWindow, cfg: &AppConfig, selected_cont_idx: usize) {
    let available_rows = build_available_rows_list(cfg);
    settings_win.set_available_rows(ModelRc::new(VecModel::from(available_rows)));

    let mut cont_summaries: Vec<ContainerItemSummary> = Vec::new();
    let mut cont_names: Vec<SharedString> = Vec::new();

    for cont in &cfg.containers {
        cont_names.push(cont.name.clone().into());
        let hk = format_hotkey_display(&cont.hotkey_modifiers, &cont.hotkey_key);
        cont_summaries.push(ContainerItemSummary {
            id: cont.id.clone().into(),
            name: cont.name.clone().into(),
            icon: cont.icon.clone().into(),
            width_val: cont.width,
            display_mode: cont.display_mode.clone().into(),
            bg_color: cont.bg_color.clone().into(),
            text_color: cont.text_color.clone().into(),
            items_count: cont.items.len() as i32,
            hotkey_display: hk.into(),
            cont_row: cont.row as i32,
        });
    }

    settings_win.set_containers_list(ModelRc::new(VecModel::from(cont_summaries)));
    settings_win.set_container_names(ModelRc::new(VecModel::from(cont_names)));

    let safe_idx = if cfg.containers.is_empty() {
        0
    } else {
        selected_cont_idx.min(cfg.containers.len() - 1)
    };
    settings_win.set_selected_container_index(safe_idx as i32);

    if let Some(cont) = cfg.containers.get(safe_idx) {
        settings_win.set_edit_container_name(cont.name.clone().into());
        settings_win.set_edit_container_icon(cont.icon.clone().into());
        settings_win.set_edit_container_width(cont.width);
        settings_win.set_edit_container_display_mode(cont.display_mode.clone().into());
        settings_win.set_edit_container_bg(cont.bg_color.clone().into());
        settings_win.set_edit_container_text(cont.text_color.clone().into());
        settings_win.set_edit_container_row(cont.row as i32);
        settings_win.set_edit_cont_mod_ctrl(cont.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("control") || m.eq_ignore_ascii_case("ctrl")));
        settings_win.set_edit_cont_mod_alt(cont.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("alt")));
        settings_win.set_edit_cont_mod_shift(cont.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("shift")));
        settings_win.set_edit_cont_mod_win(cont.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("windows")));
        settings_win.set_edit_cont_hotkey_key(cont.hotkey_key.clone().into());

        let mut items_detail: Vec<ItemDetailData> = Vec::new();
        for itm in &cont.items {
            let hk = format_hotkey_display(&itm.hotkey_modifiers, &itm.hotkey_key);
            items_detail.push(ItemDetailData {
                id: itm.id.clone().into(),
                name: itm.name.clone().into(),
                target: itm.target.clone().into(),
                icon_type: itm.icon_type.clone().into(),
                icon_value: if itm.icon_type == "extracted" { "🖼️".into() } else { itm.icon_value.clone().into() },
                container_id: cont.id.clone().into(),
                bg_color: itm.bg_color.clone().into(),
                text_color: itm.text_color.clone().into(),
                hotkey_display: hk.into(),
            });
        }
        settings_win.set_items_list(ModelRc::new(VecModel::from(items_detail)));
    } else {
        settings_win.set_items_list(ModelRc::new(VecModel::from(Vec::<ItemDetailData>::new())));
    }

    // Apparence & Système
    settings_win.set_pref_position(cfg.settings.bar_position.clone().into());
    settings_win.set_pref_containers_align(cfg.settings.containers_alignment.clone().into());
    settings_win.set_pref_bar_h(cfg.settings.bar_height);
    settings_win.set_pref_item_h(cfg.settings.item_height);
    settings_win.set_pref_cont_font(cfg.settings.container_font_size);
    settings_win.set_pref_item_font(cfg.settings.item_font_size);
    settings_win.set_pref_bar_bg_color(cfg.settings.bar_bg_color.clone().into());
    settings_win.set_pref_bar_text_color(cfg.settings.bar_text_color.clone().into());
    settings_win.set_pref_mod_ctrl(cfg.settings.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("control") || m.eq_ignore_ascii_case("ctrl")));
    settings_win.set_pref_mod_alt(cfg.settings.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("alt")));
    settings_win.set_pref_mod_shift(cfg.settings.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("shift")));
    settings_win.set_pref_mod_win(cfg.settings.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("windows")));
    settings_win.set_pref_hotkey_key(cfg.settings.hotkey_key.clone().into());
    settings_win.set_pref_autostart(cfg.settings.autostart);
    settings_win.set_pref_stay_on_top(cfg.settings.stay_on_top);
}

fn add_dropped_file_to_container(file_path: &str, target_cont_idx: usize, config_arc: &Arc<Mutex<AppConfig>>) {
    let p = std::path::Path::new(file_path);
    let mut stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Nouvel Item".to_string());
    if stem.is_empty() {
        stem = file_path.to_string();
    }
    
    let cache = cache_dir();
    let has_icon = win32_utils::win32::extract_and_cache_icon(file_path, &cache).is_some();
    let icon_type = if has_icon { "extracted".to_string() } else { "emoji".to_string() };
    let icon_val = if has_icon { "".to_string() } else {
        if p.is_dir() { "📁".to_string() } else { "🚀".to_string() }
    };

    let new_item = LauncherItem {
        id: generate_id(),
        name: stem,
        target: file_path.to_string(),
        icon_type,
        icon_value: icon_val,
        args: String::new(),
        bg_color: "".to_string(),
        text_color: "".to_string(),
        hotkey_modifiers: Vec::new(),
        hotkey_key: String::new(),
    };

    let mut cfg = config_arc.lock().unwrap();
    if cfg.containers.is_empty() {
        cfg.containers.push(ContainerConfig {
            id: generate_id(),
            name: "Raccourcis".to_string(),
            icon: "📌".to_string(),
            width: 0.0,
            order: 0,
            display_mode: "Both".to_string(),
            bg_color: "".to_string(),
            text_color: "".to_string(),
            hotkey_modifiers: Vec::new(),
            hotkey_key: String::new(),
            row: 0,
            items: vec![new_item],
        });
    } else {
        let safe_idx = target_cont_idx.min(cfg.containers.len() - 1);
        cfg.containers[safe_idx].items.push(new_item);
    }
    save_config(&cfg);
}

fn find_container_at_coordinates(cfg: &AppConfig, x: i32, y: i32, bar_total_width: f32) -> usize {
    if cfg.containers.is_empty() {
        return 0;
    }

    let bar_h = cfg.settings.bar_height.max(20.0);
    let target_row = (y.max(0) as f32 / bar_h) as usize;

    // 1. Récupérer les conteneurs de la ligne correspondante
    let mut row_containers: Vec<(usize, &ContainerConfig)> = cfg.containers
        .iter()
        .enumerate()
        .filter(|(_, c)| c.row == target_row)
        .collect();

    // Si aucun conteneur n'est sur cette ligne spécifique, prendre la ligne 0 ou tous
    if row_containers.is_empty() {
        row_containers = cfg.containers
            .iter()
            .enumerate()
            .filter(|(_, c)| c.row == 0)
            .collect();
    }
    if row_containers.is_empty() {
        row_containers = cfg.containers.iter().enumerate().collect();
    }

    // 2. Calculer la largeur de chaque conteneur
    let font_sz = cfg.settings.container_font_size.max(9.0);
    let mut estimated_widths: Vec<(usize, f32)> = Vec::new();
    let mut total_row_w: f32 = 0.0;

    for (orig_idx, cont) in &row_containers {
        let w = if cont.width > 0.0 {
            cont.width
        } else {
            let mut text_w = 0.0;
            if cont.display_mode != "IconOnly" {
                text_w += cont.name.chars().count() as f32 * (font_sz * 0.70);
            }
            let mut icon_w = 0.0;
            if cont.display_mode != "NameOnly" {
                icon_w += font_sz + 8.0;
            }
            (24.0 + icon_w + text_w + 14.0).max(45.0)
        };
        estimated_widths.push((*orig_idx, w));
        total_row_w += w + 4.0;
    }

    // 3. Offset de départ selon l'alignement
    let is_floating = cfg.settings.bar_position == "Floating";
    let left_pad = if is_floating { 28.0 } else { 6.0 };
    let mut start_x = match cfg.settings.containers_alignment.as_str() {
        "Center" => ((bar_total_width - total_row_w) / 2.0).max(left_pad),
        "Right" => (bar_total_width - total_row_w - 12.0).max(left_pad),
        _ => left_pad,
    };

    let drop_xf = x.max(0) as f32;
    for (orig_idx, w) in estimated_widths {
        if drop_xf >= start_x && drop_xf <= (start_x + w + 4.0) {
            return orig_idx;
        }
        start_x += w + 4.0;
    }

    // Si on a dépassé à droite, renvoyer le dernier conteneur de la ligne
    row_containers.last().map(|(idx, _)| *idx).unwrap_or(0)
}

#[cfg(windows)]
fn register_all_hotkeys_for_app(hwnd: windows_sys::Win32::Foundation::HWND, cfg: &AppConfig) {
    if hwnd.is_null() {
        return;
    }
    // 1. Raccourci Global Principal
    win32_utils::win32::register_hotkey_combo(
        hwnd,
        win32_utils::win32::MAIN_HOTKEY_ID,
        &cfg.settings.hotkey_modifiers,
        &cfg.settings.hotkey_key,
    );

    // 2. Raccourcis Conteneurs (IDs 10000 + i)
    for (i, cont) in cfg.containers.iter().enumerate() {
        let hotkey_id = 10000 + (i as i32);
        if !cont.hotkey_key.is_empty() {
            win32_utils::win32::register_hotkey_combo(
                hwnd,
                hotkey_id,
                &cont.hotkey_modifiers,
                &cont.hotkey_key,
            );
        } else {
            win32_utils::win32::unregister_hotkey_id(hwnd, hotkey_id);
        }
    }

    // 3. Raccourcis Items (IDs 20000 + k)
    let mut item_count = 0i32;
    for cont in &cfg.containers {
        for itm in &cont.items {
            let hotkey_id = 20000 + item_count;
            if !itm.hotkey_key.is_empty() {
                win32_utils::win32::register_hotkey_combo(
                    hwnd,
                    hotkey_id,
                    &itm.hotkey_modifiers,
                    &itm.hotkey_key,
                );
            } else {
                win32_utils::win32::unregister_hotkey_id(hwnd, hotkey_id);
            }
            item_count += 1;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_config = Arc::new(Mutex::new(load_config()));
    let selected_container_idx = Arc::new(AtomicUsize::new(0));
    // Démarrage initial visible
    let is_bar_visible = Arc::new(AtomicBool::new(true));

    let bar_window = BarWindow::new()?;
    let settings_window = SettingsWindow::new()?;

    // Initialisation des données dans la vue du bandeau
    {
        let cfg = app_config.lock().unwrap();
        refresh_bar_ui(&bar_window, &cfg);
    }

    // La fenêtre est initialement rendue hors écran puis masquée par déplacement,
    // afin de ne pas interrompre la boucle d'événements Slint.
    let _ = bar_window.show();

    // Timer d'initialisation Win32 : se déclenche sur le premier tick de l'event loop,
    // moment où le HWND natif est garanti d'exister.
    #[cfg(windows)]
    {
        let cfg_init = app_config.clone();
        slint::Timer::single_shot(std::time::Duration::ZERO, move || {
            use std::sync::atomic::Ordering;

            let apply = |hwnd: windows_sys::Win32::Foundation::HWND| {
                win32_utils::win32::BAR_HWND.store(hwnd as usize, Ordering::SeqCst);
                win32_utils::win32::BAR_EXPLICITLY_HIDDEN.store(false, Ordering::SeqCst);

                let cfg = cfg_init.lock().unwrap();
                let total_h = get_total_bar_height(&cfg);
                win32_utils::win32::set_desktop_parent(hwnd, cfg.settings.stay_on_top);
                win32_utils::win32::setup_bar_window_styles(
                    hwnd,
                    cfg.settings.stay_on_top,
                    cfg.settings.bar_position == "Floating",
                );

                // Positionner au top pleine largeur et afficher sans voler le focus
                win32_utils::win32::position_bar_window(
                    hwnd,
                    &cfg.settings.bar_position,
                    total_h,
                    cfg.settings.bar_x,
                    cfg.settings.bar_y,
                    cfg.settings.bar_width,
                    false,
                    cfg.settings.stay_on_top,
                );

                win32_utils::win32::bring_to_foreground(hwnd, cfg.settings.stay_on_top);
            };

            let hwnd = win32_utils::win32::find_bar_hwnd();
            if hwnd.is_null() {
                // HWND pas encore créé (rare) → relancer dans 50ms
                let cfg_retry = cfg_init.clone();
                slint::Timer::single_shot(std::time::Duration::from_millis(50), move || {
                    use std::sync::atomic::Ordering;
                    let hwnd2 = win32_utils::win32::find_bar_hwnd();
                    if hwnd2.is_null() { return; }
                    win32_utils::win32::BAR_HWND.store(hwnd2 as usize, Ordering::SeqCst);
                    win32_utils::win32::BAR_EXPLICITLY_HIDDEN.store(false, Ordering::SeqCst);
                    let cfg = cfg_retry.lock().unwrap();
                    let total_h = get_total_bar_height(&cfg);
                    win32_utils::win32::set_desktop_parent(hwnd2, cfg.settings.stay_on_top);
                    win32_utils::win32::setup_bar_window_styles(
                        hwnd2,
                        cfg.settings.stay_on_top,
                        cfg.settings.bar_position == "Floating",
                    );
                    win32_utils::win32::position_bar_window(
                        hwnd2,
                        &cfg.settings.bar_position,
                        total_h,
                        cfg.settings.bar_x,
                        cfg.settings.bar_y,
                        cfg.settings.bar_width,
                        false,
                        cfg.settings.stay_on_top,
                    );
                    win32_utils::win32::bring_to_foreground(hwnd2, cfg.settings.stay_on_top);
                });
                return;
            }

            apply(hwnd);
        });
    }

    #[cfg(windows)]
    {
        // Configuration du Systray et des Hotkeys dans un thread de message Win32
        let app_cfg_clone = app_config.clone();
        let bar_weak = bar_window.as_weak();
        let settings_weak = settings_window.as_weak();
        let is_visible_clone = is_bar_visible.clone();
        let sel_idx_clone = selected_container_idx.clone();

        std::thread::spawn(move || {
            use windows_sys::Win32::Foundation::*;
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

            unsafe extern "system" fn tray_wnd_proc(
                hwnd: HWND,
                msg: u32,
                wparam: WPARAM,
                lparam: LPARAM,
            ) -> LRESULT {
                match msg {
                    win32_utils::win32::WM_APP_TRAY => {
                        let event = lparam as u32;
                        if event == WM_LBUTTONUP || event == 0x0402 /* NIN_SELECT */ || event == 0x0400 /* NIN_KEYSELECT */ {
                            TRAY_HANDLER.with(|th| {
                                if let Some(handler) = th.borrow().as_ref() {
                                    (handler.toggle_bar)();
                                }
                            });
                        } else if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                            TRAY_HANDLER.with(|th| {
                                if let Some(handler) = th.borrow().as_ref() {
                                    (handler.show_menu)(hwnd);
                                }
                            });
                        }
                    }
                    WM_COMMAND => {
                        let cmd_id = (wparam & 0xffff) as usize;
                        TRAY_HANDLER.with(|th| {
                            if let Some(handler) = th.borrow().as_ref() {
                                (handler.handle_menu_cmd)(cmd_id);
                            }
                        });
                    }
                    WM_HOTKEY => {
                        let hk_id = wparam as i32;
                        TRAY_HANDLER.with(|th| {
                            if let Some(handler) = th.borrow().as_ref() {
                                (handler.on_hotkey)(hk_id);
                            }
                        });
                    }
                    WM_DROPFILES => {
                        use windows_sys::Win32::UI::Shell::*;
                        let hdrop = wparam as HDROP;
                        let count = unsafe { DragQueryFileW(hdrop, 0xffffffff, std::ptr::null_mut(), 0) };
                        for i in 0..count {
                            let mut buf: [u16; 512] = [0; 512];
                            let len = unsafe { DragQueryFileW(hdrop, i, buf.as_mut_ptr(), 512) };
                            if len > 0 {
                                let path = String::from_utf16_lossy(&buf[..len as usize]);
                                TRAY_HANDLER.with(|th| {
                                    if let Some(handler) = th.borrow().as_ref() {
                                        (handler.on_drop_file)(path);
                                    }
                                });
                            }
                        }
                        unsafe { DragFinish(hdrop); }
                    }
                    WM_TIMER => {
                        if wparam == win32_utils::win32::VISIBILITY_TIMER_ID {
                            let bar_hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst)
                                as HWND;
                            if win32_utils::win32::STAY_ON_TOP_ENABLED.load(Ordering::SeqCst)
                                && !win32_utils::win32::BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst)
                                && !bar_hwnd.is_null()
                                && unsafe { IsWindowVisible(bar_hwnd) == 0 }
                            {
                                unsafe {
                                    ShowWindow(bar_hwnd, SW_SHOWNOACTIVATE);
                                    SetWindowPos(
                                        bar_hwnd,
                                        HWND_TOP,
                                        0,
                                        0,
                                        0,
                                        0,
                                        SWP_NOMOVE
                                            | SWP_NOSIZE
                                            | SWP_NOACTIVATE
                                            | SWP_SHOWWINDOW,
                                    );
                                }
                                win32_utils::win32::BAR_WINDOW_VISIBLE
                                    .store(true, Ordering::SeqCst);
                            }
                        }
                    }
                    _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
                }
                0
            }

            struct TrayHandlers {
                toggle_bar: Box<dyn Fn() + Send>,
                show_menu: Box<dyn Fn(HWND) + Send>,
                handle_menu_cmd: Box<dyn Fn(usize) + Send>,
                on_hotkey: Box<dyn Fn(i32) + Send>,
                on_drop_file: Box<dyn Fn(String) + Send>,
            }

            thread_local! {
                static TRAY_HANDLER: RefCell<Option<TrayHandlers>> = const { RefCell::new(None) };
            }

            unsafe {
                let class_name = win32_utils::win32::to_wide_null("LanceurTrayMsgClass");
                let mut wc: WNDCLASSEXW = std::mem::zeroed();
                wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
                wc.lpfnWndProc = Some(tray_wnd_proc);
                wc.hInstance = std::ptr::null_mut();
                wc.lpszClassName = class_name.as_ptr();
                RegisterClassExW(&wc);

                let msg_hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    win32_utils::win32::to_wide_null("LanceurTrayMsgWindow").as_ptr(),
                    0,
                    0, 0, 0, 0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                );

                win32_utils::win32::SYSTRAY_HWND.store(msg_hwnd as usize, Ordering::SeqCst);
                win32_utils::win32::create_tray_icon(msg_hwnd, "⚡ Lanceur d'Applications");
                SetTimer(msg_hwnd, win32_utils::win32::VISIBILITY_TIMER_ID, 50, None);

                // Enregistrer tous les raccourcis configurés
                {
                    let cfg = app_cfg_clone.lock().unwrap();
                    win32_utils::win32::AUTOSTART_ENABLED.store(cfg.settings.autostart, Ordering::SeqCst);
                    register_all_hotkeys_for_app(msg_hwnd, &cfg);
                }

                // Définition des handlers
                let bw_for_toggle = bar_weak.clone();
                let is_vis_for_toggle = is_visible_clone.clone();
                let toggle_bar = Box::new(move || {
                    let bw = bw_for_toggle.clone();
                    let is_vis = is_vis_for_toggle.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = bw.upgrade() {
                            let is_fore = win32_utils::win32::is_bar_window_foreground();
                            let is_vis_on_screen = win32_utils::win32::is_bar_window_visible();
                            if is_fore && is_vis_on_screen {
                                hide_bar_window();
                                is_vis.store(false, Ordering::SeqCst);
                            } else {
                                show_bar_window(&ui, false);
                                is_vis.store(true, Ordering::SeqCst);
                            }
                        }
                    });
                });

                let cfg_for_menu = app_cfg_clone.clone();
                let show_menu = Box::new(move |hwnd: HWND| {
                    let is_auto = cfg_for_menu.lock().unwrap().settings.autostart;
                    win32_utils::win32::show_tray_context_menu(hwnd, is_auto);
                });

                let bw_for_cmd = bar_weak.clone();
                let sw_for_cmd = settings_weak.clone();
                let is_vis_for_cmd = is_visible_clone.clone();
                let cfg_for_cmd = app_cfg_clone.clone();
                let sel_idx_for_cmd = sel_idx_clone.clone();

                let handle_menu_cmd = Box::new(move |cmd_id: usize| {
                    match cmd_id {
                        win32_utils::win32::IDM_SHOW_HIDE => {
                            let bw = bw_for_cmd.clone();
                            let is_vis = is_vis_for_cmd.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = bw.upgrade() {
                                    let is_fore = win32_utils::win32::is_bar_window_foreground();
                                    let is_vis_on_screen = win32_utils::win32::is_bar_window_visible();
                                    if is_fore && is_vis_on_screen {
                                        hide_bar_window();
                                        is_vis.store(false, Ordering::SeqCst);
                                    } else {
                                        show_bar_window(&ui, false);
                                        is_vis.store(true, Ordering::SeqCst);
                                    }
                                }
                            });
                        }
                        win32_utils::win32::IDM_SETTINGS => {
                            let sw = sw_for_cmd.clone();
                            let cfg = cfg_for_cmd.clone();
                            let sel = sel_idx_for_cmd.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(sui) = sw.upgrade() {
                                    let cfg_guard = cfg.lock().unwrap();
                                    let idx = sel.load(Ordering::SeqCst);
                                    refresh_settings_ui(&sui, &cfg_guard, idx);
                                    let _ = sui.show();

                                    #[cfg(windows)]
                                    {
                                        use windows_sys::Win32::Foundation::HWND;
                                        use windows_sys::Win32::UI::WindowsAndMessaging::*;
                                        let bring_to_front = || {
                                            let hwnd: HWND = win32_utils::win32::find_settings_hwnd();
                                            if !hwnd.is_null() {
                                                let mut pid: u32 = 0;
                                                GetWindowThreadProcessId(hwnd, &mut pid);
                                                if pid == windows_sys::Win32::System::Threading::GetCurrentProcessId() {
                                                    win32_utils::win32::setup_settings_window_styles(hwnd);
                                                    ShowWindow(hwnd, SW_RESTORE);
                                                    SetForegroundWindow(hwnd);
                                                    BringWindowToTop(hwnd);
                                                    windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(hwnd);
                                                }
                                            }
                                        };
                                        bring_to_front();
                                        slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
                                            bring_to_front();
                                        });
                                    }
                                }
                            });
                        }
                        win32_utils::win32::IDM_AUTOSTART => {
                            let mut c = cfg_for_cmd.lock().unwrap();
                            c.settings.autostart = !c.settings.autostart;
                            let _ = win32_utils::win32::set_autostart(c.settings.autostart);
                            win32_utils::win32::AUTOSTART_ENABLED.store(c.settings.autostart, Ordering::SeqCst);
                            save_config(&c);
                        }
                        win32_utils::win32::IDM_QUIT => {
                            let _ = slint::invoke_from_event_loop(move || {
                                let _ = slint::quit_event_loop();
                            });
                        }
                        _ => {}
                    }
                });

                let cfg_for_hk = app_cfg_clone.clone();
                let bw_for_hk = bar_weak.clone();
                let is_vis_for_hk = is_visible_clone.clone();

                let on_hotkey = Box::new(move |hk_id: i32| {
                    if hk_id == win32_utils::win32::MAIN_HOTKEY_ID {
                        let bw = bw_for_hk.clone();
                        let is_vis = is_vis_for_hk.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = bw.upgrade() {
                                // Toujours afficher et amener au premier plan sans basculer en masquage
                                show_bar_window(&ui, false);
                                is_vis.store(true, Ordering::SeqCst);
                            }
                        });
                    } else if (10000..20000).contains(&hk_id) {
                        // Ouvrir le conteneur spécifié
                        let cont_idx = (hk_id - 10000) as i32;
                        let bw = bw_for_hk.clone();
                        let is_vis = is_vis_for_hk.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = bw.upgrade() {
                                show_bar_window(&ui, true);
                                is_vis.store(true, Ordering::SeqCst);
                                ui.set_active_dropdown_idx(cont_idx);
                            }
                        });
                    } else if hk_id >= 20000 {
                        // Lancer l'item directement
                        let item_idx_flat = (hk_id - 20000) as usize;
                        let cfg = cfg_for_hk.lock().unwrap();
                        let mut count = 0usize;
                        let mut target_to_launch: Option<String> = None;
                        for cont in &cfg.containers {
                            for itm in &cont.items {
                                if count == item_idx_flat {
                                    target_to_launch = Some(itm.target.clone());
                                    break;
                                }
                                count += 1;
                            }
                            if target_to_launch.is_some() { break; }
                        }
                        if let Some(target) = target_to_launch {
                            let _ = open::that(&target);
                            trim_process_memory();
                        }
                    }
                });

                let cfg_for_drop = app_cfg_clone.clone();
                let bw_for_drop = bar_weak.clone();
                let sw_for_drop = settings_weak.clone();
                let sel_for_drop = sel_idx_clone.clone();

                let on_drop_file = Box::new(move |file_path: String| {
                    let cfg_arc = cfg_for_drop.clone();
                    let bw = bw_for_drop.clone();
                    let sw = sw_for_drop.clone();
                    let sel = sel_for_drop.clone();

                    let _ = slint::invoke_from_event_loop(move || {
                        add_dropped_file_to_container(&file_path, 0, &cfg_arc);
                        let cfg_guard = cfg_arc.lock().unwrap();
                        if let Some(bui) = bw.upgrade() {
                            refresh_bar_ui(&bui, &cfg_guard);
                        }
                        if let Some(sui) = sw.upgrade() {
                            refresh_settings_ui(&sui, &cfg_guard, sel.load(Ordering::SeqCst));
                        }
                        trim_process_memory();
                    });
                });

                TRAY_HANDLER.with(|th| {
                    *th.borrow_mut() = Some(TrayHandlers {
                        toggle_bar,
                        show_menu,
                        handle_menu_cmd,
                        on_hotkey,
                        on_drop_file,
                    });
                });

                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                win32_utils::win32::remove_tray_icon(msg_hwnd);
            }
        });
    }

    // ================= ENREGISTREMENT DU DRAG & DROP GLOBAL =================
    {
        let cfg_for_drop = app_config.clone();
        let bw_for_drop = bar_window.as_weak();
        let sw_for_drop = settings_window.as_weak();
        let sel_for_drop = selected_container_idx.clone();

        win32_utils::win32::set_drop_callback(move |files, drop_x, drop_y, is_settings| {
            let cfg_arc = cfg_for_drop.clone();
            let bw = bw_for_drop.clone();
            let sw = sw_for_drop.clone();
            let sel = sel_for_drop.clone();

            let _ = slint::invoke_from_event_loop(move || {
                let target_cont_idx = if is_settings {
                    sel.load(Ordering::SeqCst)
                } else {
                    let cfg_guard = cfg_arc.lock().unwrap();
                    let bar_w = if let Some(bui) = bw.upgrade() {
                        let sz = bui.window().size();
                        if sz.width > 0 { sz.width as f32 } else { 1920.0 }
                    } else {
                        1920.0
                    };
                    find_container_at_coordinates(&cfg_guard, drop_x, drop_y, bar_w)
                };

                for f in files {
                    add_dropped_file_to_container(&f, target_cont_idx, &cfg_arc);
                }

                let cfg_guard = cfg_arc.lock().unwrap();
                if let Some(bui) = bw.upgrade() {
                    refresh_bar_ui(&bui, &cfg_guard);
                }
                if let Some(sui) = sw.upgrade() {
                    refresh_settings_ui(&sui, &cfg_guard, sel.load(Ordering::SeqCst));
                }
                trim_process_memory();
            });
        });
    }

    // ================= CALLBACKS DU BANDEAU (BAR WINDOW) =================
    {
        bar_window.on_launch_item(move |target| {
            let target_str = target.to_string();
            let _ = open::that(&target_str);
            trim_process_memory();
        });
    }

    {
        let app_cfg_clone = app_config.clone();
        bar_window.on_dropdown_state_changed(move |is_expanded| {
            #[cfg(windows)]
            {
                use windows_sys::Win32::Foundation::HWND;
                let hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst) as HWND;
                if !hwnd.is_null() {
                    let cfg = app_cfg_clone.lock().unwrap();
                    let total_h = get_total_bar_height(&cfg);
                    win32_utils::win32::set_desktop_parent(hwnd, cfg.settings.stay_on_top);
                    win32_utils::win32::position_bar_window(
                        hwnd,
                        &cfg.settings.bar_position,
                        total_h,
                        cfg.settings.bar_x,
                        cfg.settings.bar_y,
                        cfg.settings.bar_width,
                        is_expanded,
                        cfg.settings.stay_on_top,
                    );
                }
            }
        });
    }

    #[cfg(windows)]
    {
        let drag_origin = Arc::new(Mutex::new(None::<(i32, i32)>));
        let drag_origin_start = drag_origin.clone();
        bar_window.on_window_drag_started(move || {
            use windows_sys::Win32::Foundation::{HWND, RECT};
            use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
            let hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst) as HWND;
            if !hwnd.is_null() {
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                unsafe {
                    if GetWindowRect(hwnd, &mut rect) != 0 {
                        *drag_origin_start.lock().unwrap() = Some((rect.left, rect.top));
                    }
                }
            }
        });

        let drag_origin_move = drag_origin.clone();
        let drag_config = app_config.clone();
        bar_window.on_window_dragged(move |dx, dy| {
            let hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst)
                as windows_sys::Win32::Foundation::HWND;
            if let Some((x, y)) = *drag_origin_move.lock().unwrap() {
                let new_x = x + dx as i32;
                let new_y = y + dy as i32;
                win32_utils::win32::move_bar_window_to(hwnd, new_x, new_y);
                let mut cfg = drag_config.lock().unwrap();
                cfg.settings.bar_x = new_x;
                cfg.settings.bar_y = new_y;
                save_config(&cfg);
            }
        });

        let resize_config = app_config.clone();
        bar_window.on_window_width_changed(move |new_width| {
            let mut cfg = resize_config.lock().unwrap();
            cfg.settings.bar_width = (new_width as i32).max(260);
            save_config(&cfg);
            let hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst)
                as windows_sys::Win32::Foundation::HWND;
            if !hwnd.is_null() {
                let total_h = get_total_bar_height(&cfg);
                win32_utils::win32::position_bar_window(
                    hwnd,
                    &cfg.settings.bar_position,
                    total_h,
                    cfg.settings.bar_x,
                    cfg.settings.bar_y,
                    cfg.settings.bar_width,
                    false,
                    cfg.settings.stay_on_top,
                );
            }
        });

        let dropdown_config = app_config.clone();
        bar_window.on_dropdown_state_changed(move |is_expanded| {
            #[cfg(windows)]
            {
                use windows_sys::Win32::Foundation::HWND;
                let hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst) as HWND;
                if !hwnd.is_null() {
                    let cfg = dropdown_config.lock().unwrap();
                    let total_h = get_total_bar_height(&cfg);
                    win32_utils::win32::position_bar_window(
                        hwnd,
                        &cfg.settings.bar_position,
                        total_h,
                        cfg.settings.bar_x,
                        cfg.settings.bar_y,
                        cfg.settings.bar_width,
                        is_expanded,
                        cfg.settings.stay_on_top,
                    );
                }
            }
        });
    }

    // Redimensionnement de conteneur à la souris
    {
        let app_cfg_clone = app_config.clone();
        let bar_weak = bar_window.as_weak();
        bar_window.on_container_width_changed(move |idx, new_width| {
            let i = idx as usize;
            let mut cfg = app_cfg_clone.lock().unwrap();
            if let Some(cont) = cfg.containers.get_mut(i) {
                cont.width = new_width;
                save_config(&cfg);
                if let Some(bui) = bar_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
            }
        });
    }

    {
        let settings_weak = settings_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        bar_window.on_open_settings(move || {
            if let Some(sui) = settings_weak.upgrade() {
                let cfg = cfg_arc.lock().unwrap();
                refresh_settings_ui(&sui, &cfg, sel_idx.load(Ordering::SeqCst));
                let _ = sui.show();

                #[cfg(windows)]
                {
                    use windows_sys::Win32::Foundation::HWND;
                    use windows_sys::Win32::UI::WindowsAndMessaging::*;
                    let bring_to_front = || {
                        let hwnd: HWND = win32_utils::win32::find_settings_hwnd();
                        if !hwnd.is_null() {
                            unsafe {
                                let mut pid: u32 = 0;
                                GetWindowThreadProcessId(hwnd, &mut pid);
                                if pid == windows_sys::Win32::System::Threading::GetCurrentProcessId() {
                                    win32_utils::win32::setup_settings_window_styles(hwnd);
                                    ShowWindow(hwnd, SW_RESTORE);
                                    SetForegroundWindow(hwnd);
                                    BringWindowToTop(hwnd);
                                    windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(hwnd);
                                }
                            }
                        }
                    };
                    bring_to_front();
                    slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
                        bring_to_front();
                    });
                }
            }
        });
    }

    {
        bar_window.on_open_context_menu(move || {
            #[cfg(windows)]
            {
                use windows_sys::Win32::Foundation::*;
                use windows_sys::Win32::UI::WindowsAndMessaging::*;
                let tray_hwnd = win32_utils::win32::SYSTRAY_HWND.load(Ordering::SeqCst) as HWND;
                if !tray_hwnd.is_null() {
                    unsafe {
                        PostMessageW(
                            tray_hwnd,
                            win32_utils::win32::WM_APP_TRAY,
                            0,
                            WM_RBUTTONUP as LPARAM,
                        );
                    }
                }
            }
        });
    }

    // ================= CALLBACKS DES PARAMÈTRES (SETTINGS WINDOW) =================
    // 1. Sélection d'un conteneur
    {
        let settings_weak = settings_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_select_container(move |idx| {
            if let Some(sui) = settings_weak.upgrade() {
                sel_idx.store(idx as usize, Ordering::SeqCst);
                let cfg = cfg_arc.lock().unwrap();
                refresh_settings_ui(&sui, &cfg, idx as usize);
            }
        });
    }

    // 2. Ajouter un conteneur
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_add_container(move || {
            if let Some(sui) = settings_weak.upgrade() {
                let mut cfg = cfg_arc.lock().unwrap();
                let new_cont = ContainerConfig {
                    id: generate_id(),
                    name: format!("Nouveau {}", cfg.containers.len() + 1),
                    icon: "📁".to_string(),
                    width: 0.0,
                    order: cfg.containers.len(),
                    display_mode: "Both".to_string(),
                    bg_color: "".to_string(),
                    text_color: "".to_string(),
                    hotkey_modifiers: Vec::new(),
                    hotkey_key: String::new(),
                    row: 0,
                    items: Vec::new(),
                };
                cfg.containers.push(new_cont);
                save_config(&cfg);
                let new_idx = cfg.containers.len() - 1;
                sel_idx.store(new_idx, Ordering::SeqCst);
                refresh_settings_ui(&sui, &cfg, new_idx);
                if let Some(bui) = bar_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
                update_bar_window_geometry(&cfg);
            }
        });
    }

    // 3. Supprimer un conteneur
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_delete_container(move |idx| {
            if let Some(sui) = settings_weak.upgrade() {
                let i = idx as usize;
                let mut cfg = cfg_arc.lock().unwrap();
                if i < cfg.containers.len() && cfg.containers.len() > 1 {
                    cfg.containers.remove(i);
                    save_config(&cfg);
                    sel_idx.store(0, Ordering::SeqCst);
                    refresh_settings_ui(&sui, &cfg, 0);
                    if let Some(bui) = bar_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                    update_bar_window_geometry(&cfg);
                }
            }
        });
    }

    // 4. Monter / Descendre un conteneur
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        let s_weak = settings_weak.clone();
        let b_weak = bar_weak.clone();
        let c_arc = cfg_arc.clone();
        let s_idx = sel_idx.clone();
        settings_window.on_move_container_up(move |idx| {
            let i = idx as usize;
            if i > 0 {
                let mut cfg = c_arc.lock().unwrap();
                cfg.containers.swap(i, i - 1);
                save_config(&cfg);
                s_idx.store(i - 1, Ordering::SeqCst);
                if let Some(sui) = s_weak.upgrade() {
                    refresh_settings_ui(&sui, &cfg, i - 1);
                }
                if let Some(bui) = b_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
                update_bar_window_geometry(&cfg);
            }
        });

        let s_weak2 = settings_weak.clone();
        let b_weak2 = bar_weak.clone();
        let c_arc2 = cfg_arc.clone();
        let s_idx2 = sel_idx.clone();
        settings_window.on_move_container_down(move |idx| {
            let i = idx as usize;
            let mut cfg = c_arc2.lock().unwrap();
            if i + 1 < cfg.containers.len() {
                cfg.containers.swap(i, i + 1);
                save_config(&cfg);
                s_idx2.store(i + 1, Ordering::SeqCst);
                if let Some(sui) = s_weak2.upgrade() {
                    refresh_settings_ui(&sui, &cfg, i + 1);
                }
                if let Some(bui) = b_weak2.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
                update_bar_window_geometry(&cfg);
            }
        });
    }

    // 5. Sauvegarder les infos du conteneur (nom, icône, largeur, mode affichage, couleurs, raccourci dédié, ligne)
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_save_container_info(move || {
            if let Some(sui) = settings_weak.upgrade() {
                let idx = sel_idx.load(Ordering::SeqCst);
                let mut cfg = cfg_arc.lock().unwrap();
                if let Some(cont) = cfg.containers.get_mut(idx) {
                    cont.name = sui.get_edit_container_name().to_string();
                    cont.icon = sui.get_edit_container_icon().to_string();
                    cont.width = sui.get_edit_container_width();
                    cont.display_mode = sui.get_edit_container_display_mode().to_string();
                    cont.bg_color = sui.get_edit_container_bg().to_string();
                    cont.text_color = sui.get_edit_container_text().to_string();
                    cont.row = sui.get_edit_container_row() as usize;

                    let mut mods = Vec::new();
                    if sui.get_edit_cont_mod_ctrl() { mods.push("Control".to_string()); }
                    if sui.get_edit_cont_mod_alt() { mods.push("Alt".to_string()); }
                    if sui.get_edit_cont_mod_shift() { mods.push("Shift".to_string()); }
                    if sui.get_edit_cont_mod_win() { mods.push("Win".to_string()); }
                    cont.hotkey_modifiers = mods;
                    cont.hotkey_key = sui.get_edit_cont_hotkey_key().to_string();

                    save_config(&cfg);
                    update_bar_window_geometry(&cfg);

                    #[cfg(windows)]
                    {
                        use windows_sys::Win32::Foundation::HWND;
                        let tray_hwnd = win32_utils::win32::SYSTRAY_HWND.load(Ordering::SeqCst) as HWND;
                        if !tray_hwnd.is_null() {
                            register_all_hotkeys_for_app(tray_hwnd, &cfg);
                        }
                    }

                    refresh_settings_ui(&sui, &cfg, idx);
                    if let Some(bui) = bar_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                }
            }
        });
    }

    // 6. Gestion des Items : Ouvrir ajout / modification
    {
        let settings_weak = settings_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        let s_weak = settings_weak.clone();
        settings_window.on_open_add_item(move || {
            if let Some(sui) = s_weak.upgrade() {
                sui.set_selected_item_index(-1);
                sui.set_item_edit_name("".into());
                sui.set_item_edit_target("".into());
                sui.set_item_edit_icon_type("emoji".into());
                sui.set_item_edit_icon_value("🚀".into());
                sui.set_item_edit_bg("".into());
                sui.set_item_edit_text("".into());
                sui.set_item_edit_mod_ctrl(false);
                sui.set_item_edit_mod_alt(false);
                sui.set_item_edit_mod_shift(false);
                sui.set_item_edit_mod_win(false);
                sui.set_item_edit_hotkey_key("".into());
                sui.set_show_item_editor(true);
            }
        });

        let s_weak2 = settings_weak.clone();
        let c_arc2 = cfg_arc.clone();
        let s_idx2 = sel_idx.clone();
        settings_window.on_open_edit_item(move |item_idx| {
            if let Some(sui) = s_weak2.upgrade() {
                let c_idx = s_idx2.load(Ordering::SeqCst);
                let i_idx = item_idx as usize;
                let cfg = c_arc2.lock().unwrap();
                if let Some(cont) = cfg.containers.get(c_idx) {
                    if let Some(itm) = cont.items.get(i_idx) {
                        sui.set_selected_item_index(item_idx);
                        sui.set_item_edit_name(itm.name.clone().into());
                        sui.set_item_edit_target(itm.target.clone().into());
                        sui.set_item_edit_icon_type(itm.icon_type.clone().into());
                        sui.set_item_edit_icon_value(itm.icon_value.clone().into());
                        sui.set_item_edit_bg(itm.bg_color.clone().into());
                        sui.set_item_edit_text(itm.text_color.clone().into());
                        sui.set_item_target_container_idx(c_idx as i32);
                        sui.set_item_edit_mod_ctrl(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("control") || m.eq_ignore_ascii_case("ctrl")));
                        sui.set_item_edit_mod_alt(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("alt")));
                        sui.set_item_edit_mod_shift(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("shift")));
                        sui.set_item_edit_mod_win(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("windows")));
                        sui.set_item_edit_hotkey_key(itm.hotkey_key.clone().into());
                        sui.set_show_item_editor(true);
                    }
                }
            }
        });
    }

    // 7. Parcourir fichier pour cible d'item
    {
        let settings_weak = settings_window.as_weak();
        settings_window.on_browse_item_target(move || {
            if let Some(sui) = settings_weak.upgrade() {
                if let Some(file) = rfd::FileDialog::new().pick_file() {
                    let path_str = file.to_string_lossy().to_string();
                    sui.set_item_edit_target(path_str.into());
                    if sui.get_item_edit_name().is_empty() {
                        if let Some(stem) = file.file_stem() {
                            sui.set_item_edit_name(stem.to_string_lossy().to_string().into());
                        }
                    }
                }
            }
        });
    }

    // 8. Sauvegarder item depuis l'éditeur
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_save_item_editor(move || {
            if let Some(sui) = settings_weak.upgrade() {
                let c_idx = sel_idx.load(Ordering::SeqCst);
                let item_idx = sui.get_selected_item_index();
                let name = sui.get_item_edit_name().to_string();
                let target = sui.get_item_edit_target().to_string();
                let icon_type = sui.get_item_edit_icon_type().to_string();
                let icon_value = sui.get_item_edit_icon_value().to_string();
                let bg_color = sui.get_item_edit_bg().to_string();
                let text_color = sui.get_item_edit_text().to_string();

                let mut mods = Vec::new();
                if sui.get_item_edit_mod_ctrl() { mods.push("Control".to_string()); }
                if sui.get_item_edit_mod_alt() { mods.push("Alt".to_string()); }
                if sui.get_item_edit_mod_shift() { mods.push("Shift".to_string()); }
                if sui.get_item_edit_mod_win() { mods.push("Win".to_string()); }
                let hotkey_key = sui.get_item_edit_hotkey_key().to_string();

                if !name.trim().is_empty() {
                    let mut cfg = cfg_arc.lock().unwrap();
                    if let Some(cont) = cfg.containers.get_mut(c_idx) {
                        if item_idx >= 0 && (item_idx as usize) < cont.items.len() {
                            let itm = &mut cont.items[item_idx as usize];
                            itm.name = name;
                            itm.target = target;
                            itm.icon_type = icon_type;
                            itm.icon_value = icon_value;
                            itm.bg_color = bg_color;
                            itm.text_color = text_color;
                            itm.hotkey_modifiers = mods;
                            itm.hotkey_key = hotkey_key;
                        } else {
                            cont.items.push(LauncherItem {
                                id: generate_id(),
                                name,
                                target,
                                icon_type,
                                icon_value,
                                args: String::new(),
                                bg_color,
                                text_color,
                                hotkey_modifiers: mods,
                                hotkey_key,
                            });
                        }
                        save_config(&cfg);

                        #[cfg(windows)]
                        {
                            use windows_sys::Win32::Foundation::HWND;
                            let tray_hwnd = win32_utils::win32::SYSTRAY_HWND.load(Ordering::SeqCst) as HWND;
                            register_all_hotkeys_for_app(tray_hwnd, &cfg);
                        }

                        sui.set_show_item_editor(false);
                        refresh_settings_ui(&sui, &cfg, c_idx);
                        if let Some(bui) = bar_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });
    }

    // 9. Supprimer un item
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_delete_item(move |item_idx| {
            if let Some(sui) = settings_weak.upgrade() {
                let c_idx = sel_idx.load(Ordering::SeqCst);
                let i_idx = item_idx as usize;
                let mut cfg = cfg_arc.lock().unwrap();
                if let Some(cont) = cfg.containers.get_mut(c_idx) {
                    if i_idx < cont.items.len() {
                        cont.items.remove(i_idx);
                        save_config(&cfg);
                        refresh_settings_ui(&sui, &cfg, c_idx);
                        if let Some(bui) = bar_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });
    }

    // 10. Monter / Descendre un item
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        let s_weak = settings_weak.clone();
        let b_weak = bar_weak.clone();
        let c_arc = cfg_arc.clone();
        let s_idx = sel_idx.clone();
        settings_window.on_move_item_up(move |item_idx| {
            let i = item_idx as usize;
            if i > 0 {
                let c_idx = s_idx.load(Ordering::SeqCst);
                let mut cfg = c_arc.lock().unwrap();
                if let Some(cont) = cfg.containers.get_mut(c_idx) {
                    cont.items.swap(i, i - 1);
                    save_config(&cfg);
                    if let Some(sui) = s_weak.upgrade() {
                        refresh_settings_ui(&sui, &cfg, c_idx);
                    }
                    if let Some(bui) = b_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                }
            }
        });

        let s_weak2 = settings_weak.clone();
        let b_weak2 = bar_weak.clone();
        let c_arc2 = cfg_arc.clone();
        let s_idx2 = sel_idx.clone();
        settings_window.on_move_item_down(move |item_idx| {
            let i = item_idx as usize;
            let c_idx = s_idx2.load(Ordering::SeqCst);
            let mut cfg = c_arc2.lock().unwrap();
            if let Some(cont) = cfg.containers.get_mut(c_idx) {
                if i + 1 < cont.items.len() {
                    cont.items.swap(i, i + 1);
                    save_config(&cfg);
                    if let Some(sui) = s_weak2.upgrade() {
                        refresh_settings_ui(&sui, &cfg, c_idx);
                    }
                    if let Some(bui) = b_weak2.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                }
            }
        });
    }

    // 11. Déplacer un item vers un autre conteneur
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_move_item_to_container(move |item_idx, target_cont_idx| {
            if let Some(sui) = settings_weak.upgrade() {
                let src_c = sel_idx.load(Ordering::SeqCst);
                let dst_c = target_cont_idx as usize;
                let i_idx = item_idx as usize;

                let mut cfg = cfg_arc.lock().unwrap();
                if src_c != dst_c && src_c < cfg.containers.len() && dst_c < cfg.containers.len() {
                    if i_idx < cfg.containers[src_c].items.len() {
                        let itm = cfg.containers[src_c].items.remove(i_idx);
                        cfg.containers[dst_c].items.push(itm);
                        save_config(&cfg);
                        sui.set_show_item_editor(false);
                        refresh_settings_ui(&sui, &cfg, src_c);
                        if let Some(bui) = bar_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });
    }

    // 12. Dupliquer un item vers ce conteneur ou un autre conteneur
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_duplicate_item(move |item_idx, target_cont_idx| {
            if let Some(sui) = settings_weak.upgrade() {
                let src_c = sel_idx.load(Ordering::SeqCst);
                let dst_c = target_cont_idx as usize;
                let i_idx = item_idx as usize;

                let mut cfg = cfg_arc.lock().unwrap();
                if src_c < cfg.containers.len() && dst_c < cfg.containers.len() {
                    if let Some(orig) = cfg.containers[src_c].items.get(i_idx).cloned() {
                        let mut duplicated = orig;
                        duplicated.id = generate_id();
                        duplicated.name = format!("{} (Copie)", duplicated.name);
                        cfg.containers[dst_c].items.push(duplicated);
                        save_config(&cfg);
                        sui.set_show_item_editor(false);
                        refresh_settings_ui(&sui, &cfg, src_c);
                        if let Some(bui) = bar_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });
    }

    // 13. Sauvegarde globale de tous les paramètres + Toast 3s
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_save_all_preferences(move || {
            if let Some(sui) = settings_weak.upgrade() {
                let mut cfg = cfg_arc.lock().unwrap();
                cfg.settings.bar_position = sui.get_pref_position().to_string();
                cfg.settings.containers_alignment = sui.get_pref_containers_align().to_string();
                cfg.settings.bar_height = sui.get_pref_bar_h();
                cfg.settings.item_height = sui.get_pref_item_h();
                cfg.settings.container_font_size = sui.get_pref_cont_font();
                cfg.settings.item_font_size = sui.get_pref_item_font();
                cfg.settings.bar_bg_color = sui.get_pref_bar_bg_color().to_string();
                cfg.settings.bar_text_color = sui.get_pref_bar_text_color().to_string();

                let mut mods = Vec::new();
                if sui.get_pref_mod_ctrl() { mods.push("Control".to_string()); }
                if sui.get_pref_mod_alt() { mods.push("Alt".to_string()); }
                if sui.get_pref_mod_shift() { mods.push("Shift".to_string()); }
                if sui.get_pref_mod_win() { mods.push("Win".to_string()); }
                cfg.settings.hotkey_modifiers = mods;
                cfg.settings.hotkey_key = sui.get_pref_hotkey_key().to_string();
                cfg.settings.autostart = sui.get_pref_autostart();
                cfg.settings.stay_on_top = sui.get_pref_stay_on_top();

                save_config(&cfg);

                #[cfg(windows)]
                {
                    use windows_sys::Win32::Foundation::HWND;
                    let _ = win32_utils::win32::set_autostart(cfg.settings.autostart);
                    win32_utils::win32::AUTOSTART_ENABLED.store(cfg.settings.autostart, Ordering::SeqCst);

                    let hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst) as HWND;
                    if !hwnd.is_null() {
                        let total_h = get_total_bar_height(&cfg);
                        win32_utils::win32::set_desktop_parent(hwnd, cfg.settings.stay_on_top);
                        win32_utils::win32::setup_bar_window_styles(
                            hwnd,
                            cfg.settings.stay_on_top,
                            cfg.settings.bar_position == "Floating",
                        );
                        win32_utils::win32::position_bar_window(
                            hwnd,
                            &cfg.settings.bar_position,
                            total_h,
                            cfg.settings.bar_x,
                            cfg.settings.bar_y,
                            cfg.settings.bar_width,
                            false,
                            cfg.settings.stay_on_top,
                        );
                        win32_utils::win32::bring_to_foreground(hwnd, cfg.settings.stay_on_top);
                    }

                    let tray_hwnd = win32_utils::win32::SYSTRAY_HWND.load(Ordering::SeqCst) as HWND;
                    if !tray_hwnd.is_null() {
                        register_all_hotkeys_for_app(tray_hwnd, &cfg);
                    }
                }

                if let Some(bui) = bar_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
                refresh_settings_ui(&sui, &cfg, sel_idx.load(Ordering::SeqCst));

                // Afficher le toast puis le masquer automatiquement après 3s
                sui.set_show_saved_toast(true);
                let sw_toast = settings_weak.clone();
                slint::Timer::single_shot(std::time::Duration::from_secs(3), move || {
                    if let Some(ui) = sw_toast.upgrade() {
                        ui.set_show_saved_toast(false);
                    }
                });

                trim_process_memory();
            }
        });
    }

    // run_event_loop() (et non run_event_loop_until_quit()) :
    // l'event loop ne quitte QUE sur un appel explicite à slint::quit_event_loop().
    // Cela permet d'utiliser SW_HIDE librement sans risquer de terminer l'appli.
    slint::run_event_loop()?;
    Ok(())
}
