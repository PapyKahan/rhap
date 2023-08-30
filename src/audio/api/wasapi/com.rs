use std::marker::PhantomData;

use windows::Win32::{
    Foundation::RPC_E_CHANGED_MODE,
    System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
};

thread_local! {
    static WASAPI_COM_INIT: ComWasapi = {
            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            match result.clone() {
                Ok(_) => ComWasapi { result, _ptr: PhantomData },
                Err(err) => {
                    if err.code() == RPC_E_CHANGED_MODE {
                        ComWasapi { result, _ptr: PhantomData }
                    } else {
                        panic!("Failed to initialize COM: {}", err);
                    }
                }
            }
    }
}

struct ComWasapi {
    result: windows::core::Result<()>,
    _ptr: PhantomData<*mut ()>,
}

impl Drop for ComWasapi {
    #[inline]
    fn drop(&mut self) {
        if self.result.is_ok() {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

#[inline]
pub fn com_initialize() {
    WASAPI_COM_INIT.with(|_| {})
}
