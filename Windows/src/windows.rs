use std::{cell::RefCell, ffi::c_void, mem::size_of, ptr::{null, null_mut}};
use windows_sys::{w, Win32::{Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT,
    RECT, WPARAM}, Graphics::{Dwm::DwmFlush, Gdi::*}, System::{Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
    LibraryLoader::GetModuleHandleW, Threading::CreateMutexW}, UI::{Controls::DRAWITEMSTRUCT, HiDpi::{AdjustWindowRectExForDpi,
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
    Input::KeyboardAndMouse::{EnableWindow, RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT, VK_F1}, Shell::*, WindowsAndMessaging::*}}};
use crate::{controls::{self, Controls, AREA_SIZE, CAPTURE, DIRECTORY_PATH, FOLDER, HEIGHT, KEY, NEW_SERIES, PICK, SERIES,
    STATUS, TOGGLE, WIDTH}, desktop::{cursor, Surface}, overlay::{Overlay, BEGIN_DRAG, CANCEL_SELECTION, DRAG_TIMER, ESCAPE_ID},
    storage::{self, ImageEncoder, Settings}};

const CLASS: windows_sys::core::PCWSTR = w!("ShotSeries.Controls");
const TITLE: windows_sys::core::PCWSTR = w!("ShotSeries");
const APP_ICON: windows_sys::core::PCWSTR = 1usize as _; // Resource ID from ShotSeries.rc
const TRAY: u32 = WM_APP + 3;
const TRAY_KEY_SELECT: u32 = NIN_SELECT | NINF_KEY;
const OPEN: u32 = WM_APP + 4;
const REVEAL: u16 = 17;
const QUIT: u16 = 18;
const STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
const EXTENDED: u32 = WS_EX_TOOLWINDOW;

struct App {
    window: HWND,
    controls: Option<Controls>,
    overlay: Option<Overlay>,
    settings: Settings,
    folders: Vec<String>,
    hotkey_id: i32,
    taskbar_message: u32,
    icon: HICON,
    status: String,
}

impl App {
    fn new() -> Self {
        let mut icon = unsafe { LoadIconW(GetModuleHandleW(null()), APP_ICON) };
        if icon.is_null() { icon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) }; }
        Self { window: null_mut(), controls: None, overlay: None, settings: Settings::load(),
               folders: Vec::new(), hotkey_id: 0, taskbar_message: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
               icon, status: String::new() }
    }

    fn initialize(&mut self, window: HWND) {
        self.window = window;
        controls::style_caption(window);
        self.controls = Some(Controls::new(window));
        self.overlay = Some(Overlay::new(window, self.settings.region.clone(), self.settings.show));
        unsafe {
            SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE);
            SendMessageW(window, WM_SETICON, ICON_SMALL as usize, self.icon as LPARAM);
        }
        self.bind_key(self.settings.key);
        self.add_tray();
        self.refresh_folders();
        if self.settings.destination().is_none() || self.area().is_none() { self.open_controls(); }
    }

    fn bind_key(&mut self, key: u32) {
        if self.hotkey_id != 0 && self.settings.key == key { return; }
        let candidate = if self.hotkey_id == 1 { 2 } else { 1 };
        if unsafe { RegisterHotKey(self.window, candidate, MOD_NOREPEAT, key) } == 0 {
            self.report(format!("Клавиша F{} занята. Выберите другую.", key - VK_F1 as u32 + 1));
            self.update();
            return;
        }
        if self.hotkey_id != 0 { unsafe { UnregisterHotKey(self.window, self.hotkey_id) }; }
        self.hotkey_id = candidate;
        self.settings.key = key;
        self.settings.save();
        self.report(String::new());
    }

    fn area(&self) -> Option<crate::geometry::Rect> { self.overlay.as_ref()?.available_area() }

    fn add_tray(&self) {
        let mut icon = self.tray_data();
        icon.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        icon.uCallbackMessage = TRAY;
        icon.hIcon = self.icon;
        Self::tray_tip(&mut icon, &self.status);
        unsafe {
            if Shell_NotifyIconW(NIM_ADD, &icon) != 0 {
                icon.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                Shell_NotifyIconW(NIM_SETVERSION, &icon);
            }
        }
    }

    fn tray_data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.window, uID: 1, ..Default::default() }
    }

    fn tray_tip(icon: &mut NOTIFYICONDATAW, status: &str) {
        let label = if status.is_empty() { "ShotSeries".to_owned() } else { format!("ShotSeries · {status}") };
        for (dst, src) in icon.szTip.iter_mut().take(127).zip(label.encode_utf16()) { *dst = src; }
    }

    fn open_controls(&mut self) {
        self.refresh_folders();
        let position = cursor().unwrap_or_default();
        let mut monitor = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe {
            let display = MonitorFromPoint(POINT { x: position.x, y: position.y }, MONITOR_DEFAULTTONEAREST);
            if GetMonitorInfoW(display, &mut monitor) != 0 {
                let work = monitor.rcWork;
                SetWindowPos(self.window, null_mut(), work.right - 30, work.bottom - 30, 0, 0, SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE);
                let dpi = GetDpiForWindow(self.window).max(96) as i32;
                let mut rect = RECT { left: 0, top: 0, right: WIDTH * dpi / 96, bottom: HEIGHT * dpi / 96 };
                AdjustWindowRectExForDpi(&mut rect, STYLE, 0, EXTENDED, dpi as u32);
                let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
                SetWindowPos(self.window, HWND_TOPMOST, (work.right - w - 16).max(work.left),
                    (work.bottom - h - 16).max(work.top), w, h, SWP_SHOWWINDOW);
            } else { ShowWindow(self.window, SW_SHOW); }
            SetForegroundWindow(self.window);
        }
        if let Some(controls) = &mut self.controls { controls.layout(); }
        self.update();
    }

    fn refresh_folders(&mut self) {
        match storage::subfolders(&self.settings.root) {
            Ok(folders) => {
                self.folders = folders;
                if !self.settings.series.is_empty() && !self.folders.contains(&self.settings.series) {
                    self.settings.series.clear();
                    self.settings.save();
                }
            }
            Err(error) => {
                self.folders.clear();
                self.report(format!("Каталог недоступен. {error}"));
            }
        }
        self.update();
    }

    fn update(&self) {
        let Some(ui) = &self.controls else { return };
        let area = self.area();
        let status = match area {
            Some(area) => format!("{} × {} пикселей", area.width(), area.height()),
            None => "Не выделена".to_owned(),
        };
        ui.set(AREA_SIZE, &status);
        let selecting = self.overlay.as_ref().is_some_and(|overlay| overlay.selecting);
        ui.set(PICK, if selecting { "Отменить выделение" } else if area.is_some() { "Выделить заново" } else { "Выделить область" });
        ui.set(TOGGLE, if self.settings.show { "Скрыть рамку" } else { "Показать рамку" });
        let root_chosen = !self.settings.root.as_os_str().is_empty();
        let root = self.settings.root.to_string_lossy();
        ui.set(DIRECTORY_PATH, if root_chosen { &root } else { "Выберите каталог" });
        ui.set(FOLDER, if root_chosen { "Сменить…" } else { "Выбрать…" });
        let selected = self.folders.iter().position(|name| name == &self.settings.series);
        let placeholder = if self.folders.is_empty() { "Создайте серию" } else { "Выберите серию" };
        ui.set(SERIES, selected.map_or(placeholder, |index| self.folders[index].as_str()));
        ui.set(KEY, &format!("F{}", self.settings.key - VK_F1 as u32 + 1));
        ui.set(STATUS, &self.status);
        unsafe {
            EnableWindow(ui.get(TOGGLE), (area.is_some() && !selecting) as i32);
            EnableWindow(ui.get(CAPTURE), (area.is_some() && selected.is_some() && !selecting) as i32);
            EnableWindow(ui.get(NEW_SERIES), root_chosen as i32);
            EnableWindow(ui.get(SERIES), (!self.folders.is_empty()) as i32);
        }
    }

    fn report(&mut self, status: String) {
        self.status = status;
        let mut icon = self.tray_data();
        icon.uFlags = NIF_TIP | NIF_SHOWTIP;
        Self::tray_tip(&mut icon, &self.status);
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &icon) };
        if let Some(ui) = &self.controls { ui.set(STATUS, &self.status); }
    }

    fn pick_region(&mut self) {
        let Some(overlay) = &mut self.overlay else { return };
        if overlay.selecting { overlay.cancel(); }
        else {
            unsafe { ShowWindow(self.window, SW_HIDE) };
            if !overlay.begin() {
                self.open_controls();
                self.report("Не удалось открыть выделение экрана.".to_owned());
            }
        }
        self.update();
    }

    fn sync_region(&mut self) {
        if let Some(overlay) = &self.overlay {
            self.settings.region = overlay.region.clone();
            self.settings.show = overlay.visible;
            self.settings.save();
        }
        self.update();
    }

    fn capture(&mut self) {
        if self.overlay.as_ref().is_some_and(|overlay| overlay.selecting) { return; }
        let Some(folder) = self.settings.destination() else {
            self.open_controls();
            self.report("Выберите каталог и добавьте серию через +.".to_owned());
            return;
        };
        let Some(area) = self.area() else { self.pick_region(); return };
        let Some(mut frame) = Surface::new(area.width(), area.height()) else {
            self.report("Не удалось выделить память для снимка.".to_owned());
            return;
        };

        unsafe { ShowWindow(self.window, SW_HIDE) };
        if let Some(overlay) = &self.overlay { overlay.visible_for_capture(false); }
        unsafe { DwmFlush() };
        let captured = frame.capture(area);
        if let Some(overlay) = &self.overlay { overlay.visible_for_capture(true); }
        if !captured {
            self.open_controls();
            self.report("Windows не удалось получить снимок экрана.".to_owned());
            return;
        }
        match storage::save_png(&folder, &frame) {
            Ok(file) => self.report(format!("Сохранено · {} / {}", self.settings.series,
                file.file_name().unwrap_or_default().to_string_lossy())),
            Err(error) => {
                self.open_controls();
                self.report(format!("Не удалось сохранить снимок. {error}"));
            }
        }
    }

    fn command(&mut self, id: u16) {
        match id {
            PICK => self.pick_region(),
            TOGGLE => {
                unsafe { ShowWindow(self.window, SW_HIDE) };
                if let Some(overlay) = &mut self.overlay { overlay.toggle(); }
                self.sync_region();
            }
            FOLDER => {
                if let Some(path) = storage::choose_root(self.window) {
                    if self.settings.root != path { self.settings.series.clear(); }
                    self.settings.root = path;
                    self.settings.save();
                    self.report(String::new());
                    self.refresh_folders();
                }
            }
            NEW_SERIES => {
                match storage::new_series(&self.settings.root) {
                    Ok(name) => { self.settings.series = name; self.settings.save(); self.report(String::new()); }
                    Err(error) => self.report(format!("Не удалось создать папку. {error}")),
                }
                self.refresh_folders();
            }
            SERIES => {
                if let Some(ui) = &self.controls {
                    let current = self.folders.iter().position(|name| name == &self.settings.series);
                    if let Some(name) = ui.choose(SERIES, &self.folders, current).and_then(|index| self.folders.get(index)) {
                        self.settings.series = name.clone();
                        self.settings.save();
                        self.report(String::new());
                        self.update();
                    }
                }
            }
            KEY => {
                if let Some(ui) = &self.controls {
                    let current = (self.settings.key - VK_F1 as u32) as usize;
                    let options: Vec<_> = (1..=24).map(|number| format!("F{number}")).collect();
                    if let Some(index) = ui.choose(KEY, &options, Some(current)) {
                        self.bind_key(VK_F1 as u32 + index as u32);
                        self.update();
                    }
                }
            }
            CAPTURE => self.capture(),
            REVEAL => {
                let folder = self.settings.destination().unwrap_or_else(|| self.settings.root.clone());
                if !folder.as_os_str().is_empty() {
                    use std::os::windows::ffi::OsStrExt;
                    let path: Vec<u16> = folder.as_os_str().encode_wide().chain([0]).collect();
                    unsafe { ShellExecuteW(self.window, w!("open"), path.as_ptr(), null(), null(), SW_SHOWNORMAL) };
                }
            }
            QUIT => unsafe { PostQuitMessage(0) },
            _ => {}
        }
    }

    fn tray_menu(&mut self) {
        let Some(position) = cursor() else { return };
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() { return; }
            AppendMenuW(menu, MF_STRING, OPEN as usize, w!("Открыть ShotSeries"));
            AppendMenuW(menu, MF_STRING, CAPTURE as usize, w!("Снять кадр"));
            AppendMenuW(menu, MF_STRING, REVEAL as usize, w!("Открыть папку снимков"));
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(menu, MF_STRING, QUIT as usize, w!("Выйти"));
            SetForegroundWindow(self.window);
            let command = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, position.x,
                position.y, 0, self.window, null());
            DestroyMenu(menu);
            PostMessageW(self.window, WM_NULL, 0, 0);
            if command == OPEN as i32 { self.open_controls(); }
            else if command != 0 { self.command(command as u16); }
        }
    }

    fn handle(&mut self, message: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
        match message {
            OPEN => self.open_controls(),
            BEGIN_DRAG => { if let Some(overlay) = &mut self.overlay { overlay.start_drag(w); } }
            CANCEL_SELECTION => { if let Some(overlay) = &mut self.overlay { overlay.cancel(); } self.update(); }
            WM_TIMER if w == DRAG_TIMER => {
                if self.overlay.as_mut().is_some_and(Overlay::tick) { self.sync_region(); }
            }
            WM_COMMAND if (w >> 16) as u32 == BN_CLICKED => self.command(w as u16),
            WM_HOTKEY if (w as i32) == ESCAPE_ID => {
                if let Some(overlay) = &mut self.overlay { overlay.cancel(); }
                self.update();
            }
            WM_HOTKEY if (w as i32) == self.hotkey_id => self.capture(),
            TRAY => match l as u16 as u32 {
                WM_LBUTTONUP | NIN_SELECT | TRAY_KEY_SELECT => self.open_controls(),
                WM_RBUTTONUP | WM_CONTEXTMENU => self.tray_menu(),
                _ => {}
            },
            WM_DISPLAYCHANGE => {
                if let Some(overlay) = &mut self.overlay { overlay.refresh(); }
                self.update();
            }
            WM_DPICHANGED => unsafe {
                let frame = &*(l as *const RECT);
                SetWindowPos(self.window, null_mut(), frame.left, frame.top, frame.right - frame.left,
                    frame.bottom - frame.top, SWP_NOZORDER | SWP_NOACTIVATE);
                if let Some(controls) = &mut self.controls { controls.layout(); }
            },
            WM_CLOSE => { unsafe { ShowWindow(self.window, SW_HIDE) }; }
            message if message != 0 && message == self.taskbar_message => self.add_tray(),
            _ => return None,
        }
        Some(0)
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let creation = &*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, creation.lpCreateParams as isize);
        }
        match message {
            WM_PAINT => { controls::paint_panel(window); return 0; }
            WM_ERASEBKGND => return 1,
            WM_DRAWITEM => {
                if let Some(item) = (l as *const DRAWITEMSTRUCT).as_ref() {
                    if controls::draw_control(item) { return 1; }
                }
            }
            _ => {}
        }
        let controller = GetWindowLongPtrW(window, GWLP_USERDATA) as *const RefCell<App>;
        if let Some(controller) = controller.as_ref() {
            if let Ok(mut app) = controller.try_borrow_mut() {
                if !app.window.is_null() {
                    if let Some(result) = app.handle(message, w, l) { return result; }
                }
            }
        }
        DefWindowProcW(window, message, w, l)
    }
}

pub fn run() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mutex = CreateMutexW(null(), 0, w!("Local\\ShotSeries.Preview"));
        if !mutex.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            let other = FindWindowW(CLASS, null());
            if !other.is_null() { PostMessageW(other, OPEN, 0, 0); }
            CloseHandle(mutex);
            return;
        }
        if CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) < 0 {
            MessageBoxW(null_mut(), w!("Не удалось запустить Windows-компоненты."), TITLE, MB_ICONERROR);
            if !mutex.is_null() { CloseHandle(mutex); }
            return;
        }
        if let Some(encoder) = ImageEncoder::new() {
            event_loop();
            drop(encoder);
        } else {
            MessageBoxW(null_mut(), w!("Не удалось запустить PNG-кодировщик."), TITLE, MB_ICONERROR);
        }
        CoUninitialize();
        if !mutex.is_null() { CloseHandle(mutex); }
    }
}

unsafe fn event_loop() {
    let controller = Box::new(RefCell::new(App::new()));
    unsafe {
        let instance = GetModuleHandleW(null());
        let class = WNDCLASSEXW { cbSize: size_of::<WNDCLASSEXW>() as u32, hInstance: instance, lpszClassName: CLASS,
            lpfnWndProc: Some(procedure),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW), hIcon: controller.borrow().icon, ..Default::default() };
        RegisterClassExW(&class);
        let context = (&*controller as *const RefCell<App>).cast::<c_void>();
        let window = CreateWindowExW(EXTENDED, CLASS, TITLE, STYLE, CW_USEDEFAULT, CW_USEDEFAULT, WIDTH, HEIGHT,
            null_mut(), null_mut(), instance, context);
        if window.is_null() {
            MessageBoxW(null_mut(), w!("Не удалось создать окно ShotSeries."), TITLE, MB_ICONERROR);
            return;
        }
        controller.borrow_mut().initialize(window);
        let mut message = MSG::default();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            if IsWindowVisible(window) != 0 && IsDialogMessageW(window, &message) != 0 { continue; }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let mut app = controller.borrow_mut();
        app.overlay.take();
        Shell_NotifyIconW(NIM_DELETE, &app.tray_data());
        if app.hotkey_id != 0 { UnregisterHotKey(window, app.hotkey_id); }
        DestroyWindow(window);
    }
}
