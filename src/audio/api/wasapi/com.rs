use windows::Win32::{
    Foundation::RPC_E_CHANGED_MODE,
    System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
};

thread_local! {
    static WASAPI_COM_INIT: ComWasapi = {
        unsafe {
            match CoInitializeEx(None, COINIT_APARTMENTTHREADED) {
                Ok(_) => ComWasapi { is_initialized: true },
                Err(err) => {
                    if err.code() == RPC_E_CHANGED_MODE {
                        ComWasapi { is_initialized: true }
                    } else {
                        panic!("Failed to initialize COM: {}", err);
                    }
                }
            }
        }
    }
}

struct ComWasapi {
    is_initialized: bool,
}

impl Drop for ComWasapi {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            if self.is_initialized {
                CoUninitialize();
                self.is_initialized = false;
            }
        }
    }
}

#[inline(always)]
pub fn com_initialize() {
    WASAPI_COM_INIT.with(|_| {})
}

#[inline(always)]
pub fn com_uninitialize() {
    unsafe {
        CoUninitialize();
    }
}
