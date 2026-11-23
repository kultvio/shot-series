use std::{ffi::c_void, mem::size_of, ptr::{null, null_mut}};
use windows_sys::{w, Win32::{
    Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::{Dwm::*, Gdi::*},
    System::{LibraryLoader::GetModuleHandleW, SystemServices::SS_OWNERDRAW},
    UI::{Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_HOTLIGHT, ODS_NOFOCUSRECT, ODS_SELECTED, ODT_BUTTON, ODT_STATIC, WM_MOUSELEAVE},
         HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::{TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE},
         Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass}, WindowsAndMessaging::*},
}};
use crate::desktop::{string_from_wide, wide};

pub const PICK: u16 = 10;
pub const TOGGLE: u16 = 11;
pub const FOLDER: u16 = 12;
pub const SERIES: u16 = 13;
pub const NEW_SERIES: u16 = 14;
pub const KEY: u16 = 15;
pub const CAPTURE: u16 = 16;
pub const AREA_SIZE: u16 = 20;
pub const DIRECTORY_PATH: u16 = 22;
pub const STATUS: u16 = 24;
const AREA_TITLE: u16 = 25;
const DIRECTORY_LABEL: u16 = 21;
const SERIES_LABEL: u16 = 23;

pub const WIDTH: i32 = 408;
pub const HEIGHT: i32 = 375;

const fn rgb(r: u32, g: u32, b: u32) -> COLORREF { r | (g << 8) | (b << 16) }

const BACKGROUND: COLORREF = rgb(22, 24, 35);
const CARD: COLORREF = rgb(32, 35, 49);
const FIELD: COLORREF = rgb(29, 32, 46);
const BUTTON: COLORREF = rgb(46, 49, 68);
const HOVER: COLORREF = rgb(56, 60, 82);
const PRESSED: COLORREF = rgb(39, 42, 59);
const BORDER: COLORREF = rgb(62, 66, 88);
const ACCENT: COLORREF = rgb(197, 182, 255);
const TEXT: COLORREF = rgb(245, 241, 255);
const TEXT_SUBTLE: COLORREF = rgb(185, 187, 209);
const TEXT_DISABLED: COLORREF = rgb(130, 133, 154);
const TEXT_ACCENT: COLORREF = rgb(30, 23, 53);
const TEXT_SUCCESS: COLORREF = rgb(148, 225, 185);
const HOT: windows_sys::core::PCWSTR = w!("ShotSeries.Hover");

struct Field { id: u16, handle: HWND, bounds: RECT }

pub struct Controls { fields: Vec<Field>, fonts: [HFONT; 3], window: HWND }

impl Controls {
    pub fn new(window: HWND) -> Self {
        let mut ui = Self { fields: Vec::new(), fonts: [null_mut(); 3], window };
        ui.label(AREA_TITLE, "Область снимка", (74, 24, 376, 45));
        ui.label(AREA_SIZE, "Не выделена", (74, 47, 376, 64));
        ui.button(PICK, "Выделить область", (28, 72, 240, 106));
        ui.button(TOGGLE, "Скрыть рамку", (247, 72, 380, 106));
        ui.label(DIRECTORY_LABEL, "Каталог для снимков", (20, 131, 388, 152));
        ui.label(DIRECTORY_PATH, "Выберите каталог", (20, 155, 262, 189));
        ui.button(FOLDER, "Выбрать…", (271, 155, 388, 189));
        ui.label(SERIES_LABEL, "Текущая серия", (20, 199, 388, 220));
        ui.button(SERIES, "Создайте серию", (20, 223, 337, 257));
        ui.button(NEW_SERIES, "Создать серию", (345, 223, 388, 257));
        ui.button(CAPTURE, "Снять кадр", (20, 284, 275, 325));
        ui.button(KEY, "F8", (283, 284, 388, 325));
        ui.label(STATUS, "", (20, 338, 388, 372));
        ui.layout();
        ui
    }

    fn label(&mut self, id: u16, value: &str, bounds: (i32, i32, i32, i32)) {
        self.add(id, w!("STATIC"), value, SS_OWNERDRAW, bounds);
    }

    fn button(&mut self, id: u16, value: &str, bounds: (i32, i32, i32, i32)) {
        self.add(id, w!("BUTTON"), value, WS_TABSTOP | BS_OWNERDRAW as u32, bounds);
        unsafe { SetWindowSubclass(self.get(id), Some(track_hover), 1, 0) };
    }

    fn add(&mut self, id: u16, class: windows_sys::core::PCWSTR, value: &str, style: u32,
           (left, top, right, bottom): (i32, i32, i32, i32)) {
        let handle = unsafe { CreateWindowExW(0, class, wide(value).as_ptr(), WS_CHILD | WS_VISIBLE | style,
            0, 0, 0, 0, self.window, id as usize as HMENU, GetModuleHandleW(null()), null()) };
        self.fields.push(Field { id, handle, bounds: RECT { left, top, right, bottom } });
    }

    pub fn get(&self, id: u16) -> HWND {
        self.fields.iter().find(|field| field.id == id).map_or(null_mut(), |field| field.handle)
    }

    pub fn set(&self, id: u16, value: &str) {
        let control = self.get(id);
        let value = wide(value);
        if window_text(control) != value {
            unsafe {
                SetWindowTextW(control, value.as_ptr());
                InvalidateRect(control, null(), 0);
            }
        }
    }

    pub fn layout(&mut self) {
        let scale = scaler(self.window);
        unsafe {
            let next_fonts = [font(scale(14), FW_NORMAL), font(scale(14), FW_SEMIBOLD), font(scale(17), FW_SEMIBOLD)];
            for field in &self.fields {
                let bounds = field.bounds;
                let role = match field.id { AREA_TITLE => 2, DIRECTORY_LABEL | SERIES_LABEL | CAPTURE => 1, _ => 0 };
                SendMessageW(field.handle, WM_SETFONT, next_fonts[role] as usize, 0);
                let left = scale(bounds.left);
                let top = scale(bounds.top);
                MoveWindow(field.handle, left, top, scale(bounds.right) - left, scale(bounds.bottom) - top, 1);
            }
            for previous in std::mem::replace(&mut self.fonts, next_fonts) {
                if !previous.is_null() { DeleteObject(previous); }
            }
            InvalidateRect(self.window, null(), 0);
        }
    }

    pub fn choose(&self, id: u16, labels: &[String], selected: Option<usize>) -> Option<usize> {
        if labels.is_empty() { return None; }
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() { return None; }
            for (index, label) in labels.iter().enumerate() {
                let escaped = wide(&label.replace('&', "&&"));
                let check = if selected == Some(index) { MF_CHECKED } else { MF_UNCHECKED };
                AppendMenuW(menu, MF_STRING | check, index + 1, escaped.as_ptr());
            }
            let mut anchor = RECT::default();
            let placed = GetWindowRect(self.get(id), &mut anchor) != 0;
            let parameters = TPMPARAMS { cbSize: size_of::<TPMPARAMS>() as u32, rcExclude: anchor };
            let selected = if placed {
                TrackPopupMenuEx(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_WORKAREA, anchor.left, anchor.bottom,
                                 self.window, &parameters)
            } else { 0 };
            DestroyMenu(menu);
            (selected > 0).then(|| (selected - 1) as usize)
        }
    }
}

impl Drop for Controls {
    fn drop(&mut self) {
        for font in self.fonts { if !font.is_null() { unsafe { DeleteObject(font) }; } }
    }
}

unsafe fn font(height: i32, weight: u32) -> HFONT {
    unsafe { CreateFontW(-height, 0, 0, 0, weight as i32, 0, 0, 0, DEFAULT_CHARSET as u32,
        0, 0, CLEARTYPE_QUALITY as u32, 0, w!("Segoe UI")) }
}

fn scaler(window: HWND) -> impl Fn(i32) -> i32 {
    let dpi = (unsafe { GetDpiForWindow(window) }).max(96) as i32;
    move |value| (value * dpi + 48) / 96
}

fn window_text(window: HWND) -> Vec<u16> {
    let count = (unsafe { GetWindowTextLengthW(window) }).max(0);
    let mut text = vec![0; count as usize + 1];
    unsafe { GetWindowTextW(window, text.as_mut_ptr(), count + 1) };
    text
}

pub fn style_caption(window: HWND) {
    let attributes = [(DWMWA_USE_IMMERSIVE_DARK_MODE, 1), (DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND as u32),
                      (DWMWA_CAPTION_COLOR, BACKGROUND), (DWMWA_TEXT_COLOR, TEXT), (DWMWA_BORDER_COLOR, BORDER)];
    for (name, value) in attributes {
        unsafe { DwmSetWindowAttribute(window, name as u32, (&value as *const u32).cast::<c_void>(), 4) };
    }
}

fn fill(dc: HDC, rect: RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        if !brush.is_null() { FillRect(dc, &rect, brush); DeleteObject(brush); }
    }
}

fn round(dc: HDC, rect: RECT, radius: i32, fill_color: COLORREF, border: COLORREF, weight: i32) {
    unsafe {
        let brush = CreateSolidBrush(fill_color);
        let pen = CreatePen(PS_SOLID, weight, border);
        if !brush.is_null() && !pen.is_null() {
            let previous_brush = SelectObject(dc, brush);
            let previous_pen = SelectObject(dc, pen);
            RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
            SelectObject(dc, previous_pen);
            SelectObject(dc, previous_brush);
        }
        if !pen.is_null() { DeleteObject(pen); }
        if !brush.is_null() { DeleteObject(brush); }
    }
}

pub fn paint_panel(window: HWND) {
    let scale = scaler(window);
    let rect = |left, top, right, bottom| RECT { left: scale(left), top: scale(top), right: scale(right), bottom: scale(bottom) };
    unsafe {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(window, &mut paint);
        fill(dc, paint.rcPaint, BACKGROUND);
        round(dc, rect(16, 12, 392, 118), scale(24), CARD, BORDER, 1);
        fill(dc, rect(20, 270, 388, 271), BORDER);
        let icon = SendMessageW(window, WM_GETICON, ICON_SMALL as usize, 0) as HICON;
        if !icon.is_null() { DrawIconEx(dc, scale(27), scale(22), icon, scale(35), scale(35), 0, null_mut(), DI_NORMAL); }
        EndPaint(window, &paint);
    }
}

pub fn draw_control(item: &DRAWITEMSTRUCT) -> bool {
    if item.CtlType != ODT_BUTTON && item.CtlType != ODT_STATIC { return false; }
    let id = item.CtlID as u16;
    let dc = item.hDC;
    let bounds = item.rcItem;
    let scale = scaler(item.hwndItem);
    let mut label = bounds;
    let value = window_text(item.hwndItem);
    let parent_background = match id { AREA_TITLE | AREA_SIZE | PICK | TOGGLE => CARD, _ => BACKGROUND };
    let mut color = TEXT;
    let mut flags = DT_SINGLELINE | DT_END_ELLIPSIS | DT_VCENTER | DT_NOPREFIX;
    fill(dc, bounds, parent_background);

    if item.CtlType == ODT_BUTTON {
        let disabled = item.itemState & ODS_DISABLED != 0;
        let down = item.itemState & ODS_SELECTED != 0;
        let focused = item.itemState & ODS_FOCUS != 0 && item.itemState & ODS_NOFOCUSRECT == 0;
        let hovered = item.itemState & ODS_HOTLIGHT != 0 || !unsafe { GetPropW(item.hwndItem, HOT) }.is_null();
        let background = match (id == CAPTURE, disabled, down, hovered) {
            (_, true, _, _) => FIELD,
            (true, false, true, _) => rgb(165, 147, 238),
            (true, false, false, true) => rgb(216, 203, 255),
            (true, false, false, false) => ACCENT,
            (false, false, true, _) => PRESSED,
            (false, false, false, true) => HOVER,
            _ => BUTTON,
        };
        color = if disabled { TEXT_DISABLED } else if id == CAPTURE { TEXT_ACCENT } else { TEXT };
        let outline = if focused { TEXT } else if id == CAPTURE && !disabled { background } else { BORDER };
        round(dc, bounds, scale(16), background, outline, if focused { scale(2) } else { 1 });

        if id == SERIES || id == KEY {
            label.left += scale(12);
            label.right -= scale(28);
            let (x, y) = (bounds.right - scale(15), (bounds.top + bounds.bottom) / 2);
            let size = scale(4);
            lines(dc, color, scale(1), &[(x - size, y - 1, x, y + size - 1), (x, y + size - 1, x + size + 1, y - 2)]);
        } else if id == NEW_SERIES {
            let (x, y) = ((bounds.left + bounds.right) / 2, (bounds.top + bounds.bottom) / 2);
            let delta = scale(5);
            lines(dc, color, scale(2), &[(x - delta, y, x + delta + 1, y), (x, y - delta, x, y + delta + 1)]);
            return true;
        } else {
            flags |= DT_CENTER;
        }
    } else {
        match id {
            DIRECTORY_PATH => {
                round(dc, bounds, scale(16), FIELD, BORDER, 1);
                label.left += scale(11);
                label.right -= scale(8);
                flags = DT_SINGLELINE | DT_PATH_ELLIPSIS | DT_VCENTER | DT_NOPREFIX;
                color = TEXT_SUBTLE;
            }
            AREA_SIZE => color = TEXT_SUBTLE,
            STATUS => {
                if string_from_wide(&value).starts_with("Сохранено") { color = TEXT_SUCCESS; }
                else { color = TEXT_SUBTLE; }
                flags = DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX;
            }
            _ => {}
        }
    }
    unsafe {
        let saved = SaveDC(dc);
        let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
        if !font.is_null() { SelectObject(dc, font); }
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        DrawTextW(dc, value.as_ptr(), -1, &mut label, flags);
        RestoreDC(dc, saved);
    }
    true
}

fn lines(dc: HDC, color: COLORREF, width: i32, segments: &[(i32, i32, i32, i32)]) {
    unsafe {
        let pen = CreatePen(PS_SOLID, width, color);
        if pen.is_null() { return; }
        let previous = SelectObject(dc, pen);
        for &(from_x, from_y, to_x, to_y) in segments {
            MoveToEx(dc, from_x, from_y, null_mut());
            LineTo(dc, to_x, to_y);
        }
        SelectObject(dc, previous);
        DeleteObject(pen);
    }
}

unsafe extern "system" fn track_hover(window: HWND, message: u32, w: WPARAM, l: LPARAM, id: usize, _: usize) -> LRESULT {
    unsafe {
        match message {
            WM_MOUSEMOVE if GetPropW(window, HOT).is_null() => {
                let mut tracking = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE, hwndTrack: window, dwHoverTime: 0 };
                if TrackMouseEvent(&mut tracking) != 0 {
                    SetPropW(window, HOT, 1usize as *mut c_void);
                    InvalidateRect(window, null(), 0);
                }
            }
            WM_MOUSELEAVE | WM_ENABLE => {
                RemovePropW(window, HOT);
                InvalidateRect(window, null(), 0);
            }
            WM_NCDESTROY => {
                RemovePropW(window, HOT);
                RemoveWindowSubclass(window, Some(track_hover), id);
            }
            _ => {}
        }
        DefSubclassProc(window, message, w, l)
    }
}
