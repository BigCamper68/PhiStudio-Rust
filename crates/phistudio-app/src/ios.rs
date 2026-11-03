//! UIKit services for the unchanged prpr/Miniquad renderer.
use anyhow::{Context, Result, ensure};
use std::{
    cell::Cell,
    ffi::{CStr, CString, c_char, c_void},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

unsafe extern "C" {
    fn ps_initialize(controller: *mut c_void);
    fn ps_density() -> f32;
    fn ps_set_language(
        select_all: *const c_char,
        copy: *const c_char,
        paste: *const c_char,
        done: *const c_char,
    );
    fn ps_viewport_insets(insets: *mut f32);
    fn ps_show_keyboard(show: i32);
    fn ps_set_clipboard(text: *const c_char);
    fn ps_pick_document(kind: *const c_char, token: u64);
    fn ps_export_document(path: *const c_char, name: *const c_char, token: u64);
}

pub static WAS_SUSPENDED: AtomicBool = AtomicBool::new(false);
thread_local! { static KEYBOARD_LANGUAGE: Cell<Option<phistudio_core::Language>> = const { Cell::new(None) }; }

pub fn set_language(language: phistudio_core::Language) {
    if KEYBOARD_LANGUAGE.replace(Some(language)) != Some(language) {
        let _language = crate::i18n::Scope::enter(language);
        let labels = ["Select all", "Copy", "Paste", "Done"].map(|key| {
            CString::new(crate::i18n::tr(key)).expect("Catalogue labels contain no NUL")
        });
        unsafe {
            ps_set_language(
                labels[0].as_ptr(),
                labels[1].as_ptr(),
                labels[2].as_ptr(),
                labels[3].as_ptr(),
            );
        }
    }
}
static DOCUMENT_RESULTS: Mutex<Vec<DocumentResult>> = Mutex::new(Vec::new());
static INPUT: Mutex<Vec<egui::Event>> = Mutex::new(Vec::new());

pub struct DocumentResult {
    pub token: u64,
    pub path: Option<PathBuf>,
    pub name: String,
    pub error: Option<String>,
    pub canceled: bool,
}

pub fn initialize() -> Result<PathBuf> {
    let controller = *macroquad::miniquad::native::ios::VIEW_CTRL_OBJ
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    ensure!(controller != 0, "iOS view controller is not initialized");
    unsafe { ps_initialize(controller as *mut c_void) };
    let root = PathBuf::from(std::env::var_os("HOME").context("Missing iOS application home")?)
        .join("Documents/PhiStudio");
    std::fs::create_dir_all(&root)?;
    std::env::set_current_dir(&root)?;
    Ok(root)
}

pub fn density() -> f32 {
    unsafe { ps_density() }
}
/// Physical framebuffer insets, including only a docked software keyboard.
pub fn viewport_insets() -> [f32; 4] {
    let mut insets = [0.; 4];
    unsafe { ps_viewport_insets(insets.as_mut_ptr()) };
    insets
}
pub fn show_keyboard(show: bool) {
    unsafe { ps_show_keyboard(i32::from(show)) };
}
pub fn set_clipboard(text: &str) {
    if let Ok(text) = CString::new(text) {
        unsafe { ps_set_clipboard(text.as_ptr()) };
    }
}
pub fn input_events() -> Vec<egui::Event> {
    std::mem::take(&mut *INPUT.lock().unwrap_or_else(|e| e.into_inner()))
}
pub fn document_results() -> Vec<DocumentResult> {
    std::mem::take(&mut *DOCUMENT_RESULTS.lock().unwrap_or_else(|e| e.into_inner()))
}
pub fn request_package(token: u64) -> Result<()> {
    request_media("package", token)
}
pub fn request_media(kind: &str, token: u64) -> Result<()> {
    let kind = CString::new(kind)?;
    unsafe { ps_pick_document(kind.as_ptr(), token) };
    Ok(())
}
pub fn request_export_package(path: &Path, name: &str, token: u64) -> Result<()> {
    let path = CString::new(path.to_str().context("Non-UTF-8 export path")?)?;
    let name = CString::new(name)?;
    unsafe { ps_export_document(path.as_ptr(), name.as_ptr(), token) };
    Ok(())
}
pub use request_export_package as request_export_file;

// UIKit owns each string for the duration of the synchronous callback only.
unsafe fn string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn phistudio_ios_document(
    token: u64,
    path: *const c_char,
    name: *const c_char,
    error: *const c_char,
    canceled: i32,
) {
    DOCUMENT_RESULTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(DocumentResult {
            token,
            path: unsafe { string(path) }.map(PathBuf::from),
            name: unsafe { string(name) }.unwrap_or_default(),
            error: unsafe { string(error) },
            canceled: canceled != 0,
        });
}

#[unsafe(no_mangle)]
unsafe extern "C" fn phistudio_ios_input(action: i32, text: *const c_char) {
    let mut events = INPUT.lock().unwrap_or_else(|e| e.into_inner());
    match action {
        0 | 4 => {
            if let Some(text) = unsafe { string(text) } {
                events.push(if action == 4 {
                    egui::Event::Paste(text)
                } else {
                    egui::Event::Text(text)
                });
            }
        }
        1 | 2 | 5 => {
            let key = match action {
                1 => egui::Key::Backspace,
                2 => egui::Key::Enter,
                _ => egui::Key::A,
            };
            let modifiers = if action == 5 {
                egui::Modifiers {
                    command: true,
                    mac_cmd: true,
                    ..Default::default()
                }
            } else {
                egui::Modifiers::NONE
            };
            for pressed in [true, false] {
                events.push(egui::Event::Key {
                    key,
                    pressed,
                    repeat: false,
                    modifiers,
                });
            }
        }
        3 => events.push(egui::Event::Copy),
        6 => events.push(egui::Event::Cut),
        _ => {}
    }
}

#[unsafe(no_mangle)]
extern "C" fn phistudio_ios_suspend() {
    crate::video_encoder::cancel();
    phistudio_prpr::suspend_audio();
    WAS_SUSPENDED.store(true, Ordering::Release);
}

pub use request_export_package as request_export_video;
