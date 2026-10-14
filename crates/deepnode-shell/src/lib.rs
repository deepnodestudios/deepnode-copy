pub mod sparse;
pub mod copy_handler;
pub mod context_menu;
pub mod drag_drop;

pub use sparse::SparsePackage;
pub use copy_handler::CopyHookHandler;
pub use context_menu::{DeepNodeExplorerCommand, DeepNodeClassFactory, CLSID_DEEPNODE_COMMAND};
pub use drag_drop::{DeepNodeDragDropHandler, DeepNodeDragDropClassFactory, CLSID_DEEPNODE_DRAGDROP};

use std::ffi::c_void;
use windows_core::Interface;
use windows::core::{GUID, HRESULT};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, S_FALSE, S_OK};
use windows::Win32::System::Com::IClassFactory;

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return windows::Win32::Foundation::E_POINTER;
    }

    if *rclsid == CLSID_DEEPNODE_COMMAND {
        let factory: IClassFactory = DeepNodeClassFactory.into();
        return factory.query(riid, ppv);
    }

    if *rclsid == CLSID_DEEPNODE_DRAGDROP {
        let factory: IClassFactory = DeepNodeDragDropClassFactory.into();
        return factory.query(riid, ppv);
    }

    CLASS_E_CLASSNOTAVAILABLE
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

#[no_mangle]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    // Registers in registry if needed
    S_OK
}

#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    S_OK
}
