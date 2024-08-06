use anyhow::Result;
use windows::{core::*, Win32::Foundation::*, Win32::UI::WindowsAndMessaging::*, Win32::System::LibraryLoader::*, Win32::UI::Input::KeyboardAndMouse::*};
pub struct MediaKeysHandler {
    _hwnd: HWND,
}

impl MediaKeysHandler {
    pub(crate) fn new() -> Result<Self> {
        unsafe {
            // Register the window class
            let h_instance = GetModuleHandleW(None)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: h_instance.into(),
                lpszClassName: w!("my_hidden_class\0"),
                ..Default::default()
            };
            RegisterClassW(&wc);

            // Create the hidden window
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE, // Extended style to not activate the window
                wc.lpszClassName,
                w!("MultimediaKeysHandler\0"),
                WINDOW_STYLE::default(), // No window styles, invisible
                0,
                0,
                0,
                0,
                HWND_MESSAGE, // Message-only window
                None,
                h_instance,
                None,
            )?;

            println!("Listening for multimedia key events...");

            // Message loop
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
                println!("Message loop");
            }
            Ok(Self { _hwnd: hwnd })
        }
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_KEYDOWN => {
                println!("Key down event");
                match VIRTUAL_KEY(wparam.0 as u16) {
                    VK_MEDIA_NEXT_TRACK => {
                        println!("Next Track key pressed");
                    }
                    VK_MEDIA_PREV_TRACK => {
                        println!("Previous Track key pressed");
                    }
                    VK_MEDIA_PLAY_PAUSE => {
                        println!("Play/Pause key pressed");
                    }
                    VK_VOLUME_UP => {
                        println!("Volume Up key pressed");
                    }
                    VK_VOLUME_DOWN => {
                        println!("Volume Down key pressed");
                    },
                    _ => {
                        println!("Unknown key pressed");
                    }
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                println!("Destroying window");
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => {
                println!("Unknown message: {}", msg);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            },
        }
    }
}
