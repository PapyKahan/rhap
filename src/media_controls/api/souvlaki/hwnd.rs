use std::ffi::c_void;
use std::path::PathBuf;

use anyhow::{Context, Result};
use windows::core::{w, Interface, BSTR, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, PROPERTYKEY, WPARAM};
use windows::Win32::System::Com::StructuredStorage::{
    PropVariantChangeType, PROPVARIANT, PROPVAR_CHANGE_FLAGS,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::IShellLinkW;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
    RegisterClassW, HMENU, MSG, PM_REMOVE, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
};

// ShellLink CLSID {00021401-0000-0000-C000-000000000046}
const CLSID_SHELL_LINK: windows::core::GUID = windows::core::GUID {
    data1: 0x00021401,
    data2: 0x0000,
    data3: 0x0000,
    data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

// PKEY_AppUserModel_ID {9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}, pid 5
const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID {
        data1: 0x9F4C2855,
        data2: 0x9F79,
        data3: 0x4B39,
        data4: [0xA8, 0xD0, 0xE1, 0xD4, 0x2D, 0xE1, 0xD5, 0xF3],
    },
    pid: 5,
};

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Temporary Start Menu shortcut so SMTC resolves "rhap" as the app name.
/// The shortcut's filename (minus .lnk) becomes the display name.
/// Automatically deleted when dropped.
struct StartMenuShortcut {
    path: PathBuf,
}

impl StartMenuShortcut {
    fn create() -> Result<Self> {
        let appdata = std::env::var("APPDATA").context("APPDATA not set")?;
        let shortcut_path = PathBuf::from(&appdata)
            .join(r"Microsoft\Windows\Start Menu\Programs")
            .join("rhap.lnk");

        let exe_path = std::env::current_exe().context("Cannot determine exe path")?;
        let exe_wide = to_wide_null(&exe_path.to_string_lossy());
        let lnk_wide = to_wide_null(&shortcut_path.to_string_lossy());

        unsafe {
            // COM must be initialized before CoCreateInstance.
            // Safe to call multiple times (returns S_FALSE if already init'd).
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let link: IShellLinkW =
                CoCreateInstance(&CLSID_SHELL_LINK, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(PCWSTR::from_raw(exe_wide.as_ptr()))?;

            // PKEY_AppUserModel_ID requires VT_LPWSTR, not VT_BSTR.
            // Create as VT_BSTR then convert to VT_LPWSTR via PropVariantChangeType.
            let bstr_value = PROPVARIANT::from(BSTR::from("rhap"));
            let mut lpwstr_value = PROPVARIANT::default();
            PropVariantChangeType(
                &mut lpwstr_value,
                &bstr_value,
                PROPVAR_CHANGE_FLAGS(0),
                VT_LPWSTR,
            )?;

            let store: IPropertyStore = link.cast()?;
            store.SetValue(&PKEY_APP_USER_MODEL_ID, &lpwstr_value)?;
            store.Commit()?;

            let persist: IPersistFile = link.cast()?;
            persist.Save(PCWSTR::from_raw(lnk_wide.as_ptr()), true)?;
        }

        log::warn!("Created SMTC shortcut at: {}", shortcut_path.display());

        Ok(Self {
            path: shortcut_path,
        })
    }
}

impl Drop for StartMenuShortcut {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

pub struct HiddenWindow {
    hwnd: HWND,
    _shortcut: Option<StartMenuShortcut>,
}

impl HiddenWindow {
    pub fn new() -> Result<Self> {
        let shortcut = StartMenuShortcut::create()
            .map_err(|e| log::warn!("Start Menu shortcut for SMTC failed: {}", e))
            .ok();

        unsafe {
            let class_name = w!("rhap_media_controls");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                lpszClassName: class_name,
                ..Default::default()
            };

            RegisterClassW(&wc);

            // Create a regular zero-size window (not HWND_MESSAGE).
            // SMTC's GetForWindow requires a real top-level window handle.
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("rhap"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                Some(HMENU::default()),
                None,
                None,
            )
            .context("Failed to create hidden window for SMTC")?;

            Ok(Self {
                hwnd,
                _shortcut: shortcut,
            })
        }
    }

    /// Drain pending Windows messages for this HWND.
    /// Must be called periodically so COM/SMTC events are dispatched.
    pub fn pump_messages(&self) {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, Some(self.hwnd), 0, 0, PM_REMOVE).as_bool() {
                let _ = DispatchMessageW(&msg);
            }
        }
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.hwnd.0 as *mut c_void
    }
}

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
        // _shortcut drops here, deleting the .lnk file
    }
}
