use std::marker::PhantomData;
use wasapi::{initialize_mta, deinitialize};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;

thread_local! {
    static WASAPI_COM_INIT: ComWasapi = {
            let result = initialize_mta();
            match result {
                Err(err) => {
                    if err.code().0 == RPC_E_CHANGED_MODE.0 {
                        ComWasapi { is_ok: true, _ptr: PhantomData }
                    } else {
                        panic!("Failed to initialize COM: {}", err);
                    }
                },
                Ok(_) => ComWasapi { is_ok: true, _ptr: PhantomData },
            }
    }
}

struct ComWasapi {
    is_ok: bool,
    _ptr: PhantomData<*mut ()>,
}

impl Drop for ComWasapi {
    #[inline]
    fn drop(&mut self) {
        if self.is_ok {
            deinitialize()
        }
    }
}

#[inline]
pub fn com_initialize() {
    WASAPI_COM_INIT.with(|_| {})
}
