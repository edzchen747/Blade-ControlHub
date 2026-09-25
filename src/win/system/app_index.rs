//! The list of applications the Keys page offers when binding a launch action.
//!
//! There are two kinds of application to find, because Windows stores them
//! differently. Ordinary desktop programs are `.lnk` shortcuts in the per-user
//! and machine-wide Start menu trees, each resolved to the executable it points
//! at; shortcuts resolving to the same target collapse into one entry, and the
//! uninstallers and help files that share those folders are left out.
//!
//! Packaged applications — Notepad, Terminal, Calculator, Settings and anything
//! from the Store — have no shortcut at all. They live only in the shell's
//! `AppsFolder` namespace, identified by an Application User Model ID, and are
//! started through `shell:AppsFolder\<id>` rather than by path.
//!
//! Icons travel to the window as base64 PNG `data:` URLs. The webview cannot
//! read from disk — the asset protocol is off and the CSP allows `data:` — and
//! an icon is a few hundred bytes, so inlining them costs less than a protocol
//! would.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tracing::{debug, warn};
use windows::Win32::Foundation::{HANDLE, HWND, SIZE};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HBITMAP, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_CommonPrograms, FOLDERID_Programs, IEnumShellItems, IShellItem,
    IShellItemImageFactory, IShellLinkW, KF_FLAG_DEFAULT, SHCreateItemFromParsingName, SHFILEINFOW,
    SHGetFileInfoW, SHGetKnownFolderPath, SHGFI_ICON, SHGFI_LARGEICON, SIGDN, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING, SIIGBF_ICONONLY, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::{GUID, Interface, PCWSTR};

/// Icons are requested at the shell's large size, which is 32x32 at 100% DPI.
const ICON_SIZE: u32 = 32;

/// Shortcut names that are almost never what someone wants on a hotkey.
const NOISE: &[&str] = &[
    "uninstall",
    "readme",
    "read me",
    "help",
    "release notes",
    "documentation",
    "license",
    "changelog",
    "website",
    "home page",
    "support",
];

/// One entry in the picker.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct IndexedApp {
    pub name: String,
    /// The shortcut itself, which is what gets launched: it carries the working
    /// directory and arguments the publisher intended.
    pub path: String,
    /// The executable the shortcut resolves to, or the model ID of a packaged
    /// application. Used to de-duplicate and to pull an icon from.
    pub target: String,
    /// A base64 PNG `data:` URL, or empty when the icon could not be read.
    pub icon: String,
    /// Whether this is a packaged application rather than a desktop program.
    /// The window shows a word instead of an unreadable model ID.
    pub packaged: bool,
}

static INDEX: OnceLock<Mutex<Option<Vec<IndexedApp>>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<Vec<IndexedApp>>> {
    INDEX.get_or_init(|| Mutex::new(None))
}

/// The indexed applications, building the index on first use. Blocking: call it
/// from a worker, never from the Tauri event loop.
pub fn list() -> Vec<IndexedApp> {
    let mut cached = cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(apps) = cached.as_ref() {
        return apps.clone();
    }

    let apps = build();
    *cached = Some(apps.clone());
    apps
}

/// Drops the cached index so the next [`list`] rebuilds it — for when the user
/// has installed something since the window opened.
pub fn refresh() -> Vec<IndexedApp> {
    let apps = build();
    *cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(apps.clone());
    apps
}

fn build() -> Vec<IndexedApp> {
    let _com = ComScope::enter();

    let mut by_target: HashMap<String, IndexedApp> = HashMap::new();
    let mut shortcuts = Vec::new();
    for root in [&FOLDERID_Programs, &FOLDERID_CommonPrograms] {
        if let Some(folder) = known_folder(root) {
            collect_shortcuts(&folder, 0, &mut shortcuts);
        }
    }

    for shortcut in shortcuts {
        let Some(name) = file_stem(&shortcut) else {
            continue;
        };
        if is_noise(&name) {
            continue;
        }
        let Some(target) = resolve_shortcut(&shortcut) else {
            continue;
        };
        if !Path::new(&target).is_file() {
            continue;
        }

        // The first shortcut to a target wins: the per-user Start menu is
        // walked first, and that is the one the user arranged.
        let key = target.to_lowercase();
        by_target.entry(key).or_insert_with(|| IndexedApp {
            name,
            path: shortcut.to_string_lossy().into_owned(),
            icon: icon_data_url(&target).unwrap_or_default(),
            target,
            packaged: false,
        });
    }

    for app in packaged_apps() {
        // Model IDs cannot collide with file paths, so the two kinds share one
        // map without either shadowing the other.
        by_target.entry(app.target.to_lowercase()).or_insert(app);
    }

    let mut apps: Vec<IndexedApp> = by_target.into_values().collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    debug!(count = apps.len(), "Indexed Start menu applications");
    apps
}

// ── Shortcut discovery ──────────────────────────────────────────────────────

/// Start menu trees are shallow; the depth cap only stops a symlink loop from
/// walking forever.
const MAX_DEPTH: usize = 6;

fn collect_shortcuts(folder: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => collect_shortcuts(&path, depth + 1, found),
            Ok(_) => {
                if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
                {
                    found.push(path);
                }
            }
            Err(_) => {}
        }
    }
}

fn known_folder(id: *const GUID) -> Option<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, HANDLE::default()) }.ok()?;
    let folder = unsafe { path.to_string() }.ok().map(PathBuf::from);
    unsafe { CoTaskMemFree(Some(path.0 as *const _)) };
    folder
}

fn resolve_shortcut(shortcut: &Path) -> Option<String> {
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let file: IPersistFile = link.cast().ok()?;

    let wide = wide(&shortcut.to_string_lossy());
    unsafe { file.Load(PCWSTR(wide.as_ptr()), STGM_READ) }.ok()?;

    let mut buffer = [0u16; 260];
    unsafe { link.GetPath(&mut buffer, std::ptr::null_mut(), 0) }.ok()?;

    let target = String::from_utf16_lossy(&buffer[..nul(&buffer)]);
    (!target.trim().is_empty()).then_some(target)
}

fn is_noise(name: &str) -> bool {
    let lowered = name.to_lowercase();
    NOISE.iter().any(|noise| lowered.contains(noise))
}

fn file_stem(path: &Path) -> Option<String> {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
}

// ── Packaged applications ───────────────────────────────────────────────────

/// Enumerates the shell's `AppsFolder`, keeping only packaged applications.
///
/// That folder lists desktop programs too, but those are already indexed from
/// their shortcuts, where the real executable is reachable. A packaged entry is
/// told apart by its parsing name being a model ID — `Publisher.Name_hash!App`,
/// which has a `!` and no path separator — rather than a file path.
fn packaged_apps() -> Vec<IndexedApp> {
    let path = wide("shell:AppsFolder");
    let folder: IShellItem =
        match unsafe { SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None) } {
            Ok(folder) => folder,
            Err(error) => {
                warn!(%error, "Could not open the shell applications folder");
                return Vec::new();
            }
        };

    let items: IEnumShellItems = match unsafe { folder.BindToHandler(None, &BHID_EnumItems) } {
        Ok(items) => items,
        Err(error) => {
            warn!(%error, "Could not enumerate the shell applications folder");
            return Vec::new();
        }
    };

    let mut apps = Vec::new();
    loop {
        let mut fetched: [Option<IShellItem>; 1] = [None];
        let mut count = 0u32;
        if unsafe { items.Next(&mut fetched, Some(&mut count)) }.is_err() || count == 0 {
            break;
        }
        let Some(item) = fetched[0].take() else {
            break;
        };

        let Some(model_id) = display_name(&item, SIGDN_PARENTRELATIVEPARSING) else {
            continue;
        };
        if !is_model_id(&model_id) {
            continue;
        }
        let Some(name) = display_name(&item, SIGDN_NORMALDISPLAY) else {
            continue;
        };
        if is_noise(&name) {
            continue;
        }

        apps.push(IndexedApp {
            name,
            path: format!("shell:AppsFolder\\{model_id}"),
            icon: shell_item_icon(&item).unwrap_or_default(),
            target: model_id,
            packaged: true,
        });
    }
    apps
}

/// A model ID looks like `Publisher.Name_hash!Entry`; a file path does not.
fn is_model_id(parsing_name: &str) -> bool {
    parsing_name.contains('!')
        && !parsing_name.contains('\\')
        && !parsing_name.contains('/')
}

fn display_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
    let name = unsafe { item.GetDisplayName(kind) }.ok()?;
    let text = unsafe { name.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(name.0 as *const _)) };
    text.filter(|text| !text.trim().is_empty())
}

/// A packaged application has no file to read an icon from, so the shell is
/// asked to render one for the item itself.
fn shell_item_icon(item: &IShellItem) -> Option<String> {
    let factory: IShellItemImageFactory = item.cast().ok()?;
    let size = SIZE {
        cx: ICON_SIZE as i32,
        cy: ICON_SIZE as i32,
    };
    let bitmap = unsafe { factory.GetImage(size, SIIGBF_ICONONLY) }.ok()?;

    // The shell renders these already premultiplied, unlike an icon's own
    // colour bitmap, so they must not be premultiplied a second time.
    let pixels = bitmap_pixels(bitmap, false);
    let _ = unsafe { DeleteObject(HGDIOBJ::from(bitmap)) };
    encode_icon(&pixels?)
}

// ── Icons ───────────────────────────────────────────────────────────────────

/// Reads the file's shell icon and encodes it as a `data:` URL.
fn icon_data_url(target: &str) -> Option<String> {
    let icon = shell_icon(target)?;
    let pixels = icon_pixels(icon);
    let _ = unsafe { DestroyIcon(icon) };
    encode_icon(&pixels?)
}

/// Premultiplied RGBA at [`ICON_SIZE`] to a base64 PNG `data:` URL.
fn encode_icon(pixels: &[u8]) -> Option<String> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE)?;
    pixmap.data_mut().copy_from_slice(pixels);

    let png = pixmap.encode_png().ok()?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

fn shell_icon(target: &str) -> Option<HICON> {
    let path = wide(target);
    let mut info = SHFILEINFOW::default();
    let result = unsafe {
        SHGetFileInfoW(
            PCWSTR(path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    if result == 0 || info.hIcon.is_invalid() {
        return None;
    }
    Some(info.hIcon)
}

/// Converts an `HICON` to premultiplied RGBA at [`ICON_SIZE`].
fn icon_pixels(icon: HICON) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;

    // An icon's colour bitmap carries straight alpha, so it is premultiplied on
    // the way out.
    let result = bitmap_pixels(info.hbmColor, true);

    for bitmap in [info.hbmColor, info.hbmMask] {
        if !bitmap.is_invalid() {
            let _ = unsafe { DeleteObject(HGDIOBJ::from(bitmap)) };
        }
    }
    result
}

/// Reads a GDI bitmap as RGBA at [`ICON_SIZE`].
///
/// `GetDIBits` hands back bottom-up BGRA, and an image whose alpha channel is
/// entirely zero (a 24-bit icon) would come back fully transparent, so that case
/// is forced opaque rather than rendered as nothing.
fn bitmap_pixels(colour: HBITMAP, premultiply: bool) -> Option<Vec<u8>> {
    if colour.is_invalid() {
        return None;
    }

    {
        let mut bitmap = BITMAP::default();
        let written = unsafe {
            GetObjectW(
                HGDIOBJ::from(colour),
                std::mem::size_of::<BITMAP>() as i32,
                Some((&raw mut bitmap).cast()),
            )
        };
        if written == 0 || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
            return None;
        }

        let width = bitmap.bmWidth as u32;
        let height = bitmap.bmHeight as u32;
        let mut buffer = vec![0u8; (width * height * 4) as usize];

        let mut header = BITMAPINFO::default();
        header.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        header.bmiHeader.biWidth = bitmap.bmWidth;
        // Negative height asks for a top-down buffer, sparing us a flip.
        header.bmiHeader.biHeight = -bitmap.bmHeight;
        header.bmiHeader.biPlanes = 1;
        header.bmiHeader.biBitCount = 32;
        header.bmiHeader.biCompression = BI_RGB.0;

        let screen = unsafe { GetDC(HWND::default()) };
        let rows = unsafe {
            GetDIBits(
                screen,
                colour,
                0,
                height,
                Some(buffer.as_mut_ptr().cast()),
                &mut header,
                DIB_RGB_COLORS,
            )
        };
        unsafe { ReleaseDC(HWND::default(), screen) };
        if rows == 0 {
            return None;
        }

        bgra_to_rgba(&mut buffer, premultiply);
        Some(scale_to_icon_size(&buffer, width, height))
    }
}

/// In place: BGRA to RGBA, treating a wholly zero alpha channel as "this image
/// has no alpha" rather than "this image is invisible". `premultiply` is for
/// sources carrying straight alpha; an image the shell already premultiplied
/// must be left alone or it darkens.
fn bgra_to_rgba(buffer: &mut [u8], premultiply: bool) {
    let has_alpha = buffer.chunks_exact(4).any(|pixel| pixel[3] != 0);

    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if !has_alpha {
            pixel[3] = 0xff;
            continue;
        }
        if !premultiply {
            continue;
        }
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * alpha) / 255) as u8;
        }
    }
}

/// Nearest-neighbour resize. The shell almost always returns exactly the size
/// asked for, so this is a fallback for high-DPI shells, not a quality path.
fn scale_to_icon_size(buffer: &[u8], width: u32, height: u32) -> Vec<u8> {
    if width == ICON_SIZE && height == ICON_SIZE {
        return buffer.to_vec();
    }

    let mut scaled = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let source_x = x * width / ICON_SIZE;
            let source_y = y * height / ICON_SIZE;
            let from = ((source_y * width + source_x) * 4) as usize;
            let to = ((y * ICON_SIZE + x) * 4) as usize;
            scaled[to..to + 4].copy_from_slice(&buffer[from..from + 4]);
        }
    }
    scaled
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// COM has to be initialised on whichever thread resolves shortcuts. A thread
/// that is already in a different apartment is left alone.
struct ComScope {
    owned: bool,
}

impl ComScope {
    fn enter() -> Self {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        Self {
            owned: result.is_ok(),
        }
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.owned {
            unsafe { CoUninitialize() };
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn nul(buffer: &[u16]) -> usize {
    buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len())
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut block = [0u8; 3];
        block[..chunk.len()].copy_from_slice(chunk);
        let packed = (u32::from(block[0]) << 16) | (u32::from(block[1]) << 8) | u32::from(block[2]);

        for index in 0..4 {
            if index <= chunk.len() {
                let shift = 18 - index * 6;
                encoded.push(ALPHABET[((packed >> shift) & 0x3f) as usize] as char);
            } else {
                encoded.push('=');
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xef, 0xfe]), "/+/+");
    }

    #[test]
    fn uninstallers_and_help_links_are_left_out() {
        assert!(is_noise("Uninstall Thing"));
        assert!(is_noise("Thing Help"));
        assert!(is_noise("README"));
        assert!(!is_noise("Thing"));
        assert!(!is_noise("Visual Studio Code"));
    }

    #[test]
    fn bgra_becomes_rgba() {
        let mut buffer = vec![0x10, 0x20, 0x30, 0xff];

        bgra_to_rgba(&mut buffer, true);

        assert_eq!(buffer, vec![0x30, 0x20, 0x10, 0xff]);
    }

    #[test]
    fn straight_alpha_is_premultiplied_but_shell_output_is_left_alone() {
        let half = vec![0x80, 0x80, 0x80, 0x80];

        let mut straight = half.clone();
        bgra_to_rgba(&mut straight, true);
        assert_eq!(straight, vec![0x40, 0x40, 0x40, 0x80]);

        // The shell hands back premultiplied pixels; premultiplying again would
        // darken every packaged application's icon.
        let mut already = half;
        bgra_to_rgba(&mut already, false);
        assert_eq!(already, vec![0x80, 0x80, 0x80, 0x80]);
    }

    #[test]
    fn an_icon_with_no_alpha_channel_is_read_as_opaque() {
        for premultiply in [true, false] {
            let mut buffer = vec![0x10, 0x20, 0x30, 0x00];

            bgra_to_rgba(&mut buffer, premultiply);

            assert_eq!(
                buffer,
                vec![0x30, 0x20, 0x10, 0xff],
                "a 24-bit icon must not come out fully transparent"
            );
        }
    }

    #[test]
    fn a_model_id_is_told_apart_from_a_file_path() {
        assert!(is_model_id("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"));
        assert!(is_model_id(
            "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel"
        ));
        assert!(!is_model_id(
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Thing.lnk"
        ));
        assert!(!is_model_id("C:/Program Files/Thing/thing.exe"));
        assert!(!is_model_id("thing.exe"));
        assert!(!is_model_id(""));
    }

    /// Needs a real Start menu, so it is not part of the default run:
    /// `cargo test -- --ignored app_index`.
    #[test]
    #[ignore = "reads the machine's Start menu"]
    fn the_start_menu_indexes_to_real_applications() {
        let apps = build();

        assert!(!apps.is_empty(), "no Start menu shortcuts resolved");
        assert!(
            apps.iter().any(|app| !app.icon.is_empty()),
            "no application produced an icon"
        );
        let packaged: Vec<_> = apps.iter().filter(|app| app.packaged).collect();
        assert!(
            !packaged.is_empty(),
            "no packaged applications were found; Notepad and Terminal have no shortcut"
        );
        assert!(
            packaged.iter().any(|app| !app.icon.is_empty()),
            "no packaged application produced an icon"
        );
        // These ship with Windows and have no shortcut at all, so they are the
        // regression: finding them means the AppsFolder pass is working.
        for wanted in ["Notepad", "Terminal", "Paint"] {
            assert!(
                apps.iter().any(|app| app.name == wanted && app.packaged),
                "{wanted} was not indexed"
            );
        }

        for app in apps.iter().filter(|app| app.packaged).take(5) {
            println!("[packaged] {} -> {}", app.name, app.path);
        }
    }

    #[test]
    fn scaling_keeps_an_already_correct_icon_untouched() {
        let buffer = vec![0x7f; (ICON_SIZE * ICON_SIZE * 4) as usize];

        assert_eq!(scale_to_icon_size(&buffer, ICON_SIZE, ICON_SIZE), buffer);
    }
}
