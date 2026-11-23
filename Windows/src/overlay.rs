use std::{mem::size_of, ptr::{null, null_mut}};
use windows_sys::{w, Win32::{Foundation::{HWND, LPARAM, LRESULT, WPARAM}, Graphics::Gdi::{GetStockObject, BLACK_BRUSH}, System::LibraryLoader::GetModuleHandleW, UI::{HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::{GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT, VK_ESCAPE, VK_LBUTTON}, WindowsAndMessaging::*}}};
use crate::{desktop::{cursor, displays, Display, Region, Surface}, geometry::{Edge, Point, Rect}};

const CLASS: windows_sys::core::PCWSTR = w!("ShotSeries.Overlay");
const SHADE: u32 = 0x45000000;
const FIRST_HANDLE: usize = usize::MAX - Edge::ALL.len();
pub const BEGIN_DRAG: u32 = WM_APP + 1;
pub const CANCEL_SELECTION: u32 = WM_APP + 2;
pub const DRAG_TIMER: usize = 1;
pub const ESCAPE_ID: i32 = 3;

struct MouseTarget { receiver: HWND, id: usize }

struct Panel {
    window: HWND,
    display: Display,
    surface: Option<Surface>,
    _mouse: Box<MouseTarget>,
}

impl Panel {
    fn new(receiver: HWND, display: Display, id: usize, click_through: bool) -> Option<Self> {
        let handle = id >= FIRST_HANDLE;
        let bounds = display.bounds;
        let mut mouse = Box::new(MouseTarget { receiver, id });
        let mut style = WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
        if click_through { style |= WS_EX_TRANSPARENT; }
        unsafe {
            let window = CreateWindowExW(style, CLASS, w!(""), WS_POPUP, bounds.left, bounds.top,
                if handle { 1 } else { bounds.width() }, if handle { 1 } else { bounds.height() },
                null_mut(), null_mut(), GetModuleHandleW(null()), (&mut *mouse as *mut MouseTarget).cast());
            if window.is_null() { return None; }
            let mut panel = Self { window, display, surface: None, _mouse: mouse };
            SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE);
            if handle {
                if SetLayeredWindowAttributes(window, 0, 8, LWA_ALPHA) == 0 { return None; }
            } else {
                panel.surface = Some(Surface::new(bounds.width(), bounds.height())?);
            }
            Some(panel)
        }
    }

    fn draw(&mut self, region: Option<Rect>) -> bool {
        let Some(surface) = &mut self.surface else { return false };
        surface.fill(Rect::new(0, 0, surface.width, surface.height), SHADE);
        let bounds = self.display.bounds;
        if let Some(area) = region {
            let local = area.offset(-bounds.left, -bounds.top);
            surface.fill(local, 0);
            let dpi = unsafe { GetDpiForWindow(self.window) }.max(96) as i32;
            surface.draw_outline(local, 10 * dpi / 96, 2 * dpi / 96);
        }
        surface.present(self.window, bounds)
    }

    fn visible(&self, visible: bool) {
        unsafe { ShowWindow(self.window, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
    }
}

impl Drop for Panel {
    fn drop(&mut self) { unsafe { DestroyWindow(self.window) }; }
}

struct Drag { display: Display, start: Point, previous: Point, edit: Option<(Edge, Rect)> }

pub struct Overlay {
    receiver: HWND,
    shades: Vec<Panel>,
    handles: Vec<(Edge, Panel)>,
    drag: Option<Drag>,
    pub region: Option<Region>,
    pub visible: bool,
    pub selecting: bool,
}

impl Overlay {
    pub fn new(receiver: HWND, region: Option<Region>, visible: bool) -> Self {
        unsafe {
            let class = WNDCLASSEXW { cbSize: size_of::<WNDCLASSEXW>() as u32,
                hInstance: GetModuleHandleW(null()), lpszClassName: CLASS, lpfnWndProc: Some(panel_proc),
                hbrBackground: GetStockObject(BLACK_BRUSH).cast(), ..Default::default() };
            RegisterClassExW(&class);
        }
        let mut overlay = Self { receiver, shades: Vec::new(), handles: Vec::new(), drag: None,
                                 region, visible, selecting: false };
        overlay.show_region();
        overlay
    }

    pub fn begin(&mut self) -> bool {
        self.clear();
        for display in displays() {
            if let Some(mut panel) = Panel::new(self.receiver, display, self.shades.len(), false) {
                if panel.draw(None) {
                    panel.visible(true);
                    self.shades.push(panel);
                }
            }
        }
        self.selecting = !self.shades.is_empty();
        if self.selecting {
            unsafe { RegisterHotKey(self.receiver, ESCAPE_ID, MOD_NOREPEAT, VK_ESCAPE as u32) };
        } else {
            self.show_region();
        }
        self.selecting
    }

    pub fn cancel(&mut self) {
        if self.selecting {
            self.selecting = false;
            unsafe { UnregisterHotKey(self.receiver, ESCAPE_ID) };
            self.show_region();
        }
    }

    pub fn refresh(&mut self) {
        if self.selecting { self.cancel(); } else { self.show_region(); }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        self.show_region();
    }

    pub fn visible_for_capture(&self, shown: bool) {
        for panel in &self.shades { panel.visible(shown); }
        for (_, panel) in &self.handles { panel.visible(shown); }
    }

    pub fn available_area(&self) -> Option<Rect> {
        let region = self.region.as_ref()?;
        displays().iter().find_map(|display| region.area_on(display))
    }

    pub fn start_drag(&mut self, id: usize) {
        if self.drag.is_some() { return; }
        let Some(point) = cursor() else { return };
        let (display, edit) = if self.selecting {
            let Some(panel) = self.shades.get(id) else { return };
            (panel.display.clone(), None)
        } else {
            let Some(edge) = id.checked_sub(FIRST_HANDLE).and_then(|index| Edge::ALL.get(index)).copied() else { return };
            let Some(shade) = self.shades.first() else { return };
            let Some(original) = self.region.as_ref().and_then(|region| region.area_on(&shade.display)) else { return };
            (shade.display.clone(), Some((edge, original)))
        };
        let start = point.clamped(display.bounds);
        self.drag = Some(Drag { display, start, previous: point, edit });
        if unsafe { SetTimer(self.receiver, DRAG_TIMER, 15, None) } == 0 { self.drag = None; }
    }

    pub fn tick(&mut self) -> bool {
        let Some(drag) = &mut self.drag else { return false };
        let Some(point) = cursor() else { return false };
        let released = (unsafe { GetAsyncKeyState(VK_LBUTTON as i32) }) >= 0;
        if !released && point == drag.previous { return false; }
        drag.previous = point;
        let bounds = drag.display.bounds;
        let area = if let Some((edge, original)) = drag.edit {
            edge.resize(original, Point { x: point.x - drag.start.x, y: point.y - drag.start.y }, bounds)
        } else {
            Rect::between(drag.start, point, bounds)
        };

        let editing = drag.edit.is_some();
        let display = drag.display.clone();
        for panel in &mut self.shades {
            if panel.display.name == display.name {
                panel.draw(if !editing && released && !area.is_selectable() { None } else { Some(area) });
            }
        }
        if editing {
            self.move_handles(area, bounds);
            self.region = Some(Region::from_screen(&display, area));
        }
        if !released { return false; }
        unsafe { KillTimer(self.receiver, DRAG_TIMER) };
        self.drag = None;
        if !editing && area.is_selectable() {
            self.region = Some(Region::from_screen(&display, area));
            self.visible = true;
            self.cancel();
            return true;
        }
        editing
    }

    fn move_handles(&self, area: Rect, monitor: Rect) {
        let Some(shade) = self.shades.first() else { return };
        let dpi = unsafe { GetDpiForWindow(shade.window) }.max(96) as i32;
        for (edge, handle) in &self.handles {
            let frame = edge.hit_rect(area, monitor, 10 * dpi / 96);
            unsafe { SetWindowPos(handle.window, HWND_TOPMOST, frame.left, frame.top, frame.width(), frame.height(),
                                    SWP_NOACTIVATE | SWP_SHOWWINDOW) };
        }
    }

    fn show_region(&mut self) {
        self.clear();
        if !self.visible { return; }
        let Some(region) = &self.region else { return };
        for display in displays() {
            let Some(area) = region.area_on(&display) else { continue };
            let Some(mut shade) = Panel::new(self.receiver, display.clone(), 0, true) else { return };
            if !shade.draw(Some(area)) { return; }
            shade.visible(true);
            self.shades.push(shade);
            for (index, edge) in Edge::ALL.into_iter().enumerate() {
                if let Some(handle) = Panel::new(self.receiver, display.clone(), FIRST_HANDLE + index, false) {
                    self.handles.push((edge, handle));
                }
            }
            self.move_handles(area, display.bounds);
            break;
        }
    }

    fn clear(&mut self) {
        if self.drag.take().is_some() { unsafe { KillTimer(self.receiver, DRAG_TIMER) }; }
        self.handles.clear();
        self.shades.clear();
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        if self.selecting { unsafe { UnregisterHotKey(self.receiver, ESCAPE_ID) }; }
        self.clear();
    }
}

fn handle_cursor(id: usize) -> windows_sys::core::PCWSTR {
    let edge = id.checked_sub(FIRST_HANDLE).and_then(|index| Edge::ALL.get(index));
    match edge {
        Some(Edge::TopLeft | Edge::BottomRight) => IDC_SIZENWSE,
        Some(Edge::TopRight | Edge::BottomLeft) => IDC_SIZENESW,
        Some(Edge::Top | Edge::Bottom) => IDC_SIZENS,
        Some(Edge::Left | Edge::Right) => IDC_SIZEWE,
        None => IDC_CROSS,
    }
}

unsafe extern "system" fn panel_proc(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let params = &*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, params.lpCreateParams as isize);
        }
        let mouse = GetWindowLongPtrW(window, GWLP_USERDATA) as *const MouseTarget;
        if let Some(mouse) = mouse.as_ref() {
            match message {
                WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
                WM_SETCURSOR => { SetCursor(LoadCursorW(null_mut(), handle_cursor(mouse.id))); return 1; }
                WM_LBUTTONDOWN => { PostMessageW(mouse.receiver, BEGIN_DRAG, mouse.id, 0); return 0; }
                WM_RBUTTONDOWN => { PostMessageW(mouse.receiver, CANCEL_SELECTION, 0, 0); return 0; }
                _ => {}
            }
        }
        DefWindowProcW(window, message, w, l)
    }
}
