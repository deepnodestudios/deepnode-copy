//! Cross-process switch that stops the Explorer CopyHook from intercepting the
//! "Explorer" action the user explicitly chose in DeepNode's popup.

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, OpenEventW, ResetEvent, SetEvent, WaitForSingleObject,
    SYNCHRONIZATION_ACCESS_RIGHTS,
};

const EVENT_NAME: windows::core::PCWSTR = w!("Local\\DeepNodeCopy_SuppressCopyHook");

/// Holds the suppression event in the signaled state for its lifetime.
pub struct CopyHookSuppression {
    handle: HANDLE,
}

impl CopyHookSuppression {
    pub fn activate() -> Option<Self> {
        unsafe {
            let handle = CreateEventW(None, true, true, EVENT_NAME).ok()?;
            let _ = SetEvent(handle);
            Some(Self { handle })
        }
    }
}

impl Drop for CopyHookSuppression {
    fn drop(&mut self) {
        unsafe {
            let _ = ResetEvent(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

/// True while this session has an explicit Explorer operation in flight.
pub fn copy_hooks_suppressed() -> bool {
    unsafe {
        // SYNCHRONIZE (0x00100000): wait-only access to the cross-process event.
        match OpenEventW(SYNCHRONIZATION_ACCESS_RIGHTS(0x00100000), false, EVENT_NAME) {
            Ok(handle) => {
                let signaled = WaitForSingleObject(handle, 0) == WAIT_OBJECT_0;
                let _ = CloseHandle(handle);
                signaled
            }
            Err(_) => false,
        }
    }
}
