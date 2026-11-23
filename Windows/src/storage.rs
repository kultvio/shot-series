use std::{ffi::OsString, fs, io, os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}, ptr::{null, null_mut}};
use windows_sys::{core::GUID, w, Win32::{Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, HWND},
    Graphics::GdiPlus::{GdipCreateBitmapFromScan0, GdipDisposeImage, GdipSaveImageToFile, GdiplusShutdown,
        GdiplusStartup, GdiplusStartupInput, Ok as GDIP_OK}, Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH},
    System::{Com::{CoCreateGuid, CoTaskMemFree}, Registry::*},
    UI::{Input::KeyboardAndMouse::{VK_F1, VK_F8, VK_F24}, Shell::{BROWSEINFOW, BIF_NEWDIALOGSTYLE,
        BIF_RETURNONLYFSDIRS, GPFIDL_DEFAULT, SHBrowseForFolderW, SHGetPathFromIDListEx}}}};
use crate::{desktop::{wide, Region, Surface}, geometry::Rect};

const KEY: windows_sys::core::PCWSTR = w!("Software\\ShotSeries");
const SERIES_PREFIX: &str = "Серия ";

pub struct Settings {
    pub root: PathBuf,
    pub series: String,
    pub region: Option<Region>,
    pub key: u32,
    pub show: bool,
}

impl Settings {
    pub fn load() -> Self {
        let series = get(w!("Series")).and_then(|s| s.into_string().ok()).unwrap_or_default();
        let series = if Path::new(&series).file_name().is_some_and(|name| name == series.as_str()) {
            series
        } else { String::new() };
        let key = get(w!("HotKey")).and_then(|value| value.to_str()?.parse::<u32>().ok())
            .filter(|key| (VK_F1 as u32..=VK_F24 as u32).contains(key)).unwrap_or(VK_F8 as u32);
        Self {
            root: get(w!("Root")).map(PathBuf::from).unwrap_or_default(), series, key,
            region: get(w!("Region")).and_then(|value| decode_region(value.to_str()?)),
            show: get(w!("Overlay")).is_none_or(|value| value != "0"),
        }
    }

    pub fn save(&self) {
        put(w!("Root"), self.root.as_os_str().encode_wide().chain([0]).collect());
        put(w!("Series"), wide(&self.series));
        put(w!("Overlay"), wide(if self.show { "1" } else { "0" }));
        put(w!("HotKey"), wide(&self.key.to_string()));
        let region = self.region.as_ref().map(|region| {
            let rect = region.local;
            format!("{}:{}:{}:{}:{}", region.display, rect.left, rect.top, rect.right, rect.bottom)
        }).unwrap_or_default();
        put(w!("Region"), wide(&region));
    }

    pub fn destination(&self) -> Option<PathBuf> {
        (!self.root.as_os_str().is_empty() && !self.series.is_empty()).then(|| self.root.join(&self.series))
    }
}

fn decode_region(text: &str) -> Option<Region> {
    let mut fields = text.split(':');
    let display = fields.next()?.to_owned();
    let coordinates = fields.map(str::parse::<i32>).collect::<Result<Vec<_>, _>>().ok()?;
    let [left, top, right, bottom] = coordinates[..] else { return None };
    let local = Rect::new(left, top, right, bottom);
    (!display.is_empty() && local.is_selectable()).then_some(Region { display, local })
}

fn get(name: windows_sys::core::PCWSTR) -> Option<OsString> {
    let mut bytes = 0;
    unsafe {
        if RegGetValueW(HKEY_CURRENT_USER, KEY, name, RRF_RT_REG_SZ, null_mut(), null_mut(), &mut bytes) != 0 ||
            !(2..=65_536).contains(&bytes) || bytes % 2 != 0 { return None; }
        let mut value = vec![0u16; bytes as usize / 2];
        if RegGetValueW(HKEY_CURRENT_USER, KEY, name, RRF_RT_REG_SZ, null_mut(), value.as_mut_ptr().cast(), &mut bytes) != 0 {
            return None;
        }
        let end = value.iter().position(|&unit| unit == 0).unwrap_or(value.len());
        Some(OsString::from_wide(&value[..end]))
    }
}

fn put(name: windows_sys::core::PCWSTR, value: Vec<u16>) {
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, name, REG_SZ, value.as_ptr().cast(), (value.len() * 2) as u32) };
}

pub fn choose_root(owner: HWND) -> Option<PathBuf> {
    unsafe {
        let mut display = [0u16; 260];
        let browse = BROWSEINFOW { hwndOwner: owner, pszDisplayName: display.as_mut_ptr(),
            lpszTitle: w!("Выберите основной каталог для серий"), ulFlags: BIF_NEWDIALOGSTYLE | BIF_RETURNONLYFSDIRS,
            ..Default::default() };
        let id = SHBrowseForFolderW(&browse);
        if id.is_null() { return None; }
        let mut path = vec![0u16; 32768];
        let succeeded = SHGetPathFromIDListEx(id, path.as_mut_ptr(), path.len() as u32, GPFIDL_DEFAULT);
        CoTaskMemFree(id.cast());
        if succeeded == 0 { return None; }
        let length = path.iter().position(|&unit| unit == 0)?;
        (length > 0).then(|| PathBuf::from(OsString::from_wide(&path[..length])))
    }
}

pub fn subfolders(root: &Path) -> io::Result<Vec<String>> {
    if root.as_os_str().is_empty() { return Ok(Vec::new()); }
    let mut names = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Ok(name) = entry.file_name().into_string() { names.push(name); }
        }
    }
    names.sort_by_key(|name| (name.strip_prefix(SERIES_PREFIX).and_then(number).unwrap_or(u64::MAX), name.to_lowercase()));
    Ok(names)
}

fn number(text: &str) -> Option<u64> {
    if text.bytes().all(|c| c.is_ascii_digit()) { text.parse().ok() } else { None }
}

fn next(last: u64) -> io::Result<u64> {
    last.checked_add(1).ok_or_else(|| io::Error::other("Достигнут предел нумерации"))
}

pub fn new_series(root: &Path) -> io::Result<String> {
    let mut last = 0;
    for entry in fs::read_dir(root)? {
        if let Some(value) = entry?.file_name().to_str().and_then(|name| name.strip_prefix(SERIES_PREFIX)).and_then(number) {
            last = last.max(value);
        }
    }
    loop {
        last = next(last)?;
        let name = format!("{SERIES_PREFIX}{last}");
        match fs::create_dir(root.join(&name)) {
            Ok(()) => return Ok(name),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {},
            Err(error) => return Err(error),
        }
    }
}

pub struct ImageEncoder(usize);

impl ImageEncoder {
    pub fn new() -> Option<Self> {
        let mut token = 0;
        let settings = GdiplusStartupInput { GdiplusVersion: 1, ..Default::default() };
        (unsafe { GdiplusStartup(&mut token, &settings, null_mut()) } == GDIP_OK).then_some(Self(token))
    }
}

impl Drop for ImageEncoder {
    fn drop(&mut self) { unsafe { GdiplusShutdown(self.0) }; }
}

struct Staging(PathBuf);

impl Drop for Staging {
    fn drop(&mut self) { if !self.0.as_os_str().is_empty() { let _ = fs::remove_file(&self.0); } }
}

fn path_wide(path: &Path) -> Vec<u16> { path.as_os_str().encode_wide().chain([0]).collect() }

pub fn save_png(folder: &Path, surface: &Surface) -> io::Result<PathBuf> {
    let mut last = 0;
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("png")) {
            if let Some(value) = path.file_stem().and_then(|stem| stem.to_str()).and_then(number) { last = last.max(value); }
        }
    }
    let mut guid = GUID::default();
    if unsafe { CoCreateGuid(&mut guid) } < 0 { return Err(io::Error::other("Не удалось подготовить временный файл")); }
    let name = format!(".shotseries-{:08x}-{:04x}-{:04x}-{:016x}.tmp", guid.data1, guid.data2, guid.data3, u64::from_be_bytes(guid.data4));
    let mut staging = Staging(folder.join(name));
    let source = path_wide(&staging.0);
    write_image(&source, surface)?;
    loop {
        last = next(last)?;
        let filename = folder.join(format!("{last}.png"));
        if unsafe { MoveFileExW(source.as_ptr(), path_wide(&filename).as_ptr(), MOVEFILE_WRITE_THROUGH) } != 0 {
            staging.0.clear();
            return Ok(filename);
        }
        let error = io::Error::last_os_error();
        if !matches!(error.raw_os_error().map(|code| code as u32), Some(ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS)) {
            return Err(error);
        }
    }
}

fn write_image(path: &[u16], surface: &Surface) -> io::Result<()> {
    const RGB_32: i32 = 9 | (32 << 8) | 0x0002_0000;
    const PNG: GUID = GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);
    unsafe {
        let mut bitmap = null_mut();
        if GdipCreateBitmapFromScan0(surface.width, surface.height, surface.width * 4, RGB_32, surface.bytes(), &mut bitmap) != GDIP_OK {
            return Err(io::Error::other("Не удалось создать PNG"));
        }
        let status = GdipSaveImageToFile(bitmap.cast(), path.as_ptr(), &PNG, null());
        GdipDisposeImage(bitmap.cast());
        if status != GDIP_OK { return Err(io::Error::other("Не удалось записать PNG")); }
    }
    Ok(())
}
