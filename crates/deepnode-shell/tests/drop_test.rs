use deepnode_shell::{DeepNodeDragDropHandler, CLSID_DEEPNODE_DRAGDROP};
use windows::core::{IUnknown, Interface, PCWSTR};
use windows::Win32::System::Com::{IPersist, IPersistFile, STGM_READ};
use windows::Win32::System::Ole::IDropTarget;
use windows::Win32::UI::Shell::{IContextMenu, IShellExtInit};

#[test]
fn test_drag_drop_com_interfaces() {
    let unk: IUnknown = DeepNodeDragDropHandler::new().into();

    // 1. Test IPersist
    let persist: IPersist = unk.cast().expect("Should cast to IPersist");
    let clsid = unsafe { persist.GetClassID() }.expect("GetClassID should succeed");
    assert_eq!(clsid, CLSID_DEEPNODE_DRAGDROP);

    // 2. Test IPersistFile Load with target path
    let persist_file: IPersistFile = unk.cast().expect("Should cast to IPersistFile");
    let target_path = "E:\\test_drag_dst\0";
    let target_wide: Vec<u16> = target_path.encode_utf16().collect();
    let load_res = unsafe { persist_file.Load(PCWSTR(target_wide.as_ptr()), STGM_READ) };
    assert!(load_res.is_ok(), "IPersistFile::Load should succeed");

    // 3. Test IDropTarget interface query
    let drop_target: Result<IDropTarget, _> = unk.cast();
    assert!(drop_target.is_ok(), "Should cast to IDropTarget");

    // 4. Test IContextMenu interface query
    let context_menu: Result<IContextMenu, _> = unk.cast();
    assert!(context_menu.is_ok(), "Should cast to IContextMenu");

    // 5. Test IShellExtInit interface query
    let shell_init: Result<IShellExtInit, _> = unk.cast();
    assert!(shell_init.is_ok(), "Should cast to IShellExtInit");
}

#[test]
fn test_scroll_lock_bypass() {
    use deepnode_shell::CopyHookHandler;
    // When Scroll Lock is inactive (normal operation), intercept should be active
    let intercept = CopyHookHandler::should_intercept_operation();
    println!("Scroll lock active? Intercept allowed: {}", intercept);
}

