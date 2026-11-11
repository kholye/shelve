use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent};

// `Local\` keeps both objects per user session.
const MUTEX_NAME: PCWSTR = w!(r"Local\Shelve.SingleInstance");
const SHOW_EVENT_NAME: PCWSTR = w!(r"Local\Shelve.ShowExisting");

/// Holds the instance mutex; keep it alive for the whole run of the app.
pub struct SingleInstance {
    mutex: Option<HANDLE>,
    show_event: Option<HANDLE>,
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        for h in [self.mutex, self.show_event].into_iter().flatten() {
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

/// First instance: returns. Second instance: wakes the first one (`ShowExisting`)
/// and exits the process.
pub fn acquire_single_instance() -> SingleInstance {
    let mutex = unsafe { CreateMutexW(None, false, MUTEX_NAME) };
    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let show_event = show_event().ok();
    if already_running {
        if let Some(ev) = show_event {
            unsafe {
                let _ = SetEvent(ev);
            }
        }
        std::process::exit(0);
    }
    // If the mutex call itself failed, run anyway: a second icon beats no app.
    SingleInstance {
        mutex: mutex.ok(),
        show_event,
    }
}

/// Auto-reset event the running instance waits on; opens it if it exists.
pub fn show_event() -> windows::core::Result<HANDLE> {
    unsafe { CreateEventW(None, false, false, SHOW_EVENT_NAME) }
}
