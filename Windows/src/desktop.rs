use std::{ffi::c_void, mem::size_of, ptr::{null, null_mut}};
use windows_sys::{core::BOOL, Win32::{Foundation::{HWND, LPARAM, POINT, RECT, SIZE}, Graphics::Gdi::*, UI::WindowsAndMessaging::{GetCursorPos, UpdateLayeredWindow, ULW_ALPHA}}};
use crate::geometry::{Edge, Point, Rect};

#[derive(Clone)]
pub struct Display { pub name: String, pub bounds: Rect }

#[derive(Clone)]
pub struct Region { pub display: String, pub local: Rect }

impl Region {
    pub fn area_on(&self, display: &Display) -> Option<Rect> {
        if display.name == self.display { self.local.fit(display.bounds) } else { None }
    }

    pub fn from_screen(display: &Display, area: Rect) -> Self {
        Self { display: display.name.clone(), local: area.offset(-display.bounds.left, -display.bounds.top) }
    }
}

pub fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain([0]).collect() }

pub fn string_from_wide(text: &[u16]) -> String {
    String::from_utf16_lossy(&text[..text.iter().position(|&unit| unit == 0).unwrap_or(text.len())])
}

pub fn displays() -> Vec<Display> {
    unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) } != 0 {
            let list = unsafe { &mut *(data as *mut Vec<Display>) };
            let rect = info.monitorInfo.rcMonitor;
            list.push(Display { name: string_from_wide(&info.szDevice), bounds: Rect::new(rect.left, rect.top, rect.right, rect.bottom) });
        }
        1
    }
    let mut list = Vec::new();
    unsafe { EnumDisplayMonitors(null_mut(), null(), Some(collect), &mut list as *mut _ as LPARAM) };
    list
}

pub fn cursor() -> Option<Point> {
    let mut point = POINT::default();
    (unsafe { GetCursorPos(&mut point) } != 0).then_some(Point { x: point.x, y: point.y })
}

pub struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    pixels: *mut u32,
    pub width: i32,
    pub height: i32,
}

impl Surface {
    pub fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 { return None; }
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() { return None; }
            let mut surface = Self { dc, bitmap: null_mut(), previous: null_mut(), pixels: null_mut(), width, height };
            let mut info = BITMAPINFO::default();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width;
            info.bmiHeader.biHeight = -height;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut c_void = null_mut();
            surface.bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if surface.bitmap.is_null() || bits.is_null() { return None; }
            surface.pixels = bits.cast();
            surface.previous = SelectObject(dc, surface.bitmap);
            if surface.previous.is_null() { return None; }
            Some(surface)
        }
    }

    pub fn fill(&mut self, rect: Rect, bgra: u32) {
        let clip = rect.intersection(Rect::new(0, 0, self.width, self.height));
        if clip.width() <= 0 || clip.height() <= 0 { return; }
        let pixels = unsafe { std::slice::from_raw_parts_mut(self.pixels, self.width as usize * self.height as usize) };
        for row in clip.top..clip.bottom {
            let base = (row * self.width) as usize;
            pixels[base + clip.left as usize..base + clip.right as usize].fill(bgra);
        }
    }

    pub fn present(&self, window: HWND, bounds: Rect) -> bool {
        let destination = POINT { x: bounds.left, y: bounds.top };
        let source = POINT::default();
        let size = SIZE { cx: self.width, cy: self.height };
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255,
                                     AlphaFormat: AC_SRC_ALPHA as u8 };
        (unsafe { UpdateLayeredWindow(window, null_mut(), &destination, &size, self.dc, &source, 0, &blend, ULW_ALPHA) } != 0)
    }

    pub fn capture(&mut self, area: Rect) -> bool {
        unsafe {
            let source = GetDC(null_mut());
            if source.is_null() { return false; }
            let copied = BitBlt(self.dc, 0, 0, self.width, self.height, source, area.left, area.top, SRCCOPY | CAPTUREBLT);
            GdiFlush();
            ReleaseDC(null_mut(), source);
            copied != 0
        }
    }

    pub fn bytes(&self) -> *const u8 { self.pixels.cast() }

    pub fn draw_outline(&mut self, area: Rect, arm: i32, weight: i32) {
        const COLOR: u32 = 0xffe2d8ff;
        let horizontal = |a: i32, b: i32, y: i32, w: i32| Rect::new(a.min(b), y - w, a.max(b), y + w);
        let vertical = |a: i32, b: i32, x: i32, w: i32| Rect::new(x - w, a.min(b), x + w, a.max(b));
        for y in [area.top, area.bottom] { self.fill(horizontal(area.left, area.right, y, 1), COLOR); }
        for x in [area.left, area.right] { self.fill(vertical(area.top, area.bottom, x, 1), COLOR); }
        if !area.is_selectable() { return; }

        let arm = arm.min(area.width() / 4).min(area.height() / 4);
        for edge in Edge::ALL {
            let (dx, dy) = edge.direction();
            let Point { x, y } = edge.anchor(area);
            if dx != 0 && dy != 0 {
                self.fill(horizontal(x - dx * arm, x, y, weight), COLOR);
                self.fill(vertical(y - dy * arm, y, x, weight), COLOR);
            } else if dy != 0 {
                self.fill(horizontal(x - arm, x + arm, y, weight), COLOR);
            } else {
                self.fill(vertical(y - arm, y + arm, x, weight), COLOR);
            }
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() { SelectObject(self.dc, self.previous); }
            if !self.bitmap.is_null() { DeleteObject(self.bitmap); }
            DeleteDC(self.dc);
        }
    }
}
