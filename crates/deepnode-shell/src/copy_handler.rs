use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SCROLL};
use tracing::info;

pub struct CopyHookHandler;

impl CopyHookHandler {
    /// Checks whether Scroll Lock is active. If active, bypass DeepNode Copy and fallback to Windows Explorer copy.
    pub fn is_scroll_lock_active() -> bool {
        unsafe {
            let state = GetKeyState(VK_SCROLL.0 as i32);
            (state & 1) != 0
        }
    }

    /// Handles file drag and drop / clipboard paste request.
    /// Returns true if intercepted and handed over to DeepNode Copy.
    pub fn should_intercept_operation() -> bool {
        if Self::is_scroll_lock_active() {
            info!("Scroll Lock is active: Bypassing DeepNode Copy, using standard Explorer handler.");
            return false;
        }
        true
    }
}
