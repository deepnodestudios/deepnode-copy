use serde::{Deserialize, Serialize};
use tracing::info;
use windows::core::PCWSTR;
use windows::Win32::System::Shutdown::{ExitWindowsEx, EWX_FORCEIFHUNG, EWX_SHUTDOWN};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFinishAction {
    DoNothing,
    Shutdown,
    Sleep,
    Hibernate,
    CloseApp,
}

impl Default for OnFinishAction {
    fn default() -> Self {
        OnFinishAction::DoNothing
    }
}

pub struct OnFinishExecutor;

impl OnFinishExecutor {
    /// Executes the configured post-transfer system action.
    pub fn execute(action: OnFinishAction) {
        match action {
            OnFinishAction::DoNothing => {}
            OnFinishAction::Shutdown => {
                info!("Executing system shutdown...");
                unsafe {
                    let _ = ExitWindowsEx(
                        EWX_SHUTDOWN | EWX_FORCEIFHUNG,
                        windows::Win32::System::Shutdown::SHUTDOWN_REASON(0),
                    );
                }
            }
            OnFinishAction::Sleep => {
                info!("Entering sleep mode...");
                // SetSuspendState: bHibernate = FALSE, bForce = FALSE, bWakeupEventsDisabled = FALSE
                #[link(name = "PowrProf")]
                extern "system" {
                    fn SetSuspendState(bhibernate: u8, bforce: u8, bwakeupeventsdisabled: u8) -> u8;
                }
                unsafe {
                    SetSuspendState(0, 0, 0);
                }
            }
            OnFinishAction::Hibernate => {
                info!("Entering hibernation...");
                #[link(name = "PowrProf")]
                extern "system" {
                    fn SetSuspendState(bhibernate: u8, bforce: u8, bwakeupeventsdisabled: u8) -> u8;
                }
                unsafe {
                    SetSuspendState(1, 0, 0);
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
                windows::core::w!("SystemHand")
            } else {
                windows::core::w!("SystemAsterisk")
            };
            let _ = windows::Win32::Media::Audio::PlaySoundW(
                PCWSTR(sound_alias.as_ptr()),
                None,
                windows::Win32::Media::Audio::SND_ALIAS | windows::Win32::Media::Audio::SND_ASYNC,
            );
        }
    }
}
