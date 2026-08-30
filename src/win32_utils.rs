#![allow(dead_code, unused_imports, unused_variables)]

use std::path::{Path, PathBuf};

#[cfg(windows)]
pub mod win32 {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Graphics::Dwm::*;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::Registry::*;
    use windows_sys::Win32::System::Threading::*;
    use windows_sys::Win32::UI::Controls::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    use windows_sys::Win32::UI::Shell::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub static SYSTRAY_HWND: AtomicUsize = AtomicUsize::new(0);
    pub static BAR_HWND: AtomicUsize = AtomicUsize::new(0);
    pub static APP_RUNNING: AtomicBool = AtomicBool::new(true);
    pub static AUTOSTART_ENABLED: AtomicBool = AtomicBool::new(false);
    pub static STAY_ON_TOP_ENABLED: AtomicBool = AtomicBool::new(false);
    pub static BAR_EXPLICITLY_HIDDEN: AtomicBool = AtomicBool::new(false);

    pub const WM_APP_TRAY: u32 = WM_APP + 1;
    pub const WM_APP_HOTKEY: u32 = WM_APP + 2;
    pub const IDM_SHOW_HIDE: usize = 1001;
    pub const IDM_SETTINGS: usize = 1002;
    pub const IDM_AUTOSTART: usize = 1003;
    pub const IDM_QUIT: usize = 1004;
    pub const MAIN_HOTKEY_ID: i32 = 9001;

    pub fn to_wide_null(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    /// Recherche fiable du HWND du bandeau avec mise en cache et énumération fallback
    pub fn find_bar_hwnd() -> HWND {
        let cached = BAR_HWND.load(Ordering::SeqCst) as HWND;
        if !cached.is_null() && unsafe { IsWindow(cached) } != 0 {
            return cached;
        }

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let mut process_id: u32 = 0;
            unsafe { GetWindowThreadProcessId(hwnd, &mut process_id); }
            if process_id == unsafe { GetCurrentProcessId() } {
                let mut title_buf = [0u16; 256];
                let len = unsafe { GetWindowTextW(hwnd, title_buf.as_mut_ptr(), 256) };
                if len > 0 {
                    let title = String::from_utf16_lossy(&title_buf[..len as usize]);
                    if title == "Lanceur Bandeau" {
                        unsafe { *(lparam as *mut HWND) = hwnd; }
                        return 0;
                    }
                }
            }
            1
        }
        let mut found_hwnd: HWND = std::ptr::null_mut();
        unsafe {
            EnumWindows(Some(enum_proc), &mut found_hwnd as *mut _ as LPARAM);
        }
        if !found_hwnd.is_null() {
            BAR_HWND.store(found_hwnd as usize, Ordering::SeqCst);
        }
        found_hwnd
    }

    /// Subclass Window Procedure pour intercepter le vol de focus et résister à Win+D
    unsafe extern "system" fn bar_wnd_proc_hook(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _uid_subclass: usize,
        _ref_data: usize,
    ) -> LRESULT {
        match msg {
            WM_NCCALCSIZE => {
                // Supprime totalement le cadre non-client (barre de titre et bordures DWM)
                if wparam != 0 {
                    return 0;
                }
            }
            WM_NCACTIVATE => {
                // Empêche Windows de dessiner la barre de titre standard "Lanceur Bandeau"
                return 1;
            }
            WM_ERASEBKGND => {
                // Empêche formellement Windows de repeindre le fond en blanc par défaut
                return 1;
            }
            WM_NCPAINT => {
                // Empêche Windows de dessiner une bordure ou barre de titre standard non-client
                return 0;
            }
            WM_MOUSEACTIVATE => {
                // Empêche formellement la fenêtre de voler le focus lors des clics souris ordinaires
                return MA_NOACTIVATE as isize;
            }
            WM_SYSCOMMAND => {
                // Empêche Windows de minimiser le bandeau lors d'un Win+D
                let cmd = (wparam & 0xFFF0) as u32;
                if cmd == SC_MINIMIZE {
                    let is_explicit = BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst);
                    if !is_explicit {
                        return 0; // Bloquer la minimisation demandée par le Shell
                    }
                }
            }
            WM_WINDOWPOSCHANGING => {
                // Intercepte les tentatives du Shell de masquer le bandeau lors d'un Win+D
                let is_explicit = BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst);
                if !is_explicit && lparam != 0 {
                    let pos_ptr = lparam as *mut WINDOWPOS;
                    if !pos_ptr.is_null() {
                        let pos = unsafe { &mut *pos_ptr };
                        if (pos.flags & SWP_HIDEWINDOW) != 0 {
                            pos.flags &= !SWP_HIDEWINDOW;
                            pos.flags |= SWP_SHOWWINDOW;
                        }
                    }
                }
            }
            WM_WINDOWPOSCHANGED => {
                let is_explicit = BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst);
                if !is_explicit {
                    unsafe {
                        if IsIconic(hwnd) != 0 {
                            ShowWindow(hwnd, SW_RESTORE);
                        }
                    }
                }
            }
            WM_SIZE => {
                let is_explicit = BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst);
                if !is_explicit && wparam == SIZE_MINIMIZED as usize {
                    unsafe { ShowWindow(hwnd, SW_RESTORE); }
                    return 0;
                }
            }
            WM_SHOWWINDOW => {
                let is_explicit = BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst);
                if !is_explicit && wparam == 0 {
                    return 0;
                }
            }
            _ => {}
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Rattache la fenêtre à Progman (Bureau Windows, architecture Stardock Fences / Desktop Widgets).
    /// Permet à la fenêtre de résister à Win + D (car elle fait partie intégrante du Bureau)
    /// tout en cédant 100% du focus et du premier plan aux autres applications actives.
    /// Applique les styles ToolWindow, NoActivate et configure le mode stay_on_top
    pub fn setup_bar_window_styles(hwnd: HWND, stay_on_top: bool) {
        if hwnd.is_null() {
            return;
        }
        STAY_ON_TOP_ENABLED.store(stay_on_top, Ordering::SeqCst);
        unsafe {
            // 1. Installer le Subclassing Windows (idempotent)
            RemoveWindowSubclass(hwnd, Some(bar_wnd_proc_hook), 101);
            SetWindowSubclass(hwnd, Some(bar_wnd_proc_hook), 101, 0);

            // 2. Styles étendus : ToolWindow + NoActivate
            let mut ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            ex_style |= WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
            ex_style &= !WS_EX_APPWINDOW;
            if stay_on_top {
                ex_style |= WS_EX_TOPMOST;
            } else {
                ex_style &= !WS_EX_TOPMOST;
            }
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style as i32);

            // 3. Styles standard (WS_POPUP pur sans bordures ni barres)
            let mut style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            style &= !(WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU | WS_BORDER | WS_DLGFRAME);
            style |= WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
            SetWindowLongW(hwnd, GWL_STYLE, style as i32);

            let insert_after = if stay_on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
            SetWindowPos(
                hwnd,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );

            // 4. Définir un pinceau de classe sombre pour que Windows ne peigne JAMAIS de fond blanc
            use windows_sys::Win32::Graphics::Gdi::*;
            let dark_brush = CreateSolidBrush(0x002a170f); // RGB(15, 23, 42) = #0f172a
            SetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND, dark_brush as isize);

            // Activer la réception du Drag & Drop
            DragAcceptFiles(hwnd, 1);
        }
    }

    /// Amène la fenêtre au premier plan absolu au-dessus de toutes les fenêtres ouvertes
    pub fn bring_to_foreground(hwnd: HWND, stay_on_top: bool) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            if stay_on_top {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            } else {
                // Flash to topmost to ensure it rises above any maximized or foreground window, then settle
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
                SetWindowPos(
                    hwnd,
                    HWND_NOTOPMOST,
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    /// Donne le focus actif au bandeau lors du démasquage
    pub fn focus_bar_window(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
            
            let cur_fg = GetForegroundWindow();
            let cur_thread = GetWindowThreadProcessId(cur_fg, std::ptr::null_mut());
            let our_thread = windows_sys::Win32::System::Threading::GetCurrentThreadId();

            if cur_thread != 0 && cur_thread != our_thread {
                AttachThreadInput(our_thread, cur_thread, 1);
                SetForegroundWindow(hwnd);
                BringWindowToTop(hwnd);
                SetActiveWindow(hwnd);
                SetFocus(hwnd);
                AttachThreadInput(our_thread, cur_thread, 0);
            } else {
                SetForegroundWindow(hwnd);
                BringWindowToTop(hwnd);
                SetActiveWindow(hwnd);
                SetFocus(hwnd);
            }
        }
    }

    /// Enregistre la barre auprès de Windows (AppBar) pour réserver l'espace à l'écran
    pub fn register_appbar(hwnd: HWND, position: &str, bar_h: i32) {
        if hwnd.is_null() || position == "Floating" {
            return;
        }
        unsafe {
            use windows_sys::Win32::UI::Shell::*;
            let mut abd = APPBARDATA {
                cbSize: std::mem::size_of::<APPBARDATA>() as u32,
                hWnd: hwnd,
                uCallbackMessage: WM_APP + 3,
                uEdge: if position == "Bottom" { ABE_BOTTOM } else { ABE_TOP },
                rc: RECT { left: 0, top: 0, right: 0, bottom: 0 },
                lParam: 0,
            };

            SHAppBarMessage(ABM_NEW, &mut abd);

            let (screen_w, screen_h) = (
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            );

            if position == "Bottom" {
                abd.rc.left = 0;
                abd.rc.right = screen_w;
                abd.rc.top = screen_h - bar_h;
                abd.rc.bottom = screen_h;
            } else {
                abd.rc.left = 0;
                abd.rc.right = screen_w;
                abd.rc.top = 0;
                abd.rc.bottom = bar_h;
            }

            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            SHAppBarMessage(ABM_SETPOS, &mut abd);
        }
    }

    /// Retire l'enregistrement AppBar auprès de Windows
    pub fn unregister_appbar(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            use windows_sys::Win32::UI::Shell::*;
            let mut abd = APPBARDATA {
                cbSize: std::mem::size_of::<APPBARDATA>() as u32,
                hWnd: hwnd,
                uCallbackMessage: 0,
                uEdge: 0,
                rc: RECT { left: 0, top: 0, right: 0, bottom: 0 },
                lParam: 0,
            };
            SHAppBarMessage(ABM_REMOVE, &mut abd);
        }
    }

    /// Dimensionne la fenêtre à sa taille finale (pleine largeur) SANS la rendre visible.
    /// Utilisé lors de l'initialisation pour que la zone de clic droit soit correcte
    /// dès le premier affichage, même avant que l'utilisateur n'ait masqué/réaffiché la barre.
    pub fn size_bar_window_hidden(
        hwnd: HWND,
        position: &str,
        bar_h: i32,
        bar_x: i32,
        bar_y: i32,
        bar_w: i32,
    ) {
        if hwnd.is_null() {
            return;
        }
        let (work_x, work_y, work_w, work_h) = get_work_area();
        let (x, y, w, h) = match position {
            "Bottom" => (work_x, work_y + work_h - bar_h, work_w, bar_h),
            "Floating" => {
                let width = if bar_w > 0 { bar_w } else { 860.min(work_w - 40) };
                let x = if bar_x > 0 { bar_x } else { work_x + (work_w - width) / 2 };
                let y = if bar_y > 0 { bar_y } else { work_y + 30 };
                (x, y, width, bar_h)
            }
            _ => (work_x, work_y, work_w, bar_h), // "Top" par défaut
        };
        unsafe {
            // SWP_NOACTIVATE + ni SWP_SHOWWINDOW ni SWP_HIDEWINDOW → taille sans affichage
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                x,
                y,
                w,
                h,
                SWP_NOACTIVATE,
            );
        }
    }

    /// Récupère l'espace de travail Windows (hors barre des tâches)
    pub fn get_work_area() -> (i32, i32, i32, i32) {
        unsafe {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                &mut rect as *mut _ as *mut _,
                0,
            ) != 0
            {
                let x = rect.left;
                let y = rect.top;
                let w = rect.right - rect.left;
                let h = rect.bottom - rect.top;
                (x, y, w, h)
            } else {
                let w = GetSystemMetrics(SM_CXSCREEN);
                let h = GetSystemMetrics(SM_CYSCREEN);
                (0, 0, w, h)
            }
        }
    }

    /// Repositionne et redimensionne strictement la fenêtre sans bloquer les fenêtres en arrière-plan
    pub fn position_bar_window(
        hwnd: HWND,
        position: &str,
        bar_h: i32,
        bar_x: i32,
        bar_y: i32,
        bar_w: i32,
        is_expanded: bool,
        stay_on_top: bool,
    ) {
        if hwnd.is_null() {
            return;
        }
        let (work_x, work_y, work_w, work_h) = get_work_area();
        let current_h = if is_expanded { (bar_h + 240).min(work_h) } else { bar_h };

        let (x, y, w, h) = match position {
            "Bottom" => {
                let y = work_y + work_h - current_h;
                (work_x, y, work_w, current_h)
            }
            "Floating" => {
                let width = if bar_w > 0 { bar_w } else { 860.min(work_w - 40) };
                let x = if bar_x > 0 { bar_x } else { work_x + (work_w - width) / 2 };
                let y = if bar_y > 0 { bar_y } else { work_y + 30 };
                (x, y, width, current_h)
            }
            _ => {
                // "Top" par défaut : ancré en haut
                (work_x, work_y, work_w, current_h)
            }
        };

        unsafe {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                x,
                y,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            InvalidateRect(hwnd, std::ptr::null(), 1);
            UpdateWindow(hwnd);
        }
    }

    /// Enregistre un raccourci global
    pub fn register_hotkey_combo(hwnd: HWND, id: i32, modifiers: &[String], key: &str) -> bool {
        if hwnd.is_null() || key.trim().is_empty() {
            return false;
        }
        unsafe {
            UnregisterHotKey(hwnd, id);

            let mut mod_flags = MOD_NOREPEAT;
            for m in modifiers {
                match m.to_lowercase().as_str() {
                    "control" | "ctrl" => mod_flags |= MOD_CONTROL,
                    "alt" => mod_flags |= MOD_ALT,
                    "shift" => mod_flags |= MOD_SHIFT,
                    "win" | "windows" => mod_flags |= MOD_WIN,
                    _ => {}
                }
            }

            let vk = parse_virtual_key(key);
            RegisterHotKey(hwnd, id, mod_flags, vk) != 0
        }
    }

    pub fn unregister_hotkey_id(hwnd: HWND, id: i32) {
        if !hwnd.is_null() {
            unsafe {
                UnregisterHotKey(hwnd, id);
            }
        }
    }

    fn parse_virtual_key(key: &str) -> u32 {
        match key.to_uppercase().as_str() {
            "SPACE" => VK_SPACE as u32,
            "RETURN" | "ENTER" => VK_RETURN as u32,
            "TAB" => VK_TAB as u32,
            "ESCAPE" | "ESC" => VK_ESCAPE as u32,
            "F1" => VK_F1 as u32,
            "F2" => VK_F2 as u32,
            "F3" => VK_F3 as u32,
            "F4" => VK_F4 as u32,
            "F5" => VK_F5 as u32,
            "F6" => VK_F6 as u32,
            "F7" => VK_F7 as u32,
            "F8" => VK_F8 as u32,
            "F9" => VK_F9 as u32,
            "F10" => VK_F10 as u32,
            "F11" => VK_F11 as u32,
            "F12" => VK_F12 as u32,
            s if s.len() == 1 => {
                let c = s.chars().next().unwrap();
                c as u32
            }
            _ => VK_SPACE as u32,
        }
    }

    /// Création / Mise à jour de l'icône dans la zone de notification (Systray)
    pub fn create_tray_icon(hwnd: HWND, tooltip: &str) -> bool {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = WM_APP_TRAY;
            nid.hIcon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);

            let tip_wide = to_wide_null(tooltip);
            let copy_len = tip_wide.len().min(nid.szTip.len() - 1);
            for i in 0..copy_len {
                nid.szTip[i] = tip_wide[i];
            }

            Shell_NotifyIconW(NIM_ADD, &nid) != 0
        }
    }

    pub fn remove_tray_icon(hwnd: HWND) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }

    /// Affiche le menu contextuel instantané et autonome avec TPM_RETURNCMD et fix KB135788.
    /// Sauvegarde et restaure la fenêtre au premier plan pour éviter le vol de focus permanent.
    pub fn show_tray_context_menu(hwnd: HWND, is_autostart: bool) {
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }

            let show_text = to_wide_null("👁️ Afficher / Masquer");
            let set_text = to_wide_null("⚙️ Paramètres...");
            let auto_text = to_wide_null(if is_autostart {
                "✓ Lancer au démarrage de Windows"
            } else {
                "  Lancer au démarrage de Windows"
            });
            let quit_text = to_wide_null("❌ Quitter");

            AppendMenuW(menu, MF_STRING, IDM_SHOW_HIDE, show_text.as_ptr());
            AppendMenuW(menu, MF_STRING, IDM_SETTINGS, set_text.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, IDM_AUTOSTART, auto_text.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, IDM_QUIT, quit_text.as_ptr());

            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);

            // Sauvegarde la fenêtre active AVANT de prendre temporairement le focus
            // (obligatoire pour que TrackPopupMenuEx fonctionne, mais on restaure ensuite)
            let prev_foreground = GetForegroundWindow();
            SetForegroundWindow(hwnd);

            let cmd_selected = TrackPopupMenuEx(
                menu,
                TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
                pt.x,
                pt.y,
                hwnd,
                std::ptr::null(),
            ) as usize;

            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);

            // Restaure immédiatement le focus à la fenêtre précédente
            if !prev_foreground.is_null() && prev_foreground != hwnd {
                SetForegroundWindow(prev_foreground);
            }

            if cmd_selected != 0 {
                let tray_hwnd = SYSTRAY_HWND.load(Ordering::SeqCst) as HWND;
                if !tray_hwnd.is_null() {
                    PostMessageW(tray_hwnd, WM_COMMAND, cmd_selected, 0);
                }
            }
        }
    }

    /// Basculer l'option de démarrage automatique avec Windows
    pub fn set_autostart(enabled: bool) -> Result<(), String> {
        unsafe {
            let key_path = to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
            let mut hkey: HKEY = std::ptr::null_mut();
            let res = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                key_path.as_ptr(),
                0,
                KEY_ALL_ACCESS,
                &mut hkey,
            );
            if res != 0 {
                return Err("Impossible d'ouvrir le registre Windows".to_string());
            }

            let app_name = to_wide_null("LanceurBar");
            if enabled {
                if let Ok(exe) = std::env::current_exe() {
                    let exe_str = format!("\"{}\"", exe.to_string_lossy());
                    let exe_wide = to_wide_null(&exe_str);
                    RegSetValueExW(
                        hkey,
                        app_name.as_ptr(),
                        0,
                        REG_SZ,
                        exe_wide.as_ptr() as *const u8,
                        (exe_wide.len() * 2) as u32,
                    );
                }
            } else {
                RegDeleteValueW(hkey, app_name.as_ptr());
            }

            RegCloseKey(hkey);
            Ok(())
        }
    }

    /// Extrait l'icône d'un fichier .exe, .ico ou .lnk et l'enregistre en cache PNG
    pub fn extract_and_cache_icon(target_path: &str, cache_dir: &Path) -> Option<PathBuf> {
        let p = Path::new(target_path);
        let stem = p.file_stem()?.to_string_lossy();
        let cache_file = cache_dir.join(format!("{}.png", stem));

        if cache_file.exists() {
            return Some(cache_file);
        }

        unsafe {
            let wide_path = to_wide_null(target_path);
            let mut hicon: HICON = std::ptr::null_mut();

            // Extraire l'icône principale du fichier
            ExtractIconExW(
                wide_path.as_ptr(),
                0,
                &mut hicon,
                std::ptr::null_mut(),
                1,
            );

            if hicon.is_null() {
                return None;
            }

            // Convertir HICON en image RGBA
            let mut icon_info: ICONINFO = std::mem::zeroed();
            if GetIconInfo(hicon, &mut icon_info) == 0 {
                DestroyIcon(hicon);
                return None;
            }

            let hdc = CreateCompatibleDC(std::ptr::null_mut());
            let mut bmp: BITMAP = std::mem::zeroed();
            GetObjectW(
                icon_info.hbmColor,
                std::mem::size_of::<BITMAP>() as i32,
                &mut bmp as *mut _ as *mut _,
            );

            let width = bmp.bmWidth;
            let height = bmp.bmHeight;

            if width <= 0 || height <= 0 {
                if !icon_info.hbmColor.is_null() { DeleteObject(icon_info.hbmColor); }
                if !icon_info.hbmMask.is_null() { DeleteObject(icon_info.hbmMask); }
                DeleteDC(hdc);
                DestroyIcon(hicon);
                return None;
            }

            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = width;
            bi.bmiHeader.biHeight = -height; // Top-down
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;

            let mut raw_pixels: Vec<u8> = vec![0; (width * height * 4) as usize];
            GetDIBits(
                hdc,
                icon_info.hbmColor,
                0,
                height as u32,
                raw_pixels.as_mut_ptr() as *mut _,
                &mut bi,
                DIB_RGB_COLORS,
            );

            // BGRX / BGRA -> RGBA
            for chunk in raw_pixels.chunks_exact_mut(4) {
                let b = chunk[0];
                let r = chunk[2];
                chunk[0] = r;
                chunk[2] = b;
                if chunk[3] == 0 && (chunk[0] > 0 || chunk[1] > 0 || chunk[2] > 0) {
                    chunk[3] = 255;
                }
            }

            if !icon_info.hbmColor.is_null() { DeleteObject(icon_info.hbmColor); }
            if !icon_info.hbmMask.is_null() { DeleteObject(icon_info.hbmMask); }
            DeleteDC(hdc);
            DestroyIcon(hicon);

            if let Some(img) = image::RgbaImage::from_raw(width as u32, height as u32, raw_pixels) {
                if img.save(&cache_file).is_ok() {
                    return Some(cache_file);
                }
            }
            None
        }
    }
}

#[cfg(not(windows))]
pub mod win32 {
    use super::*;
    pub static BAR_EXPLICITLY_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    pub fn setup_bar_window_styles(_hwnd: *mut std::ffi::c_void, _stay_on_top: bool) {}
    pub fn position_bar_window(_hwnd: *mut std::ffi::c_void, _pos: &str, _h: i32, _x: i32, _y: i32, _w: i32, _exp: bool, _stay: bool) {}
    pub fn bring_to_foreground(_hwnd: *mut std::ffi::c_void) {}
    pub fn register_hotkey_combo(_hwnd: *mut std::ffi::c_void, _id: i32, _mods: &[String], _key: &str) -> bool { true }
    pub fn unregister_hotkey_id(_hwnd: *mut std::ffi::c_void, _id: i32) {}
    pub fn create_tray_icon(_hwnd: *mut std::ffi::c_void, _tip: &str) -> bool { true }
    pub fn remove_tray_icon(_hwnd: *mut std::ffi::c_void) {}
    pub fn show_tray_context_menu(_hwnd: *mut std::ffi::c_void, _is_auto: bool) {}
    pub fn set_autostart(_enabled: bool) -> Result<(), String> { Ok(()) }
    pub fn extract_and_cache_icon(_target: &str, _cache: &Path) -> Option<PathBuf> { None }
    pub fn to_wide_null(_s: &str) -> Vec<u16> { Vec::new() }
    pub fn find_bar_hwnd() -> *mut std::ffi::c_void { std::ptr::null_mut() }
}
