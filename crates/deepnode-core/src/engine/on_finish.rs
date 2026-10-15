use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Shutdown::{ExitWindowsEx, EWX_FORCEIFHUNG, EWX_SHUTDOWN, SHUTDOWN_REASON};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnFinishAction {
    #[default]
    DoNothing,
    Shutdown,
    Sleep,
    Hibernate,
    CloseApp,
}

pub struct OnFinishExecutor;

fn enable_shutdown_privilege() -> Result<(), String> {
    unsafe {
        let mut token: HANDLE = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .map_err(|e| format!("OpenProcessToken başarısız: {}", e))?;

        let mut luid = LUID::default();
        if let Err(e) = LookupPrivilegeValueW(None, SE_SHUTDOWN_NAME, &mut luid) {
            let _ = CloseHandle(token);
            return Err(format!("LookupPrivilegeValueW (SE_SHUTDOWN_NAME) başarısız: {}", e));
        }

        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };

        let res = AdjustTokenPrivileges(
            token,
            false,
            Some(&tp),
            std::mem::size_of::<TOKEN_PRIVILEGES>() as u32,
            None,
            None,
        );
        let _ = CloseHandle(token);

        res.map_err(|e| format!("AdjustTokenPrivileges (SE_SHUTDOWN_NAME) başarısız: {}", e))
    }
}

impl OnFinishExecutor {
    /// Executes the configured post-transfer system action.
    pub fn execute(action: OnFinishAction) -> Result<(), String> {
        match action {
            OnFinishAction::DoNothing => Ok(()),
            OnFinishAction::Shutdown => {
                info!("Executing system shutdown...");
                enable_shutdown_privilege()?;
                unsafe {
                    ExitWindowsEx(
                        EWX_SHUTDOWN | EWX_FORCEIFHUNG,
                        SHUTDOWN_REASON(0),
                    )
                    .map_err(|e| {
                        error!("ExitWindowsEx failed: {}", e);
                        format!("Sistem kapatma çağrısı başarısız oldu (ExitWindowsEx): {}", e)
                    })?;
                }
                Ok(())
            }
            OnFinishAction::Sleep => {
                info!("Entering sleep mode...");
                // SetSuspendState: bHibernate = FALSE, bForce = FALSE, bWakeupEventsDisabled = FALSE
                #[link(name = "PowrProf")]
                extern "system" {
                    fn SetSuspendState(bhibernate: u8, bforce: u8, bwakeupeventsdisabled: u8) -> u8;
                }
                let ret = unsafe { SetSuspendState(0, 0, 0) };
                if ret == 0 {
                    let err = std::io::Error::last_os_error();
                    error!("SetSuspendState(Sleep) failed: {}", err);
                    Err(format!("Uyku moduna geçiş başarısız oldu: {}", err))
                } else {
                    Ok(())
                }
            }
            OnFinishAction::Hibernate => {
                info!("Entering hibernation...");
                #[link(name = "PowrProf")]
                extern "system" {
                    fn SetSuspendState(bhibernate: u8, bforce: u8, bwakeupeventsdisabled: u8) -> u8;
                }
                let ret = unsafe { SetSuspendState(1, 0, 0) };
                if ret == 0 {
                    let err = std::io::Error::last_os_error();
                    error!("SetSuspendState(Hibernate) failed: {}", err);
                    Err(format!("Hazırda bekletme moduna geçiş başarısız oldu: {}", err))
                } else {
                    Ok(())
                }
            }
            OnFinishAction::CloseApp => {
                info!("Closing DeepNode Copy application...");
                std::process::exit(0);
            }
        }
    }

    /// Plays a system completion notification sound.
    pub fn play_completion_sound(has_errors: bool) {
        unsafe {
            let sound_alias = if has_errors {
                w!("SystemHand")
            } else {
                w!("SystemAsterisk")
            };
            let played = windows::Win32::Media::Audio::PlaySoundW(
                PCWSTR(sound_alias.as_ptr()),
                None,
                windows::Win32::Media::Audio::SND_ALIAS | windows::Win32::Media::Audio::SND_ASYNC,
            );
            if !played.as_bool() {
                warn!("PlaySoundW failed to play completion sound");
            }
        }
    }
}
