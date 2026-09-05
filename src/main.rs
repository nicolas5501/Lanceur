#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code, unused_imports, unused_variables)]

mod config;
mod win32_utils;

use config::*;
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

slint::include_modules!();

fn trim_process_memory() {
    win32_utils::win32::trim_process_memory();
}

fn parse_hex_color(hex_str: &str, default: Color) -> Color {
    let s = hex_str.trim().trim_start_matches('#');
    if s.len() == 3 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..1], 16),
            u8::from_str_radix(&s[1..2], 16),
            u8::from_str_radix(&s[2..3], 16),
        ) {
            return Color::from_argb_u8(255, r * 17, g * 17, b * 17);
        }
    } else if s.len() == 6 {
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
    let rows_count = (max_row + 1).max(cfg.settings.rows_count).max(1);
    let eff_bar_h = cfg.settings.bar_height.max(cfg.settings.icon_size + 14.0);
    (eff_bar_h as i32) * (rows_count as i32)
}

fn get_max_allowed_rows(cfg: &AppConfig) -> usize {
    #[cfg(windows)]
    let (_, _, _, work_h) = win32_utils::win32::get_work_area();
    #[cfg(not(windows))]
    let work_h = 1080;

    let eff_bar_h = cfg.settings.bar_height.max(cfg.settings.icon_size + 14.0);
    let bar_h = (eff_bar_h as i32).max(16);
    let max_by_screen = ((work_h / bar_h) as usize).max(1);
    let max_in_cfg = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0) + 1;
    let max_explicit = cfg.settings.rows_count;
    max_by_screen.max(max_in_cfg).max(max_explicit).max(1)
}

fn build_available_rows_list(cfg: &AppConfig) -> Vec<SharedString> {
    let max_row = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0);
    let rows_count = (max_row + 1).max(cfg.settings.rows_count).max(1);
    let mut rows = Vec::with_capacity(rows_count);
    for r in 0..rows_count {
        if r == 0 {
            rows.push("Ligne 1 (Haut)".into());
        } else {
            rows.push(format!("Ligne {}", r + 1).into());
        }
    }
    rows
}

fn build_lines_summary_list(cfg: &AppConfig) -> Vec<LineSummaryData> {
    let max_row = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0);
    let rows_count = (max_row + 1).max(cfg.settings.rows_count).max(1);
    let mut lines = Vec::with_capacity(rows_count);
    for r in 0..rows_count {
        let name = if r == 0 {
            "Ligne 1 (Haut)".into()
        } else {
            format!("Ligne {}", r + 1).into()
        };
        let count = cfg.containers.iter().filter(|c| c.row == r).count() as i32;
        lines.push(LineSummaryData {
            index: r as i32,
            name,
            containers_count: count,
        });
    }
    lines
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
    bar.set_icon_sz(cfg.settings.icon_size);
    bar.set_container_font_sz(cfg.settings.container_font_size);
    bar.set_item_font_sz(cfg.settings.item_font_size);

    let bar_bg = parse_hex_color(&cfg.settings.bar_bg_color, Color::from_argb_u8(255, 15, 23, 42));
    let bar_text = parse_hex_color(&cfg.settings.bar_text_color, Color::from_argb_u8(255, 248, 250, 252));
    let hover_col = parse_hex_color(&cfg.settings.dropdown_hover_color, Color::from_argb_u8(255, 37, 99, 235));
    bar.set_bar_bg_color(bar_bg);
    bar.set_bar_text_color(bar_text);
    bar.set_dropdown_hover_color(hover_col);
    bar.set_dropdown_hover_style(cfg.settings.dropdown_hover_style.clone().into());

    let cache = cache_dir();
    let max_row = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0);
    let rows_count = (max_row + 1).max(cfg.settings.rows_count).max(1);
    bar.set_rows_count(rows_count as i32);

    let mut rows_data: Vec<BarRowData> = Vec::new();

    for r in 0..rows_count {
        let mut row_containers: Vec<BarContainerData> = Vec::new();

        for (flat_idx, cont) in cfg.containers.iter().enumerate() {
            if cont.row == r {
                let num_cols = cont.columns_count.clamp(1, 10);
                let mut col_buckets: Vec<Vec<BarItemData>> = vec![Vec::new(); num_cols];
                let mut items_data: Vec<BarItemData> = Vec::new();

                for itm in &cont.items {
                    let mut icon_img = slint::Image::default();
                    let mut icon_type = itm.icon_type.clone();

                    if icon_type == "extracted" {
                        let path_to_extract = if !itm.icon_value.is_empty() {
                            &itm.icon_value
                        } else {
                            &itm.target
                        };
                        let icon_path = win32_utils::win32::extract_and_cache_icon(path_to_extract, &cache);
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

                    let b_item = BarItemData {
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
                    };

                    let c = itm.column.min(num_cols - 1);
                    col_buckets[c].push(b_item.clone());
                    items_data.push(b_item);
                }

                let max_items_in_col = col_buckets.iter().map(|b| b.len()).max().unwrap_or(0).max(1);
                let mut bar_columns: Vec<BarColumnData> = Vec::new();
                for (c_i, bucket) in col_buckets.into_iter().enumerate() {
                    bar_columns.push(BarColumnData {
                        col_idx: c_i as i32,
                        items: ModelRc::new(VecModel::from(bucket)),
                    });
                }

                let cont_hk_str = format_hotkey_display(&cont.hotkey_modifiers, &cont.hotkey_key);
                let cont_bg = parse_hex_color(&cont.bg_color, Color::from_argb_u8(0, 0, 0, 0));
                let cont_text = parse_hex_color(&cont.text_color, Color::from_argb_u8(0, 0, 0, 0));

                let mut cont_icon_img = slint::Image::default();
                let mut cont_icon_type = cont.icon_type.clone();
                if cont_icon_type == "extracted" {
                    let path_to_extract = if !cont.icon_path.is_empty() {
                        &cont.icon_path
                    } else {
                        &cont.icon
                    };
                    if let Some(p) = win32_utils::win32::extract_and_cache_icon(path_to_extract, &cache) {
                        if let Ok(img) = slint::Image::load_from_path(&p) {
                            cont_icon_img = img;
                        } else {
                            cont_icon_type = "emoji".to_string();
                        }
                    } else {
                        cont_icon_type = "emoji".to_string();
                    }
                }

                row_containers.push(BarContainerData {
                    flat_idx: flat_idx as i32,
                    id: cont.id.clone().into(),
                    name: cont.name.clone().into(),
                    icon: cont.icon.clone().into(),
                    icon_type: cont_icon_type.into(),
                    icon_image: cont_icon_img,
                    width_val: cont.width,
                    display_mode: cont.display_mode.clone().into(),
                    bg_color: cont_bg,
                    text_color: cont_text,
                    columns_count: num_cols as i32,
                    max_items_in_col: max_items_in_col as i32,
                    columns: ModelRc::new(VecModel::from(bar_columns)),
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

        refresh_bar_ui(bar, &cfg);
        if !is_expanded {
            bar.set_active_dropdown_idx(-1);
        }

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
fn show_bar_window(bar: &BarWindow, is_expanded: bool) {
    let cfg = load_config();
    refresh_bar_ui(bar, &cfg);
    if !is_expanded {
        bar.set_active_dropdown_idx(-1);
    }
    bar.window().request_redraw();
}

// Helper to refresh SettingsWindow UI models
fn refresh_settings_ui(settings_win: &SettingsWindow, cfg: &AppConfig, selected_cont_idx: usize) {
    let available_rows = build_available_rows_list(cfg);
    settings_win.set_available_rows(ModelRc::new(VecModel::from(available_rows)));

    let lines_summary = build_lines_summary_list(cfg);
    settings_win.set_lines_list(ModelRc::new(VecModel::from(lines_summary)));

    let mut cont_summaries: Vec<ContainerItemSummary> = Vec::new();
    let mut cont_names: Vec<SharedString> = Vec::new();

    let cache = cache_dir();

    for cont in &cfg.containers {
        cont_names.push(cont.name.clone().into());
        let hk = format_hotkey_display(&cont.hotkey_modifiers, &cont.hotkey_key);
        let mut cont_img = slint::Image::default();
        if cont.icon_type == "extracted" {
            let path_to_extract = if !cont.icon_path.is_empty() { &cont.icon_path } else { &cont.icon };
            if let Some(p) = win32_utils::win32::extract_and_cache_icon(path_to_extract, &cache) {
                if let Ok(img) = slint::Image::load_from_path(&p) {
                    cont_img = img;
                }
            }
        }
        cont_summaries.push(ContainerItemSummary {
            id: cont.id.clone().into(),
            name: cont.name.clone().into(),
            icon: cont.icon.clone().into(),
            icon_type: cont.icon_type.clone().into(),
            icon_image: cont_img,
            width_val: cont.width,
            display_mode: cont.display_mode.clone().into(),
            bg_color: cont.bg_color.clone().into(),
            text_color: cont.text_color.clone().into(),
            items_count: cont.items.len() as i32,
            hotkey_display: hk.into(),
            cont_row: cont.row as i32,
            columns_count: cont.columns_count.clamp(1, 10) as i32,
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
        if settings_win.get_edit_container_name().as_str() != cont.name {
            settings_win.set_edit_container_name(cont.name.clone().into());
        }
        if settings_win.get_edit_container_icon().as_str() != cont.icon {
            settings_win.set_edit_container_icon(cont.icon.clone().into());
        }
        settings_win.set_edit_container_icon_type(cont.icon_type.clone().into());
        let mut cont_edit_img = slint::Image::default();
        if cont.icon_type == "extracted" {
            let p_ext = if !cont.icon_path.is_empty() { &cont.icon_path } else { &cont.icon };
            if let Some(p) = win32_utils::win32::extract_and_cache_icon(p_ext, &cache) {
                if let Ok(img) = slint::Image::load_from_path(&p) {
                    cont_edit_img = img;
                }
            }
        }
        settings_win.set_edit_container_icon_image(cont_edit_img);
        settings_win.set_edit_container_width(cont.width);
        settings_win.set_edit_container_display_mode(cont.display_mode.clone().into());
        if settings_win.get_edit_container_bg().as_str() != cont.bg_color {
            settings_win.set_edit_container_bg(cont.bg_color.clone().into());
        }
        if settings_win.get_edit_container_text().as_str() != cont.text_color {
            settings_win.set_edit_container_text(cont.text_color.clone().into());
        }
        let c_bg_col = parse_hex_color(&cont.bg_color, Color::from_argb_u8(255, 30, 41, 59));
        let c_txt_col = parse_hex_color(&cont.text_color, Color::from_argb_u8(255, 248, 250, 252));
        settings_win.set_edit_container_bg_col(c_bg_col);
        settings_win.set_edit_container_text_col(c_txt_col);
        settings_win.set_edit_container_row(cont.row as i32);

        let num_cols = cont.columns_count.clamp(1, 10);
        settings_win.set_edit_container_columns((num_cols - 1) as i32);

        let available_cols: Vec<SharedString> = (1..=num_cols)
            .map(|c| format!("Colonne {}", c).into())
            .collect();
        settings_win.set_available_item_columns(ModelRc::new(VecModel::from(available_cols)));

        settings_win.set_edit_cont_mod_ctrl(cont.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("control") || m.eq_ignore_ascii_case("ctrl")));
        settings_win.set_edit_cont_mod_alt(cont.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("alt")));
        settings_win.set_edit_cont_mod_shift(cont.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("shift")));
        settings_win.set_edit_cont_mod_win(cont.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("windows")));
        settings_win.set_edit_cont_hotkey_key(cont.hotkey_key.clone().into());

        let mut items_detail: Vec<ItemDetailData> = Vec::new();
        for itm in &cont.items {
            let hk = format_hotkey_display(&itm.hotkey_modifiers, &itm.hotkey_key);
            let col = itm.column.min(num_cols - 1);
            let mut itm_img = slint::Image::default();
            if itm.icon_type == "extracted" {
                let p_extract = if !itm.icon_value.is_empty() { &itm.icon_value } else { &itm.target };
                if let Some(p) = win32_utils::win32::extract_and_cache_icon(p_extract, &cache) {
                    if let Ok(img) = slint::Image::load_from_path(&p) {
                        itm_img = img;
                    }
                }
            }
            items_detail.push(ItemDetailData {
                id: itm.id.clone().into(),
                name: itm.name.clone().into(),
                target: itm.target.clone().into(),
                icon_type: itm.icon_type.clone().into(),
                icon_value: if itm.icon_type == "extracted" { "🖼️".into() } else { itm.icon_value.clone().into() },
                icon_image: itm_img,
                container_id: cont.id.clone().into(),
                bg_color: itm.bg_color.clone().into(),
                text_color: itm.text_color.clone().into(),
                hotkey_display: hk.into(),
                col_idx: col as i32,
            });
        }
        settings_win.set_items_list(ModelRc::new(VecModel::from(items_detail)));

        let current_item_idx = settings_win.get_selected_item_index();
        let safe_item_idx = if cont.items.is_empty() {
            -1
        } else if current_item_idx >= 0 && (current_item_idx as usize) < cont.items.len() {
            current_item_idx
        } else {
            0
        };
        settings_win.set_selected_item_index(safe_item_idx);

        if safe_item_idx >= 0 {
            if let Some(itm) = cont.items.get(safe_item_idx as usize) {
                if settings_win.get_item_edit_name().as_str() != itm.name {
                    settings_win.set_item_edit_name(itm.name.clone().into());
                }
                if settings_win.get_item_edit_target().as_str() != itm.target {
                    settings_win.set_item_edit_target(itm.target.clone().into());
                }
                settings_win.set_item_edit_icon_type(itm.icon_type.clone().into());
                if settings_win.get_item_edit_icon_value().as_str() != itm.icon_value {
                    settings_win.set_item_edit_icon_value(itm.icon_value.clone().into());
                }
                let mut itm_edit_img = slint::Image::default();
                if itm.icon_type == "extracted" {
                    let p_extract = if !itm.icon_value.is_empty() { &itm.icon_value } else { &itm.target };
                    if let Some(p) = win32_utils::win32::extract_and_cache_icon(p_extract, &cache) {
                        if let Ok(img) = slint::Image::load_from_path(&p) {
                            itm_edit_img = img;
                        }
                    }
                }
                settings_win.set_item_edit_icon_image(itm_edit_img);
                if settings_win.get_item_edit_bg().as_str() != itm.bg_color {
                    settings_win.set_item_edit_bg(itm.bg_color.clone().into());
                }
                if settings_win.get_item_edit_text().as_str() != itm.text_color {
                    settings_win.set_item_edit_text(itm.text_color.clone().into());
                }
                let i_bg_col = parse_hex_color(&itm.bg_color, Color::from_argb_u8(255, 30, 41, 59));
                let i_txt_col = parse_hex_color(&itm.text_color, Color::from_argb_u8(255, 248, 250, 252));
                settings_win.set_item_edit_bg_col(i_bg_col);
                settings_win.set_item_edit_text_col(i_txt_col);
                settings_win.set_item_target_container_idx(safe_idx as i32);
                settings_win.set_item_edit_column(itm.column.min(num_cols - 1) as i32);
                settings_win.set_item_edit_mod_ctrl(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("control") || m.eq_ignore_ascii_case("ctrl")));
                settings_win.set_item_edit_mod_alt(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("alt")));
                settings_win.set_item_edit_mod_shift(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("shift")));
                settings_win.set_item_edit_mod_win(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("windows")));
                settings_win.set_item_edit_hotkey_key(itm.hotkey_key.clone().into());
            }
        } else {
            settings_win.set_item_edit_name("".into());
            settings_win.set_item_edit_target("".into());
            settings_win.set_item_edit_icon_type("emoji".into());
            settings_win.set_item_edit_icon_value("🚀".into());
            settings_win.set_item_edit_icon_image(slint::Image::default());
            settings_win.set_item_edit_bg("".into());
            settings_win.set_item_edit_text("".into());
            settings_win.set_item_edit_bg_col(Color::from_argb_u8(255, 30, 41, 59));
            settings_win.set_item_edit_text_col(Color::from_argb_u8(255, 248, 250, 252));
            settings_win.set_item_target_container_idx(safe_idx as i32);
            settings_win.set_item_edit_column(0);
            settings_win.set_item_edit_mod_ctrl(false);
            settings_win.set_item_edit_mod_alt(false);
            settings_win.set_item_edit_mod_shift(false);
            settings_win.set_item_edit_mod_win(false);
            settings_win.set_item_edit_hotkey_key("".into());
        }
    } else {
        settings_win.set_items_list(ModelRc::new(VecModel::from(Vec::<ItemDetailData>::new())));
    }

    // Apparence & Système
    settings_win.set_pref_position(cfg.settings.bar_position.clone().into());
    settings_win.set_pref_containers_align(cfg.settings.containers_alignment.clone().into());
    settings_win.set_pref_bar_h(cfg.settings.bar_height);
    settings_win.set_pref_item_h(cfg.settings.item_height);
    settings_win.set_pref_icon_sz(cfg.settings.icon_size);
    settings_win.set_pref_cont_font(cfg.settings.container_font_size);
    settings_win.set_pref_item_font(cfg.settings.item_font_size);
    settings_win.set_pref_bar_bg_color(cfg.settings.bar_bg_color.clone().into());
    settings_win.set_pref_bar_text_color(cfg.settings.bar_text_color.clone().into());
    let bar_bg_col = parse_hex_color(&cfg.settings.bar_bg_color, Color::from_argb_u8(255, 15, 23, 42));
    let bar_text_col = parse_hex_color(&cfg.settings.bar_text_color, Color::from_argb_u8(255, 248, 250, 252));
    settings_win.set_pref_bar_bg_color_val(bar_bg_col);
    settings_win.set_pref_bar_text_color_val(bar_text_col);
    let hover_col = parse_hex_color(&cfg.settings.dropdown_hover_color, Color::from_argb_u8(255, 37, 99, 235));
    settings_win.set_pref_dropdown_hover_color(cfg.settings.dropdown_hover_color.clone().into());
    settings_win.set_pref_dropdown_hover_color_val(hover_col);
    settings_win.set_pref_dropdown_hover_style(cfg.settings.dropdown_hover_style.clone().into());
    settings_win.set_pref_mod_ctrl(cfg.settings.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("control") || m.eq_ignore_ascii_case("ctrl")));
    settings_win.set_pref_mod_alt(cfg.settings.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("alt")));
    settings_win.set_pref_mod_shift(cfg.settings.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("shift")));
    settings_win.set_pref_mod_win(cfg.settings.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("windows")));
    settings_win.set_pref_hotkey_key(cfg.settings.hotkey_key.clone().into());
    settings_win.set_pref_autostart(cfg.settings.autostart);
    settings_win.set_pref_stay_on_top(cfg.settings.stay_on_top);
}

fn apply_item_editor_to_config(sui: &SettingsWindow, cfg: &mut AppConfig, c_idx: usize) {
    let item_idx = sui.get_selected_item_index();
    let name = sui.get_item_edit_name().to_string();
    let target = sui.get_item_edit_target().to_string();
    let mut icon_type = sui.get_item_edit_icon_type().to_string();
    let mut icon_value = sui.get_item_edit_icon_value().to_string();
    let bg_color = sui.get_item_edit_bg().to_string();
    let text_color = sui.get_item_edit_text().to_string();
    let target_cont_raw = sui.get_item_target_container_idx();

    // Si l'icône est par défaut ou extraite et qu'une cible est renseignée, s'assurer de l'extraction
    if (icon_type == "extracted" || icon_value.is_empty() || icon_value == "🚀") && !target.trim().is_empty() {
        let cache = cache_dir();
        let check_path = if icon_type == "extracted" && !icon_value.is_empty() {
            &icon_value
        } else {
            &target
        };
        if let Some(cached_icon) = win32_utils::win32::extract_and_cache_icon(check_path, &cache) {
            if let Ok(img) = slint::Image::load_from_path(&cached_icon) {
                icon_type = "extracted".to_string();
                sui.set_item_edit_icon_type("extracted".into());
                sui.set_item_edit_icon_image(img);
                if icon_value.is_empty() || icon_value == "🚀" {
                    icon_value = target.clone();
                    sui.set_item_edit_icon_value(icon_value.clone().into());
                }
            }
        }
    }

    let mut mods = Vec::new();
    if sui.get_item_edit_mod_ctrl() { mods.push("Control".to_string()); }
    if sui.get_item_edit_mod_alt() { mods.push("Alt".to_string()); }
    if sui.get_item_edit_mod_shift() { mods.push("Shift".to_string()); }
    if sui.get_item_edit_mod_win() { mods.push("Win".to_string()); }
    let hotkey_key = sui.get_item_edit_hotkey_key().to_string();

    if !name.trim().is_empty() {
        let target_cont_idx = if target_cont_raw >= 0 && (target_cont_raw as usize) < cfg.containers.len() {
            target_cont_raw as usize
        } else {
            c_idx
        };

        let max_cols = cfg.containers.get(target_cont_idx).map(|c| c.columns_count.clamp(1, 10)).unwrap_or(1);
        let col = (sui.get_item_edit_column() as usize).min(max_cols - 1);

        if target_cont_idx != c_idx && c_idx < cfg.containers.len() && item_idx >= 0 && (item_idx as usize) < cfg.containers[c_idx].items.len() {
            let mut itm = cfg.containers[c_idx].items.remove(item_idx as usize);
            itm.name = name;
            itm.target = target;
            itm.icon_type = icon_type;
            itm.icon_value = icon_value;
            itm.bg_color = bg_color;
            itm.text_color = text_color;
            itm.hotkey_modifiers = mods;
            itm.hotkey_key = hotkey_key;
            itm.column = col;
            cfg.containers[target_cont_idx].items.push(itm);
        } else if let Some(cont) = cfg.containers.get_mut(target_cont_idx) {
            if item_idx >= 0 && (item_idx as usize) < cont.items.len() && target_cont_idx == c_idx {
                let itm = &mut cont.items[item_idx as usize];
                itm.name = name;
                itm.target = target;
                itm.icon_type = icon_type;
                itm.icon_value = icon_value;
                itm.bg_color = bg_color;
                itm.text_color = text_color;
                itm.hotkey_modifiers = mods;
                itm.hotkey_key = hotkey_key;
                itm.column = col;
            } else if !target.trim().is_empty() {
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
                    column: col,
                });
            }
        }
    }
}

fn add_dropped_file_to_container(file_path: &str, target_cont_idx: usize, config_arc: &Arc<Mutex<AppConfig>>) {
    let p = std::path::Path::new(file_path);
    let mut stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Nouvel Item".to_string());
    if stem.is_empty() {
        stem = file_path.to_string();
    }
    
    // Si le fichier glissé est un raccourci .lnk, résoudre la véritable cible du programme et ses arguments
    let (real_target, real_args) = if let Some((target_path, args)) = win32_utils::win32::resolve_lnk_target(file_path) {
        (target_path.to_string_lossy().to_string(), args)
    } else {
        (file_path.to_string(), String::new())
    };

    let cache = cache_dir();
    let icon_source = if Path::new(&real_target).exists() { real_target.as_str() } else { file_path };
    let has_icon = win32_utils::win32::extract_and_cache_icon(icon_source, &cache).is_some()
        || win32_utils::win32::extract_and_cache_icon(file_path, &cache).is_some();
    let icon_type = if has_icon { "extracted".to_string() } else { "emoji".to_string() };
    let icon_val = if has_icon { icon_source.to_string() } else {
        if p.is_dir() { "📁".to_string() } else { "🚀".to_string() }
    };

    let new_item = LauncherItem {
        id: generate_id(),
        name: stem,
        target: real_target,
        icon_type,
        icon_value: icon_val,
        args: real_args,
        bg_color: "".to_string(),
        text_color: "".to_string(),
        hotkey_modifiers: Vec::new(),
        hotkey_key: String::new(),
        column: 0,
    };

    let mut cfg = config_arc.lock().unwrap();
    if cfg.containers.is_empty() {
        cfg.containers.push(ContainerConfig {
            id: generate_id(),
            name: "Raccourcis".to_string(),
            icon: "📌".to_string(),
            icon_type: "emoji".to_string(),
            icon_path: String::new(),
            width: 0.0,
            order: 0,
            display_mode: "Both".to_string(),
            bg_color: "".to_string(),
            text_color: "".to_string(),
            hotkey_modifiers: Vec::new(),
            hotkey_key: String::new(),
            row: 0,
            columns_count: 1,
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

    let eff_bar_h = cfg.settings.bar_height.max(cfg.settings.icon_size + 14.0).max(20.0);
    let target_row = (y.max(0) as f32 / eff_bar_h) as usize;

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
        let mut text_w = 0.0;
        if cont.display_mode != "IconOnly" {
            text_w += cont.name.chars().count() as f32 * (font_sz * 0.72);
        }
        let mut icon_w = 0.0;
        if cont.display_mode != "NameOnly" {
            icon_w += cfg.settings.icon_size.max(9.0) + 4.0;
        }
        let pad_w = (cfg.settings.icon_size * 0.25).max(10.0) * 2.0;
        let spacing_w = (cfg.settings.icon_size * 0.22).max(6.0) * 2.0;
        let arrow_w = 12.0;
        let min_needed_w = (pad_w + icon_w + text_w + spacing_w + arrow_w).max(45.0);

        let w = if cont.width > 0.0 {
            cont.width.max(min_needed_w)
        } else {
            min_needed_w
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
                            let _ = win32_utils::win32::open_path_or_url(&target);
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
            let _ = win32_utils::win32::open_path_or_url(&target_str);
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
    }

    // ================= SURVEILLANCE DU CURSEUR (FERMETURE AUTOMATIQUE SI SORTIE DE LA FENETRE) =================
    {
        let bw = bar_window.as_weak();
        let mouse_cfg = app_config.clone();
        let mouse_timer = slint::Timer::default();
        mouse_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(50), move || {
            if let Some(bui) = bw.upgrade() {
                if bui.get_active_dropdown_idx() >= 0 {
                    #[cfg(windows)]
                    {
                        use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
                        use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};
                        let hwnd = win32_utils::win32::BAR_HWND.load(Ordering::SeqCst) as HWND;
                        if !hwnd.is_null() {
                            let mut pt = POINT { x: 0, y: 0 };
                            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                            unsafe {
                                if GetCursorPos(&mut pt) != 0 && GetWindowRect(hwnd, &mut rect) != 0 {
                                    if pt.x < rect.left || pt.x >= rect.right || pt.y < rect.top || pt.y >= rect.bottom {
                                        bui.set_active_dropdown_idx(-1);
                                        let cfg = mouse_cfg.lock().unwrap();
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
                                }
                            }
                        }
                    }
                }
            }
        });
        std::mem::forget(mouse_timer);
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
    // 0. Callbacks Lignes
    {
        let settings_weak = settings_window.as_weak();
        settings_window.on_select_line(move |line_idx| {
            if let Some(sui) = settings_weak.upgrade() {
                sui.set_selected_line_idx(line_idx);
            }
        });
    }

    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_add_line(move || {
            if let Some(sui) = settings_weak.upgrade() {
                let mut cfg = cfg_arc.lock().unwrap();
                let max_row = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0);
                let current_rows = (max_row + 1).max(cfg.settings.rows_count).max(1);
                let new_row_count = current_rows + 1;
                cfg.settings.rows_count = new_row_count;
                save_config(&cfg);
                sui.set_selected_line_idx((new_row_count - 1) as i32);
                let current_sel = sel_idx.load(Ordering::SeqCst);
                refresh_settings_ui(&sui, &cfg, current_sel);
                if let Some(bui) = bar_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
                update_bar_window_geometry(&cfg);
            }
        });
    }

    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_delete_line(move |line_idx| {
            let del_row = line_idx as usize;
            if let Some(sui) = settings_weak.upgrade() {
                let mut cfg = cfg_arc.lock().unwrap();
                let max_row = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0);
                let current_rows = (max_row + 1).max(cfg.settings.rows_count).max(1);
                if current_rows <= 1 {
                    return;
                }
                let fallback_row = if del_row > 0 { del_row - 1 } else { 0 };
                for cont in &mut cfg.containers {
                    if cont.row == del_row {
                        cont.row = fallback_row;
                    } else if cont.row > del_row {
                        cont.row -= 1;
                    }
                }
                cfg.settings.rows_count = current_rows - 1;
                save_config(&cfg);
                sui.set_selected_line_idx(fallback_row as i32);
                let current_sel = sel_idx.load(Ordering::SeqCst);
                refresh_settings_ui(&sui, &cfg, current_sel);
                if let Some(bui) = bar_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
                update_bar_window_geometry(&cfg);
            }
        });
    }

    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_move_line_up(move |line_idx| {
            let row = line_idx as usize;
            if row == 0 {
                return;
            }
            if let Some(sui) = settings_weak.upgrade() {
                let mut cfg = cfg_arc.lock().unwrap();
                for cont in &mut cfg.containers {
                    if cont.row == row {
                        cont.row = row - 1;
                    } else if cont.row == row - 1 {
                        cont.row = row;
                    }
                }
                save_config(&cfg);
                sui.set_selected_line_idx((row - 1) as i32);
                let current_sel = sel_idx.load(Ordering::SeqCst);
                refresh_settings_ui(&sui, &cfg, current_sel);
                if let Some(bui) = bar_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
                update_bar_window_geometry(&cfg);
            }
        });
    }

    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_move_line_down(move |line_idx| {
            let row = line_idx as usize;
            if let Some(sui) = settings_weak.upgrade() {
                let mut cfg = cfg_arc.lock().unwrap();
                let max_row = cfg.containers.iter().map(|c| c.row).max().unwrap_or(0);
                let current_rows = (max_row + 1).max(cfg.settings.rows_count).max(1);
                if row + 1 >= current_rows {
                    return;
                }
                for cont in &mut cfg.containers {
                    if cont.row == row {
                        cont.row = row + 1;
                    } else if cont.row == row + 1 {
                        cont.row = row;
                    }
                }
                save_config(&cfg);
                sui.set_selected_line_idx((row + 1) as i32);
                let current_sel = sel_idx.load(Ordering::SeqCst);
                refresh_settings_ui(&sui, &cfg, current_sel);
                if let Some(bui) = bar_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
                update_bar_window_geometry(&cfg);
            }
        });
    }

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
                let sel_line = sui.get_selected_line_idx();
                let target_row = if sel_line >= 0 { sel_line as usize } else { 0 };
                let new_cont = ContainerConfig {
                    id: generate_id(),
                    name: format!("Nouveau {}", cfg.containers.len() + 1),
                    icon: "📦".to_string(),
                    icon_type: "emoji".to_string(),
                    icon_path: String::new(),
                    width: 0.0,
                    order: cfg.containers.len(),
                    display_mode: "Both".to_string(),
                    bg_color: "".to_string(),
                    text_color: "".to_string(),
                    hotkey_modifiers: Vec::new(),
                    hotkey_key: String::new(),
                    row: target_row,
                    columns_count: 1,
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
                    cont.icon_type = sui.get_edit_container_icon_type().to_string();
                    cont.width = sui.get_edit_container_width();
                    cont.display_mode = sui.get_edit_container_display_mode().to_string();
                    cont.bg_color = sui.get_edit_container_bg().to_string();
                    cont.text_color = sui.get_edit_container_text().to_string();
                    cont.row = sui.get_edit_container_row() as usize;
                    let new_cols = (sui.get_edit_container_columns() as usize + 1).clamp(1, 10);
                    cont.columns_count = new_cols;
                    for itm in &mut cont.items {
                        if itm.column >= new_cols {
                            itm.column = new_cols - 1;
                        }
                    }

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

    // 5bis. Parcourir fichier pour extraire l'icône d'un conteneur
    {
        let settings_weak = settings_window.as_weak();
        let bar_weak = bar_window.as_weak();
        let cfg_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();

        settings_window.on_browse_container_icon_file(move || {
            if let Some(sui) = settings_weak.upgrade() {
                if let Some(file) = win32_utils::win32::pick_file_dialog(
                    Some("Sélectionner une icône"),
                    Some("Exécutables & Icônes (*.exe, *.lnk, *.ico, *.dll)"),
                    Some(&["exe", "lnk", "ico", "dll"]),
                ) {
                    let path_str = file.to_string_lossy().to_string();
                    let resolved_icon = if let Some((target_path, _)) = win32_utils::win32::resolve_lnk_target(&path_str) {
                        target_path.to_string_lossy().to_string()
                    } else {
                        path_str.clone()
                    };
                    let icon_source = if Path::new(&resolved_icon).exists() { &resolved_icon } else { &path_str };
                    let cache = cache_dir();
                    if let Some(cached_icon) = win32_utils::win32::extract_and_cache_icon(icon_source, &cache) {
                        if let Ok(img) = slint::Image::load_from_path(&cached_icon) {
                            sui.set_edit_container_icon_type("extracted".into());
                            sui.set_edit_container_icon_image(img);
                            let idx = sel_idx.load(Ordering::SeqCst);
                            let mut cfg = cfg_arc.lock().unwrap();
                            if let Some(cont) = cfg.containers.get_mut(idx) {
                                cont.icon_type = "extracted".to_string();
                                cont.icon_path = icon_source.clone();
                                save_config(&cfg);
                                if let Some(bui) = bar_weak.upgrade() {
                                    refresh_bar_ui(&bui, &cfg);
                                }
                            }
                            refresh_settings_ui(&sui, &cfg, idx);
                        }
                    }
                }
            }
        });
    }

    // 5ter. Ouvrir le site web Emojipedia pour choisir et copier des émojis
    {
        settings_window.on_open_emoji_website(move || {
            let _ = win32_utils::win32::open_path_or_url("https://emojipedia.org/");
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
                sui.set_item_edit_bg_col(Color::from_argb_u8(255, 30, 41, 59));
                sui.set_item_edit_text_col(Color::from_argb_u8(255, 248, 250, 252));
                sui.set_item_edit_column(0);
                sui.set_item_edit_mod_ctrl(false);
                sui.set_item_edit_mod_alt(false);
                sui.set_item_edit_mod_shift(false);
                sui.set_item_edit_mod_win(false);
                sui.set_item_edit_hotkey_key("".into());
                sui.set_show_item_editor(false);
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
                        let i_bg = parse_hex_color(&itm.bg_color, Color::from_argb_u8(255, 30, 41, 59));
                        let i_txt = parse_hex_color(&itm.text_color, Color::from_argb_u8(255, 248, 250, 252));
                        sui.set_item_edit_bg_col(i_bg);
                        sui.set_item_edit_text_col(i_txt);
                        sui.set_item_target_container_idx(c_idx as i32);
                        sui.set_item_edit_column(itm.column.min(cont.columns_count.saturating_sub(1)) as i32);
                        sui.set_item_edit_mod_ctrl(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("control") || m.eq_ignore_ascii_case("ctrl")));
                        sui.set_item_edit_mod_alt(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("alt")));
                        sui.set_item_edit_mod_shift(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("shift")));
                        sui.set_item_edit_mod_win(itm.hotkey_modifiers.iter().any(|m| m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("windows")));
                        sui.set_item_edit_hotkey_key(itm.hotkey_key.clone().into());
                        let mut itm_edit_img = slint::Image::default();
                        if itm.icon_type == "extracted" {
                            let cache = cache_dir();
                            let p_extract = if !itm.icon_value.is_empty() { &itm.icon_value } else { &itm.target };
                            if let Some(p) = win32_utils::win32::extract_and_cache_icon(p_extract, &cache) {
                                if let Ok(img) = slint::Image::load_from_path(&p) {
                                    itm_edit_img = img;
                                }
                            }
                        }
                        sui.set_item_edit_icon_image(itm_edit_img);
                        sui.set_show_item_editor(false);
                    }
                }
            }
        });
    }

    // 7. Parcourir fichier pour cible d'item (avec extraction automatique d'icône)
    {
        let settings_weak = settings_window.as_weak();
        settings_window.on_browse_item_target(move || {
            if let Some(sui) = settings_weak.upgrade() {
                if let Some(file) = win32_utils::win32::pick_file_dialog(
                    Some("Sélectionner une application ou un fichier"),
                    None,
                    None,
                ) {
                    let path_str = file.to_string_lossy().to_string();
                    let full_target = if let Some((t, a)) = win32_utils::win32::resolve_lnk_target(&path_str) {
                        if a.trim().is_empty() {
                            t.to_string_lossy().to_string()
                        } else {
                            format!("\"{}\" {}", t.display(), a.trim())
                        }
                    } else {
                        path_str.clone()
                    };
                    sui.set_item_edit_target(full_target.into());
                    if sui.get_item_edit_name().is_empty() {
                        if let Some(stem) = file.file_stem() {
                            sui.set_item_edit_name(stem.to_string_lossy().to_string().into());
                        }
                    }
                    // Extraction automatique de l'icône de l'application visée
                    let cache = cache_dir();
                    let raw_target = if let Some((t, _)) = win32_utils::win32::resolve_lnk_target(&path_str) {
                        t.to_string_lossy().to_string()
                    } else {
                        path_str.clone()
                    };
                    let icon_source = if Path::new(&raw_target).exists() { &raw_target } else { &path_str };
                    if let Some(cached_icon) = win32_utils::win32::extract_and_cache_icon(icon_source, &cache) {
                        if let Ok(img) = slint::Image::load_from_path(&cached_icon) {
                            sui.set_item_edit_icon_type("extracted".into());
                            sui.set_item_edit_icon_image(img);
                            sui.set_item_edit_icon_value(icon_source.clone().into());
                        }
                    }
                }
            }
        });
    }

    // 7bis. Parcourir un fichier pour extraire une icône spécifique pour un item
    {
        let settings_weak = settings_window.as_weak();
        settings_window.on_browse_item_icon_file(move || {
            if let Some(sui) = settings_weak.upgrade() {
                if let Some(file) = win32_utils::win32::pick_file_dialog(
                    Some("Sélectionner une icône"),
                    Some("Exécutables & Icônes (*.exe, *.lnk, *.ico, *.dll)"),
                    Some(&["exe", "lnk", "ico", "dll"]),
                ) {
                    let path_str = file.to_string_lossy().to_string();
                    let resolved_icon = if let Some((target_path, _)) = win32_utils::win32::resolve_lnk_target(&path_str) {
                        target_path.to_string_lossy().to_string()
                    } else {
                        path_str.clone()
                    };
                    let icon_source = if Path::new(&resolved_icon).exists() { &resolved_icon } else { &path_str };
                    let cache = cache_dir();
                    if let Some(cached_icon) = win32_utils::win32::extract_and_cache_icon(icon_source, &cache) {
                        if let Ok(img) = slint::Image::load_from_path(&cached_icon) {
                            sui.set_item_edit_icon_type("extracted".into());
                            sui.set_item_edit_icon_image(img);
                            sui.set_item_edit_icon_value(icon_source.clone().into());
                        }
                    }
                }
            }
        });
    }

    // 7ter. Filtrage par colonne dans la Zone 3 & 4
    {
        let settings_weak = settings_window.as_weak();
        settings_window.on_select_column_filter(move |col_idx| {
            if let Some(sui) = settings_weak.upgrade() {
                sui.set_selected_column_filter(col_idx);
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
                let mut cfg = cfg_arc.lock().unwrap();
                apply_item_editor_to_config(&sui, &mut cfg, c_idx);
                save_config(&cfg);

                #[cfg(windows)]
                {
                    use windows_sys::Win32::Foundation::HWND;
                    let tray_hwnd = win32_utils::win32::SYSTRAY_HWND.load(Ordering::SeqCst) as HWND;
                    register_all_hotkeys_for_app(tray_hwnd, &cfg);
                }

                refresh_settings_ui(&sui, &cfg, c_idx);
                if let Some(bui) = bar_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
            }
        });
    }

    // 8bis. Callbacks de pipette et de couleur (Conteneurs, Items, Bandeau)
    {
        // Conteneur Fond
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_pick_container_bg_color(move || {
            if let Some(sui) = s_weak.upgrade() {
                let cur = sui.get_edit_container_bg().to_string();
                if let Some(new_col) = win32_utils::win32::pick_color_dialog(&cur) {
                    sui.set_edit_container_bg(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 30, 41, 59));
                    sui.set_edit_container_bg_col(col);
                    let idx = sel_idx.load(Ordering::SeqCst);
                    let mut cfg = c_arc.lock().unwrap();
                    if let Some(cont) = cfg.containers.get_mut(idx) {
                        cont.bg_color = new_col;
                        save_config(&cfg);
                        if let Some(bui) = b_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });

        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_container_bg_text_changed(move || {
            if let Some(sui) = s_weak.upgrade() {
                let txt = sui.get_edit_container_bg().to_string();
                let col = parse_hex_color(&txt, Color::from_argb_u8(255, 30, 41, 59));
                sui.set_edit_container_bg_col(col);
                let idx = sel_idx.load(Ordering::SeqCst);
                let mut cfg = c_arc.lock().unwrap();
                if let Some(cont) = cfg.containers.get_mut(idx) {
                    cont.bg_color = txt;
                    save_config(&cfg);
                    if let Some(bui) = b_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                }
            }
        });

        // Conteneur Texte
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_pick_container_text_color(move || {
            if let Some(sui) = s_weak.upgrade() {
                let cur = sui.get_edit_container_text().to_string();
                if let Some(new_col) = win32_utils::win32::pick_color_dialog(&cur) {
                    sui.set_edit_container_text(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 248, 250, 252));
                    sui.set_edit_container_text_col(col);
                    let idx = sel_idx.load(Ordering::SeqCst);
                    let mut cfg = c_arc.lock().unwrap();
                    if let Some(cont) = cfg.containers.get_mut(idx) {
                        cont.text_color = new_col;
                        save_config(&cfg);
                        if let Some(bui) = b_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });

        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_container_text_changed(move || {
            if let Some(sui) = s_weak.upgrade() {
                let txt = sui.get_edit_container_text().to_string();
                let col = parse_hex_color(&txt, Color::from_argb_u8(255, 248, 250, 252));
                sui.set_edit_container_text_col(col);
                let idx = sel_idx.load(Ordering::SeqCst);
                let mut cfg = c_arc.lock().unwrap();
                if let Some(cont) = cfg.containers.get_mut(idx) {
                    cont.text_color = txt;
                    save_config(&cfg);
                    if let Some(bui) = b_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                }
            }
        });

        // Item Fond
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_pick_item_bg_color(move || {
            if let Some(sui) = s_weak.upgrade() {
                let cur = sui.get_item_edit_bg().to_string();
                if let Some(new_col) = win32_utils::win32::pick_color_dialog(&cur) {
                    sui.set_item_edit_bg(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 30, 41, 59));
                    sui.set_item_edit_bg_col(col);
                    let c_idx = sel_idx.load(Ordering::SeqCst);
                    let item_idx = sui.get_selected_item_index();
                    if item_idx >= 0 {
                        let mut cfg = c_arc.lock().unwrap();
                        if let Some(cont) = cfg.containers.get_mut(c_idx) {
                            if let Some(itm) = cont.items.get_mut(item_idx as usize) {
                                itm.bg_color = new_col;
                                save_config(&cfg);
                                if let Some(bui) = b_weak.upgrade() {
                                    refresh_bar_ui(&bui, &cfg);
                                }
                            }
                        }
                    }
                }
            }
        });

        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_item_bg_text_changed(move || {
            if let Some(sui) = s_weak.upgrade() {
                let txt = sui.get_item_edit_bg().to_string();
                let col = parse_hex_color(&txt, Color::from_argb_u8(255, 30, 41, 59));
                sui.set_item_edit_bg_col(col);
                let c_idx = sel_idx.load(Ordering::SeqCst);
                let item_idx = sui.get_selected_item_index();
                if item_idx >= 0 {
                    let mut cfg = c_arc.lock().unwrap();
                    if let Some(cont) = cfg.containers.get_mut(c_idx) {
                        if let Some(itm) = cont.items.get_mut(item_idx as usize) {
                            itm.bg_color = txt;
                            save_config(&cfg);
                            if let Some(bui) = b_weak.upgrade() {
                                refresh_bar_ui(&bui, &cfg);
                            }
                        }
                    }
                }
            }
        });

        // Item Texte
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_pick_item_text_color(move || {
            if let Some(sui) = s_weak.upgrade() {
                let cur = sui.get_item_edit_text().to_string();
                if let Some(new_col) = win32_utils::win32::pick_color_dialog(&cur) {
                    sui.set_item_edit_text(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 248, 250, 252));
                    sui.set_item_edit_text_col(col);
                    let c_idx = sel_idx.load(Ordering::SeqCst);
                    let item_idx = sui.get_selected_item_index();
                    if item_idx >= 0 {
                        let mut cfg = c_arc.lock().unwrap();
                        if let Some(cont) = cfg.containers.get_mut(c_idx) {
                            if let Some(itm) = cont.items.get_mut(item_idx as usize) {
                                itm.text_color = new_col;
                                save_config(&cfg);
                                if let Some(bui) = b_weak.upgrade() {
                                    refresh_bar_ui(&bui, &cfg);
                                }
                            }
                        }
                    }
                }
            }
        });

        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_item_text_changed(move || {
            if let Some(sui) = s_weak.upgrade() {
                let txt = sui.get_item_edit_text().to_string();
                let col = parse_hex_color(&txt, Color::from_argb_u8(255, 248, 250, 252));
                sui.set_item_edit_text_col(col);
                let c_idx = sel_idx.load(Ordering::SeqCst);
                let item_idx = sui.get_selected_item_index();
                if item_idx >= 0 {
                    let mut cfg = c_arc.lock().unwrap();
                    if let Some(cont) = cfg.containers.get_mut(c_idx) {
                        if let Some(itm) = cont.items.get_mut(item_idx as usize) {
                            itm.text_color = txt;
                            save_config(&cfg);
                            if let Some(bui) = b_weak.upgrade() {
                                refresh_bar_ui(&bui, &cfg);
                            }
                        }
                    }
                }
            }
        });

        // Bandeau Fond
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        settings_window.on_pick_bar_bg_color(move || {
            if let Some(sui) = s_weak.upgrade() {
                let cur = sui.get_pref_bar_bg_color().to_string();
                if let Some(new_col) = win32_utils::win32::pick_color_dialog(&cur) {
                    sui.set_pref_bar_bg_color(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 15, 23, 42));
                    sui.set_pref_bar_bg_color_val(col);
                    let mut cfg = c_arc.lock().unwrap();
                    cfg.settings.bar_bg_color = new_col;
                    save_config(&cfg);
                    if let Some(bui) = b_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                }
            }
        });

        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        settings_window.on_bar_bg_text_changed(move || {
            if let Some(sui) = s_weak.upgrade() {
                let txt = sui.get_pref_bar_bg_color().to_string();
                let col = parse_hex_color(&txt, Color::from_argb_u8(255, 15, 23, 42));
                sui.set_pref_bar_bg_color_val(col);
                let mut cfg = c_arc.lock().unwrap();
                cfg.settings.bar_bg_color = txt;
                save_config(&cfg);
                if let Some(bui) = b_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
            }
        });

        // Bandeau Texte
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        settings_window.on_pick_bar_text_color(move || {
            if let Some(sui) = s_weak.upgrade() {
                let cur = sui.get_pref_bar_text_color().to_string();
                if let Some(new_col) = win32_utils::win32::pick_color_dialog(&cur) {
                    sui.set_pref_bar_text_color(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 248, 250, 252));
                    sui.set_pref_bar_text_color_val(col);
                    let mut cfg = c_arc.lock().unwrap();
                    cfg.settings.bar_text_color = new_col;
                    save_config(&cfg);
                    if let Some(bui) = b_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                }
            }
        });

        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        settings_window.on_bar_text_text_changed(move || {
            if let Some(sui) = s_weak.upgrade() {
                let txt = sui.get_pref_bar_text_color().to_string();
                let col = parse_hex_color(&txt, Color::from_argb_u8(255, 248, 250, 252));
                sui.set_pref_bar_text_color_val(col);
                let mut cfg = c_arc.lock().unwrap();
                cfg.settings.bar_text_color = txt;
                save_config(&cfg);
                if let Some(bui) = b_weak.upgrade() {
                    refresh_bar_ui(&bui, &cfg);
                }
            }
        });

        // Pipette d'écran (Eyedropper) Conteneur Fond
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_eyedropper_container_bg(move || {
            if let Some(sui) = s_weak.upgrade() {
                if let Some(new_col) = win32_utils::win32::pick_color_eyedropper() {
                    sui.set_edit_container_bg(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 30, 41, 59));
                    sui.set_edit_container_bg_col(col);
                    let idx = sel_idx.load(Ordering::SeqCst);
                    let mut cfg = c_arc.lock().unwrap();
                    if let Some(cont) = cfg.containers.get_mut(idx) {
                        cont.bg_color = new_col;
                        save_config(&cfg);
                        if let Some(bui) = b_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });

        // Pipette d'écran (Eyedropper) Conteneur Texte
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_eyedropper_container_text(move || {
            if let Some(sui) = s_weak.upgrade() {
                if let Some(new_col) = win32_utils::win32::pick_color_eyedropper() {
                    sui.set_edit_container_text(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 248, 250, 252));
                    sui.set_edit_container_text_col(col);
                    let idx = sel_idx.load(Ordering::SeqCst);
                    let mut cfg = c_arc.lock().unwrap();
                    if let Some(cont) = cfg.containers.get_mut(idx) {
                        cont.text_color = new_col;
                        save_config(&cfg);
                        if let Some(bui) = b_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });

        // Pipette d'écran (Eyedropper) Item Fond
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_eyedropper_item_bg(move || {
            if let Some(sui) = s_weak.upgrade() {
                if let Some(new_col) = win32_utils::win32::pick_color_eyedropper() {
                    sui.set_item_edit_bg(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 30, 41, 59));
                    sui.set_item_edit_bg_col(col);
                    let c_idx = sel_idx.load(Ordering::SeqCst);
                    let item_idx = sui.get_selected_item_index();
                    if item_idx >= 0 {
                        let mut cfg = c_arc.lock().unwrap();
                        if let Some(cont) = cfg.containers.get_mut(c_idx) {
                            if let Some(itm) = cont.items.get_mut(item_idx as usize) {
                                itm.bg_color = new_col;
                                save_config(&cfg);
                                if let Some(bui) = b_weak.upgrade() {
                                    refresh_bar_ui(&bui, &cfg);
                                }
                            }
                        }
                    }
                }
            }
        });

        // Pipette d'écran (Eyedropper) Item Texte
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        let sel_idx = selected_container_idx.clone();
        settings_window.on_eyedropper_item_text(move || {
            if let Some(sui) = s_weak.upgrade() {
                if let Some(new_col) = win32_utils::win32::pick_color_eyedropper() {
                    sui.set_item_edit_text(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 248, 250, 252));
                    sui.set_item_edit_text_col(col);
                    let c_idx = sel_idx.load(Ordering::SeqCst);
                    let item_idx = sui.get_selected_item_index();
                    if item_idx >= 0 {
                        let mut cfg = c_arc.lock().unwrap();
                        if let Some(cont) = cfg.containers.get_mut(c_idx) {
                            if let Some(itm) = cont.items.get_mut(item_idx as usize) {
                                itm.text_color = new_col;
                                save_config(&cfg);
                                if let Some(bui) = b_weak.upgrade() {
                                    refresh_bar_ui(&bui, &cfg);
                                }
                            }
                        }
                    }
                }
            }
        });

        // Pipette d'écran (Eyedropper) Bandeau Fond
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        settings_window.on_eyedropper_bar_bg(move || {
            if let Some(sui) = s_weak.upgrade() {
                if let Some(new_col) = win32_utils::win32::pick_color_eyedropper() {
                    sui.set_pref_bar_bg_color(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 15, 23, 42));
                    sui.set_pref_bar_bg_color_val(col);
                    let mut cfg = c_arc.lock().unwrap();
                    cfg.settings.bar_bg_color = new_col;
                    save_config(&cfg);
                    if let Some(bui) = b_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
                    }
                }
            }
        });

        // Pipette d'écran (Eyedropper) Bandeau Texte
        let s_weak = settings_window.as_weak();
        let b_weak = bar_window.as_weak();
        let c_arc = app_config.clone();
        settings_window.on_eyedropper_bar_text(move || {
            if let Some(sui) = s_weak.upgrade() {
                if let Some(new_col) = win32_utils::win32::pick_color_eyedropper() {
                    sui.set_pref_bar_text_color(new_col.clone().into());
                    let col = parse_hex_color(&new_col, Color::from_argb_u8(255, 248, 250, 252));
                    sui.set_pref_bar_text_color_val(col);
                    let mut cfg = c_arc.lock().unwrap();
                    cfg.settings.bar_text_color = new_col;
                    save_config(&cfg);
                    if let Some(bui) = b_weak.upgrade() {
                        refresh_bar_ui(&bui, &cfg);
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
            let c_idx = s_idx.load(Ordering::SeqCst);
            let mut cfg = c_arc.lock().unwrap();
            if let Some(cont) = cfg.containers.get_mut(c_idx) {
                if i < cont.items.len() {
                    let cur_col = cont.items[i].column;
                    if let Some(prev_idx) = (0..i).rev().find(|&k| cont.items[k].column == cur_col) {
                        cont.items.swap(i, prev_idx);
                        save_config(&cfg);
                        if let Some(sui) = s_weak.upgrade() {
                            sui.set_selected_item_index(prev_idx as i32);
                            refresh_settings_ui(&sui, &cfg, c_idx);
                        }
                        if let Some(bui) = b_weak.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
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
                if i < cont.items.len() {
                    let cur_col = cont.items[i].column;
                    if let Some(next_idx) = ((i + 1)..cont.items.len()).find(|&k| cont.items[k].column == cur_col) {
                        cont.items.swap(i, next_idx);
                        save_config(&cfg);
                        if let Some(sui) = s_weak2.upgrade() {
                            sui.set_selected_item_index(next_idx as i32);
                            refresh_settings_ui(&sui, &cfg, c_idx);
                        }
                        if let Some(bui) = b_weak2.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
                    }
                }
            }
        });

        let s_weak3 = settings_weak.clone();
        let b_weak3 = bar_weak.clone();
        let c_arc3 = cfg_arc.clone();
        let s_idx3 = sel_idx.clone();
        settings_window.on_move_item_column(move |item_idx, delta| {
            let i = item_idx as usize;
            let c_idx = s_idx3.load(Ordering::SeqCst);
            let mut cfg = c_arc3.lock().unwrap();
            if let Some(cont) = cfg.containers.get_mut(c_idx) {
                if i < cont.items.len() {
                    let max_col = cont.columns_count.clamp(1, 10) - 1;
                    let cur_col = cont.items[i].column.min(max_col);
                    let new_col = if delta < 0 {
                        cur_col.saturating_sub(1)
                    } else {
                        (cur_col + 1).min(max_col)
                    };
                    if new_col != cur_col {
                        cont.items[i].column = new_col;
                        save_config(&cfg);
                        if let Some(sui) = s_weak3.upgrade() {
                            sui.set_item_edit_column(new_col as i32);
                            refresh_settings_ui(&sui, &cfg, c_idx);
                        }
                        if let Some(bui) = b_weak3.upgrade() {
                            refresh_bar_ui(&bui, &cfg);
                        }
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
                        let mut itm = cfg.containers[src_c].items.remove(i_idx);
                        let dst_max_cols = cfg.containers[dst_c].columns_count.clamp(1, 10);
                        itm.column = itm.column.min(dst_max_cols - 1);
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
                        let dst_max_cols = cfg.containers[dst_c].columns_count.clamp(1, 10);
                        duplicated.column = duplicated.column.min(dst_max_cols - 1);
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
                let idx = sel_idx.load(Ordering::SeqCst);
                let mut cfg = cfg_arc.lock().unwrap();

                // 1. Sauvegarder les données du conteneur en cours d'édition
                if let Some(cont) = cfg.containers.get_mut(idx) {
                    let name = sui.get_edit_container_name().to_string();
                    if !name.is_empty() {
                        cont.name = name;
                    }
                    cont.icon = sui.get_edit_container_icon().to_string();
                    cont.icon_type = sui.get_edit_container_icon_type().to_string();
                    cont.width = sui.get_edit_container_width();
                    cont.display_mode = sui.get_edit_container_display_mode().to_string();
                    cont.bg_color = sui.get_edit_container_bg().to_string();
                    cont.text_color = sui.get_edit_container_text().to_string();
                    cont.row = sui.get_edit_container_row() as usize;
                    let new_cols = (sui.get_edit_container_columns() as usize + 1).clamp(1, 10);
                    cont.columns_count = new_cols;
                    for itm in &mut cont.items {
                        if itm.column >= new_cols {
                            itm.column = new_cols - 1;
                        }
                    }

                    let mut c_mods = Vec::new();
                    if sui.get_edit_cont_mod_ctrl() { c_mods.push("Control".to_string()); }
                    if sui.get_edit_cont_mod_alt() { c_mods.push("Alt".to_string()); }
                    if sui.get_edit_cont_mod_shift() { c_mods.push("Shift".to_string()); }
                    if sui.get_edit_cont_mod_win() { c_mods.push("Win".to_string()); }
                    cont.hotkey_modifiers = c_mods;
                    cont.hotkey_key = sui.get_edit_cont_hotkey_key().to_string();
                }

                // 1bis. Sauvegarder les données de l'item/raccourci en cours d'édition (unification globale)
                apply_item_editor_to_config(&sui, &mut cfg, idx);

                // 2. Sauvegarder les préférences globales du bandeau
                cfg.settings.bar_position = sui.get_pref_position().to_string();
                cfg.settings.containers_alignment = sui.get_pref_containers_align().to_string();
                cfg.settings.icon_size = sui.get_pref_icon_sz();
                cfg.settings.bar_height = sui.get_pref_bar_h().max(cfg.settings.icon_size + 14.0);
                cfg.settings.item_height = sui.get_pref_item_h();
                cfg.settings.container_font_size = sui.get_pref_cont_font();
                cfg.settings.item_font_size = sui.get_pref_item_font();
                cfg.settings.bar_bg_color = sui.get_pref_bar_bg_color().to_string();
                cfg.settings.bar_text_color = sui.get_pref_bar_text_color().to_string();
                cfg.settings.dropdown_hover_color = sui.get_pref_dropdown_hover_color().to_string();
                cfg.settings.dropdown_hover_style = sui.get_pref_dropdown_hover_style().to_string();

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

    // ================= TIMER D'ANIMATION POUR EFFETS DE SURVOL (PULSE / ONDULATION) =================
    {
        let bw = bar_window.as_weak();
        let sw = settings_window.as_weak();
        let pulse_timer = slint::Timer::default();
        pulse_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(280), move || {
            if let Some(bui) = bw.upgrade() {
                if bui.get_active_dropdown_idx() >= 0 {
                    bui.set_anim_pulse(!bui.get_anim_pulse());
                }
            }
            if let Some(sui) = sw.upgrade() {
                sui.set_preview_anim_pulse(!sui.get_preview_anim_pulse());
                let txt = sui.get_pref_dropdown_hover_color();
                let col = parse_hex_color(&txt, Color::from_argb_u8(255, 37, 99, 235));
                sui.set_pref_dropdown_hover_color_val(col);
            }
        });
        std::mem::forget(pulse_timer);
    }

    // run_event_loop() (et non run_event_loop_until_quit()) :
    // l'event loop ne quitte QUE sur un appel explicite à slint::quit_event_loop().
    // Cela permet d'utiliser SW_HIDE librement sans risquer de terminer l'appli.
    slint::run_event_loop()?;
    Ok(())
}
