#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::missing_safety_doc)]

pub mod sparse;
pub mod copy_handler;
pub mod context_menu;
pub mod drag_drop;
pub mod util;
pub mod registry;

pub use sparse::SparsePackage;
pub use copy_handler::{CopyHookHandler, DeepNodeCopyHook, DeepNodeCopyHookClassFactory, CLSID_DEEPNODE_COPYHOOK};
pub use context_menu::{DeepNodeExplorerCommand, DeepNodeClassFactory, CLSID_DEEPNODE_COMMAND};
pub use drag_drop::{DeepNodeDragDropHandler, DeepNodeDragDropClassFactory, CLSID_DEEPNODE_DRAGDROP};

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows_core::Interface;
use windows::core::{GUID, HRESULT};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, S_FALSE, S_OK};
use windows::Win32::System::Com::IClassFactory;

static MODULE_REF_COUNT: AtomicUsize = AtomicUsize::new(0);

#[inline]
pub fn dll_add_ref() {
    MODULE_REF_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[inline]
pub fn dll_release() {
    MODULE_REF_COUNT.fetch_sub(1, Ordering::SeqCst);
}

#[inline]
pub fn dll_ref_count() -> usize {
    MODULE_REF_COUNT.load(Ordering::SeqCst)
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return windows::Win32::Foundation::E_POINTER;
    }

    util::log_shell(&format!("DllGetClassObject: rclsid={:?}, riid={:?}", *rclsid, *riid));

    if *rclsid == CLSID_DEEPNODE_COMMAND {
        let factory: IClassFactory = DeepNodeClassFactory::new().into();
        return factory.query(riid, ppv);
    }

    if *rclsid == CLSID_DEEPNODE_DRAGDROP {
        let factory: IClassFactory = DeepNodeDragDropClassFactory::new().into();
        return factory.query(riid, ppv);
    }

    if *rclsid == CLSID_DEEPNODE_COPYHOOK {
        let factory: IClassFactory = DeepNodeCopyHookClassFactory::new().into();
        return factory.query(riid, ppv);
    }

    CLASS_E_CLASSNOTAVAILABLE
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if MODULE_REF_COUNT.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[no_mangle]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    registry::register_server()
}

#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    registry::unregister_server()
}
