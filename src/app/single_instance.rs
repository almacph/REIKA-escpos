#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, ReleaseMutex, MUTEX_ALL_ACCESS};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONWARNING, MB_YESNO};

const MUTEX_NAME: &str = "Global\\REIKA_ESCPOS_PRINTER_SERVICE";

#[cfg(windows)]
pub struct SingleInstance {
    handle: HANDLE,
}

#[cfg(windows)]
impl SingleInstance {
    pub fn acquire() -> Result<Self, SingleInstanceError> {
        let wide_name: Vec<u16> = MUTEX_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            if let Ok(existing) = OpenMutexW(MUTEX_ALL_ACCESS, false, PCWSTR(wide_name.as_ptr())) {
                let _ = CloseHandle(existing);
                return Err(SingleInstanceError::AlreadyRunning);
            }

            let result: Result<HANDLE, windows::core::Error> =
                CreateMutexW(None, true, PCWSTR(wide_name.as_ptr()));

            match result {
                Ok(handle) => Ok(Self { handle }),
                Err(e) => Err(SingleInstanceError::CreateFailed(e.to_string())),
            }
        }
    }
}

#[cfg(windows)]
impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
pub fn show_already_running_dialog() -> bool {
    let title: Vec<u16> = "REIKA Printer Service"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let message: Vec<u16> = "REIKA Printer Service is already running.\n\nWould you like to close the existing instance and start a new one?"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let result = MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_YESNO | MB_ICONWARNING,
        );
        result == IDYES
    }
}

#[cfg(not(windows))]
pub struct SingleInstance;

#[cfg(not(windows))]
impl SingleInstance {
    pub fn acquire() -> Result<Self, SingleInstanceError> {
        Ok(Self)
    }
}

#[cfg(not(windows))]
pub fn show_already_running_dialog() -> bool {
    eprintln!("Another instance is already running.");
    false
}

#[derive(Debug)]
pub enum SingleInstanceError {
    AlreadyRunning,
    CreateFailed(String),
}

impl std::fmt::Display for SingleInstanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => write!(f, "Another instance is already running"),
            Self::CreateFailed(e) => write!(f, "Failed to create mutex: {}", e),
        }
    }
}

impl std::error::Error for SingleInstanceError {}
