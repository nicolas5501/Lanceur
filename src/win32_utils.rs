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
    use windows_sys::Win32::System::Ole::*;
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
    pub static BAR_WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);
    pub static DESKTOP_PARENT: AtomicUsize = AtomicUsize::new(0);

    pub const WM_APP_TRAY: u32 = WM_APP + 1;
    pub const WM_APP_HOTKEY: u32 = WM_APP + 2;
    pub const IDM_SHOW_HIDE: usize = 1001;
    pub const IDM_SETTINGS: usize = 1002;
    pub const IDM_AUTOSTART: usize = 1003;
    pub const IDM_QUIT: usize = 1004;
    pub const MAIN_HOTKEY_ID: i32 = 9001;
    pub const VISIBILITY_TIMER_ID: usize = 1;

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

    /// Recherche fiable du HWND de la fenêtre des Paramètres
    pub fn find_settings_hwnd() -> HWND {
        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let mut process_id: u32 = 0;
            unsafe { GetWindowThreadProcessId(hwnd, &mut process_id); }
            if process_id == unsafe { GetCurrentProcessId() } {
                let mut title_buf = [0u16; 256];
                let len = unsafe { GetWindowTextW(hwnd, title_buf.as_mut_ptr(), 256) };
                if len > 0 {
                    let title = String::from_utf16_lossy(&title_buf[..len as usize]);
                    if title.contains("Configuration du Lanceur") {
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
        found_hwnd
    }

    pub type DropCallback = Box<dyn Fn(Vec<String>, i32, i32, bool) + Send + Sync + 'static>;
    pub static DROP_CALLBACK: std::sync::Mutex<Option<DropCallback>> = std::sync::Mutex::new(None);

    pub fn set_drop_callback<F>(cb: F)
    where
        F: Fn(Vec<String>, i32, i32, bool) + Send + Sync + 'static,
    {
        if let Ok(mut lock) = DROP_CALLBACK.lock() {
            *lock = Some(Box::new(cb));
        }
    }

    /// Subclass Window Procedure pour intercepter le vol de focus, résister à Win+D et gérer le Drag & Drop
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
                // Supprime totalement le cadre non-client
                if wparam != 0 {
                    return 0;
                }
            }
            WM_NCACTIVATE => {
                // Empêche Windows de dessiner la barre de titre standard
                return 1;
            }
            WM_ACTIVATE => {
                let state = (wparam & 0xFFFF) as u32;
                if state == WA_INACTIVE as u32 {
                    if STAY_ON_TOP_ENABLED.load(Ordering::SeqCst) && !BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst) {
                        unsafe {
                            let progman = FindWindowW(to_wide_null("Progman").as_ptr(), std::ptr::null());
                            if !progman.is_null() {
                                SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, progman as isize);
                            }
                        }
                    }
                }
                return 1;
            }
            WM_ERASEBKGND => {
                // Empêche Windows d'effacer le fond avec un pinceau blanc standard
                return 1;
            }
            WM_NCPAINT => {
                return 0;
            }
            WM_MOUSEACTIVATE => {
                // Empêche formellement la fenêtre de voler le focus lors des clics souris ordinaires
                return MA_NOACTIVATE as isize;
            }
            WM_DROPFILES => {
                let hdrop = wparam as HDROP;
                let mut pt = POINT { x: 0, y: 0 };
                unsafe { DragQueryPoint(hdrop, &mut pt); }
                let count = unsafe { DragQueryFileW(hdrop, 0xffffffff, std::ptr::null_mut(), 0) };
                let mut files = Vec::new();
                for i in 0..count {
                    let mut buf = [0u16; 512];
                    let len = unsafe { DragQueryFileW(hdrop, i, buf.as_mut_ptr(), 512) };
                    if len > 0 {
                        files.push(String::from_utf16_lossy(&buf[..len as usize]));
                    }
                }
                unsafe { DragFinish(hdrop); }

                if !files.is_empty() {
                    if let Ok(guard) = DROP_CALLBACK.lock() {
                        if let Some(cb) = guard.as_ref() {
                            cb(files, pt.x, pt.y, false);
                        }
                    }
                }
                return 0;
            }
            WM_SYSCOMMAND => {
                // Empêche Windows de minimiser le bandeau lors d'un Win+D
                let cmd = (wparam & 0xFFF0) as u32;
                if STAY_ON_TOP_ENABLED.load(Ordering::SeqCst) && cmd == SC_MINIMIZE {
                    let is_explicit = BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst);
                    if !is_explicit {
                        return 0; // Bloquer la minimisation demandée par le Shell
                    }
                }
            }
            WM_WINDOWPOSCHANGING => {
                if STAY_ON_TOP_ENABLED.load(Ordering::SeqCst) && !BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst) && lparam != 0 {
                    let pos = unsafe { &mut *(lparam as *mut WINDOWPOS) };
                    // Empêcher le masquage automatique déclenché par Windows+D
                    pos.flags &= !SWP_HIDEWINDOW;
                }
            }
            WM_SHOWWINDOW => {
                if wparam == 0 {
                    if STAY_ON_TOP_ENABLED.load(Ordering::SeqCst) && !BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst) {
                        // Bloquer le masquage automatique déclenché par Windows+D
                        return 0;
                    }
                    BAR_WINDOW_VISIBLE.store(false, Ordering::SeqCst);
                } else {
                    BAR_WINDOW_VISIBLE.store(true, Ordering::SeqCst);
                }
            }
            _ => {}
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Subclass Window Procedure pour la fenêtre des Paramètres (gestion du Drag & Drop)
    unsafe extern "system" fn settings_wnd_proc_hook(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _uid_subclass: usize,
        _ref_data: usize,
    ) -> LRESULT {
        match msg {
            WM_DROPFILES => {
                let hdrop = wparam as HDROP;
                let mut pt = POINT { x: 0, y: 0 };
                unsafe { DragQueryPoint(hdrop, &mut pt); }
                let count = unsafe { DragQueryFileW(hdrop, 0xffffffff, std::ptr::null_mut(), 0) };
                let mut files = Vec::new();
                for i in 0..count {
                    let mut buf = [0u16; 512];
                    let len = unsafe { DragQueryFileW(hdrop, i, buf.as_mut_ptr(), 512) };
                    if len > 0 {
                        files.push(String::from_utf16_lossy(&buf[..len as usize]));
                    }
                }
                unsafe { DragFinish(hdrop); }

                if !files.is_empty() {
                    if let Ok(guard) = DROP_CALLBACK.lock() {
                        if let Some(cb) = guard.as_ref() {
                            cb(files, pt.x, pt.y, true);
                        }
                    }
                }
                return 0;
            }
            _ => {}
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Applique le hook et active le Drag & Drop sur la fenêtre des Paramètres
    pub fn setup_settings_window_styles(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            RemoveWindowSubclass(hwnd, Some(settings_wnd_proc_hook), 102);
            SetWindowSubclass(hwnd, Some(settings_wnd_proc_hook), 102, 0);
            let _ = RevokeDragDrop(hwnd);
            DragAcceptFiles(hwnd, 1);
            ChangeWindowMessageFilter(WM_DROPFILES, 1);
            ChangeWindowMessageFilter(WM_COPYDATA, 1);
            ChangeWindowMessageFilter(0x0049, 1);
        }
    }

    /// Applique les styles ToolWindow, NoActivate et configure le mode stay_on_top
    pub fn setup_bar_window_styles(hwnd: HWND, stay_on_top: bool, floating: bool) {
        if hwnd.is_null() {
            return;
        }
        STAY_ON_TOP_ENABLED.store(stay_on_top, Ordering::SeqCst);
        unsafe {
            // 1. Installer le Subclassing Windows (idempotent)
            RemoveWindowSubclass(hwnd, Some(bar_wnd_proc_hook), 101);
            SetWindowSubclass(hwnd, Some(bar_wnd_proc_hook), 101, 0);

            // 2. Styles étendus : ToolWindow + NoActivate, JAMAIS de WS_EX_TOPMOST pour ne jamais écraser les applications actives
            let mut ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            ex_style |= WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
            ex_style &= !WS_EX_APPWINDOW;
            ex_style &= !WS_EX_TOPMOST;
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style as i32);

            // 3. Styles standard : TOUJOURS WS_POPUP (jamais WS_CHILD) pour préserver le moteur de rendu Direct3D/Slint et les menus déroulants
            let mut style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            style &= !(WS_CAPTION | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU | WS_BORDER | WS_DLGFRAME | WS_THICKFRAME | WS_CHILD);
            style |= WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_VISIBLE;
            SetWindowLongW(hwnd, GWL_STYLE, style as i32);

            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );

            // Activer la réception du Drag & Drop et débloquer les filtres UIPI
            let _ = RevokeDragDrop(hwnd);
            DragAcceptFiles(hwnd, 1);
            ChangeWindowMessageFilter(WM_DROPFILES, 1);
            ChangeWindowMessageFilter(WM_COPYDATA, 1);
            ChangeWindowMessageFilter(0x0049, 1);
        }
    }

    /// Configure Progman comme propriétaire de la fenêtre Popup (architecture Fences).
    /// Maintient le bandeau au-dessus du Bureau lors de Win+D tout en laissant toutes
    /// les fenêtres d'applications actives s'afficher au-dessus de lui.
    pub fn set_desktop_parent(hwnd: HWND, enabled: bool) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            let progman = FindWindowW(to_wide_null("Progman").as_ptr(), std::ptr::null());
            if enabled && !progman.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, progman as isize);
                DESKTOP_PARENT.store(progman as usize, Ordering::SeqCst);
            } else {
                SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
                DESKTOP_PARENT.store(0, Ordering::SeqCst);
            }
        }
    }

    /// Recherche le conteneur hôte du bureau Windows (Progman ou WorkerW contenant SHELLDLL_DefView).
    /// Architecture identique à Stardock Fences / Rainmeter Desktop Widgets.
    pub fn get_desktop_host_window() -> HWND {
        unsafe {
            let progman = FindWindowW(to_wide_null("Progman").as_ptr(), std::ptr::null());
            if progman.is_null() {
                return std::ptr::null_mut();
            }

            // Envoi du message non documenté 0x052C pour scinder la pile WorkerW
            let mut result: usize = 0;
            SendMessageTimeoutW(
                progman,
                0x052C,
                0x0000000D,
                0,
                SMTO_NORMAL,
                1000,
                &mut result as *mut usize as *mut _,
            );

            // 1. Vérifier si SHELLDLL_DefView est directement dans Progman
            let defview_in_progman = FindWindowExW(
                progman,
                std::ptr::null_mut(),
                to_wide_null("SHELLDLL_DefView").as_ptr(),
                std::ptr::null(),
            );
            if !defview_in_progman.is_null() {
                return progman;
            }

            // 2. Sinon, énumérer les WorkerW top-level pour trouver celui qui abrite SHELLDLL_DefView
            unsafe extern "system" fn enum_workerw_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
                unsafe {
                    let def_view = FindWindowExW(
                        hwnd,
                        std::ptr::null_mut(),
                        to_wide_null("SHELLDLL_DefView").as_ptr(),
                        std::ptr::null(),
                    );
                    if !def_view.is_null() {
                        *(lparam as *mut HWND) = hwnd;
                        return 0; // Arrêter l'énumération dès qu'on le trouve
                    }
                }
                1
            }

            let mut host_workerw: HWND = std::ptr::null_mut();
            EnumWindows(Some(enum_workerw_proc), &mut host_workerw as *mut _ as LPARAM);

            if !host_workerw.is_null() {
                return host_workerw;
            }

            progman
        }
    }

    /// Amène la fenêtre au premier plan actif au-dessus des autres fenêtres
    pub fn bring_to_foreground(hwnd: HWND, _stay_on_top: bool) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            // 1. Détacher temporairement de Progman pour permettre l'élévation Z-Order au premier plan
            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);

            let fore_wnd = GetForegroundWindow();
            let target_thread = if !fore_wnd.is_null() {
                GetWindowThreadProcessId(fore_wnd, std::ptr::null_mut())
            } else {
                0
            };
            let current_thread = GetCurrentThreadId();
            let attached = target_thread != 0
                && target_thread != current_thread
                && AttachThreadInput(current_thread, target_thread, 1) != 0;

            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            SetActiveWindow(hwnd);

            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );

            if attached {
                AttachThreadInput(current_thread, target_thread, 0);
            }

            if _stay_on_top {
                let progman = FindWindowW(to_wide_null("Progman").as_ptr(), std::ptr::null());
                if !progman.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, progman as isize);
                    DESKTOP_PARENT.store(progman as usize, Ordering::SeqCst);
                }
            }
        }
    }

    /// Indique si la fenêtre du bandeau est actuellement la fenêtre active au premier plan
    pub fn is_bar_window_foreground() -> bool {
        let hwnd = find_bar_hwnd();
        if hwnd.is_null() || unsafe { IsWindow(hwnd) == 0 } {
            return false;
        }
        unsafe {
            let fore = GetForegroundWindow();
            fore == hwnd
        }
    }

    /// Vérifie si le bandeau est actuellement visible à l'écran (non masqué hors écran)
    pub fn is_bar_window_visible() -> bool {
        let hwnd = find_bar_hwnd();
        if hwnd.is_null() || unsafe { IsWindow(hwnd) == 0 } {
            return false;
        }
        if BAR_EXPLICITLY_HIDDEN.load(Ordering::SeqCst) {
            return false;
        }
        unsafe {
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if GetWindowRect(hwnd, &mut rect) != 0 {
                if rect.left < -10000 || rect.top < -10000 {
                    return false;
                }
            }
        }
        BAR_WINDOW_VISIBLE.load(Ordering::SeqCst)
    }

    pub fn restore_foreground_window(hwnd: HWND) {
        if hwnd.is_null() || unsafe { IsWindow(hwnd) == 0 } {
            return;
        }
        unsafe {
            let target_thread = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
            let current_thread = GetCurrentThreadId();
            let attached = target_thread != 0
                && target_thread != current_thread
                && AttachThreadInput(current_thread, target_thread, 1) != 0;

            SetForegroundWindow(hwnd);
            SetActiveWindow(hwnd);

            if attached {
                AttachThreadInput(current_thread, target_thread, 0);
            }
        }
    }

    /// Compatibilité API : l'affichage du bandeau ne doit jamais prendre le focus.
    pub fn focus_bar_window(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE); }
    }

    /// Compatibilité conservée pour les anciens appels. La barre ne doit pas
    /// réserver l'espace de travail Windows.
    pub fn register_appbar(hwnd: HWND, position: &str, bar_h: i32) {
        let _ = (hwnd, position, bar_h);
    }

    /// Retire l'enregistrement AppBar auprès de Windows
    pub fn unregister_appbar(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            let mut abd: APPBARDATA = std::mem::zeroed();
            abd.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
            abd.hWnd = hwnd;
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
        let safe_h = bar_h.min(work_h);
        let (x, y, w, h) = match position {
            "Bottom" => (work_x, work_y + work_h - safe_h, work_w, safe_h),
            "Floating" => {
                let width = if bar_w > 0 { bar_w } else { 860.min(work_w - 40) };
                let x = if bar_x > 0 { bar_x } else { work_x + (work_w - width) / 2 };
                let y = if bar_y > 0 { bar_y } else { work_y + 30 };
                (x, y, width, safe_h)
            }
            _ => (work_x, work_y, work_w, safe_h), // "Top" par défaut
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
        let current_h = if is_expanded { (bar_h + 500).min(work_h) } else { bar_h.min(work_h) };

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

    pub fn move_bar_window_to(hwnd: HWND, x: i32, y: i32) {
        if hwnd.is_null() {
            return;
        }
        unsafe {
            SetWindowPos(hwnd, HWND_TOP, x, y, 0, 0,
                SWP_NOSIZE | SWP_NOACTIVATE);
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
            "BACKSPACE" | "BACK" => VK_BACK as u32,
            "PRINTSCREEN" | "PRINT" | "SNAPSHOT" => VK_SNAPSHOT as u32,
            "PAUSE" => VK_PAUSE as u32,
            "SCROLLLOCK" | "SCROLL" => VK_SCROLL as u32,
            "INSERT" | "INS" => VK_INSERT as u32,
            "DELETE" | "DEL" | "SUPPR" => VK_DELETE as u32,
            "HOME" | "DEBUT" => VK_HOME as u32,
            "END" | "FIN" => VK_END as u32,
            "PAGEUP" | "PGUP" | "PRIOR" => VK_PRIOR as u32,
            "PAGEDOWN" | "PGDN" | "NEXT" => VK_NEXT as u32,
            "UP" | "HAUT" => VK_UP as u32,
            "DOWN" | "BAS" => VK_DOWN as u32,
            "LEFT" | "GAUCHE" => VK_LEFT as u32,
            "RIGHT" | "DROITE" => VK_RIGHT as u32,
            "CAPSLOCK" | "CAPS" | "CAPITAL" => VK_CAPITAL as u32,
            "NUMLOCK" => VK_NUMLOCK as u32,
            "NUMPAD0" => VK_NUMPAD0 as u32,
            "NUMPAD1" => VK_NUMPAD1 as u32,
            "NUMPAD2" => VK_NUMPAD2 as u32,
            "NUMPAD3" => VK_NUMPAD3 as u32,
            "NUMPAD4" => VK_NUMPAD4 as u32,
            "NUMPAD5" => VK_NUMPAD5 as u32,
            "NUMPAD6" => VK_NUMPAD6 as u32,
            "NUMPAD7" => VK_NUMPAD7 as u32,
            "NUMPAD8" => VK_NUMPAD8 as u32,
            "NUMPAD9" => VK_NUMPAD9 as u32,
            "MULTIPLY" => VK_MULTIPLY as u32,
            "ADD" => VK_ADD as u32,
            "SUBTRACT" => VK_SUBTRACT as u32,
            "DECIMAL" => VK_DECIMAL as u32,
            "DIVIDE" => VK_DIVIDE as u32,
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
            "F13" => 0x7C,
            "F14" => 0x7D,
            "F15" => 0x7E,
            "F16" => 0x7F,
            "F17" => 0x80,
            "F18" => 0x81,
            "F19" => 0x82,
            "F20" => 0x83,
            "F21" => 0x84,
            "F22" => 0x85,
            "F23" => 0x86,
            "F24" => 0x87,
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

    /// Boîte de dialogue native Windows pour choisir une couleur avec palette et pipette
    pub fn pick_color_dialog(initial_hex: &str) -> Option<String> {
        #[repr(C)]
        struct CHOOSECOLORW {
            l_struct_size: u32,
            hwnd_owner: HWND,
            h_instance: HWND,
            rgb_result: u32,
            lp_cust_colors: *mut u32,
            flags: u32,
            l_cust_data: isize,
            lpfn_hook: Option<unsafe extern "system" fn(HWND, u32, usize, isize) -> usize>,
            lp_template_name: *const u16,
        }

        #[link(name = "comdlg32")]
        unsafe extern "system" {
            fn ChooseColorW(lpcc: *mut CHOOSECOLORW) -> i32;
        }

        let mut cust_colors: [u32; 16] = [
            0x1e293b, 0x0f172a, 0x334155, 0x2563eb,
            0x3b82f6, 0x38bdf8, 0x06b6d4, 0x10b981,
            0xf59e0b, 0xef4444, 0xec4899, 0x8b5cf6,
            0xffffff, 0xf8fafc, 0x94a3b8, 0x000000,
        ];

        let clean_hex = initial_hex.trim().trim_start_matches('#');
        let initial_rgb = if clean_hex.len() >= 6 {
            let r = u32::from_str_radix(&clean_hex[0..2], 16).unwrap_or(0);
            let g = u32::from_str_radix(&clean_hex[2..4], 16).unwrap_or(0);
            let b = u32::from_str_radix(&clean_hex[4..6], 16).unwrap_or(0);
            r | (g << 8) | (b << 16)
        } else {
            0x3b291e
        };

        let mut cc: CHOOSECOLORW = unsafe { std::mem::zeroed() };
        cc.l_struct_size = std::mem::size_of::<CHOOSECOLORW>() as u32;
        cc.rgb_result = initial_rgb;
        cc.lp_cust_colors = cust_colors.as_mut_ptr();
        cc.flags = 0x00000001 | 0x00000002; // CC_RGBINIT | CC_FULLOPEN

        let ret = unsafe { ChooseColorW(&mut cc) };
        if ret != 0 {
            let r = (cc.rgb_result & 0xFF) as u8;
            let g = ((cc.rgb_result >> 8) & 0xFF) as u8;
            let b = ((cc.rgb_result >> 16) & 0xFF) as u8;
            Some(format!("#{:02x}{:02x}{:02x}", r, g, b))
        } else {
            None
        }
    }

    /// Pipette de sélection d'écran (Eyedropper) : capture la couleur de n'importe quel pixel de l'écran au clic
    pub fn pick_color_eyedropper() -> Option<String> {
        unsafe {
            // Attendre le relâchement du bouton gauche s'il était déjà enfoncé
            while (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }

            let cursor = LoadCursorW(std::ptr::null_mut(), IDC_CROSS);

            let start_time = std::time::Instant::now();
            loop {
                if !cursor.is_null() {
                    SetCursor(cursor);
                }

                // Annulation après 45 secondes d'inactivité
                if start_time.elapsed().as_secs() > 45 {
                    return None;
                }

                // Annulation via Echap ou Clic droit
                if (GetAsyncKeyState(VK_ESCAPE as i32) as u16 & 0x8000) != 0
                    || (GetAsyncKeyState(VK_RBUTTON as i32) as u16 & 0x8000) != 0
                {
                    return None;
                }

                // Clic gauche : capture du pixel sous le curseur
                if (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 {
                    let mut pt = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut pt);
                    let hdc = GetDC(std::ptr::null_mut());
                    if !hdc.is_null() {
                        let pixel = GetPixel(hdc, pt.x, pt.y);
                        ReleaseDC(std::ptr::null_mut(), hdc);
                        if pixel != 0xFFFFFFFF {
                            let r = (pixel & 0xFF) as u8;
                            let g = ((pixel >> 8) & 0xFF) as u8;
                            let b = ((pixel >> 16) & 0xFF) as u8;
                            return Some(format!("#{:02x}{:02x}{:02x}", r, g, b));
                        }
                    }
                    return None;
                }

                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }
    }
}

#[cfg(not(windows))]
pub mod win32 {
    use super::*;
    pub static BAR_EXPLICITLY_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    pub fn setup_bar_window_styles(_hwnd: *mut std::ffi::c_void, _stay_on_top: bool, _floating: bool) {}
    pub fn position_bar_window(_hwnd: *mut std::ffi::c_void, _pos: &str, _h: i32, _x: i32, _y: i32, _w: i32, _exp: bool, _stay: bool) {}
    pub fn bring_to_foreground(_hwnd: *mut std::ffi::c_void, _stay_on_top: bool) {}
    pub fn set_desktop_parent(_hwnd: *mut std::ffi::c_void, _enabled: bool) {}
    pub fn is_bar_window_visible() -> bool { true }
    pub fn is_bar_window_foreground() -> bool { false }
    pub fn register_hotkey_combo(_hwnd: *mut std::ffi::c_void, _id: i32, _mods: &[String], _key: &str) -> bool { true }
    pub fn unregister_hotkey_id(_hwnd: *mut std::ffi::c_void, _id: i32) {}
    pub fn create_tray_icon(_hwnd: *mut std::ffi::c_void, _tip: &str) -> bool { true }
    pub fn pick_color_dialog(_initial_hex: &str) -> Option<String> { None }
    pub fn pick_color_eyedropper() -> Option<String> { None }
    pub fn remove_tray_icon(_hwnd: *mut std::ffi::c_void) {}
    pub fn show_tray_context_menu(_hwnd: *mut std::ffi::c_void, _is_auto: bool) {}
    pub fn set_autostart(_enabled: bool) -> Result<(), String> { Ok(()) }
    pub fn extract_and_cache_icon(_target: &str, _cache: &Path) -> Option<PathBuf> { None }
    pub fn to_wide_null(_s: &str) -> Vec<u16> { Vec::new() }
    pub fn find_bar_hwnd() -> *mut std::ffi::c_void { std::ptr::null_mut() }
    pub fn setup_settings_window_styles(_hwnd: *mut std::ffi::c_void) {}
    pub fn set_drop_callback<F>(_cb: F) where F: Fn(Vec<String>, i32, i32, bool) + Send + Sync + 'static {}
}
