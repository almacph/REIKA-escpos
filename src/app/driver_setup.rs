//! Read-only Windows driver diagnostics used by the Setup/Repair screen.
//!
//! This module deliberately does not install, bind, or open a USB device.  Any
//! mutating operation belongs to the elevated helper in a later package group.

#![allow(dead_code)]

use std::fmt;
use std::sync::mpsc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairOutcome {
    Ready,
    RebootRequired,
    ReplugRequired,
    Cancelled,
    TimedOut,
    Failed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HelperProcessState {
    Running,
    Exited,
}

fn timeout_outcome(state: HelperProcessState) -> Option<RepairOutcome> {
    (state == HelperProcessState::Exited).then_some(RepairOutcome::TimedOut)
}

impl fmt::Display for RepairOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready => f.write_str("WinUSB repair completed and was verified."),
            Self::RebootRequired => {
                f.write_str("Windows requires a reboot before the binding is ready.")
            }
            Self::ReplugRequired => {
                f.write_str("Unplug and reconnect the printer to finish repair.")
            }
            Self::Cancelled => {
                f.write_str("Administrator elevation was cancelled; no device changes were made.")
            }
            Self::TimedOut => {
                f.write_str("The elevated repair helper timed out; check Windows device settings.")
            }
            Self::Failed(error) => write!(f, "WinUSB repair failed: {error}"),
        }
    }
}

/// Launch only the packaged helper from the protected installation directory.
/// The helper validates the instance, manifest key, package signature, and
/// selected device again after elevation. This function never accepts a path,
/// INF name, VID/PID, or arbitrary command from the GUI.
pub fn launch_repair(instance: String, manifest_key: String) -> mpsc::Receiver<RepairOutcome> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(|| launch_repair_blocking(&instance, &manifest_key))
            .unwrap_or_else(|_| RepairOutcome::Failed("elevated helper worker crashed".into()));
        let _ = tx.send(outcome);
    });
    rx
}

#[cfg(target_os = "windows")]
fn launch_repair_blocking(instance: &str, manifest_key: &str) -> RepairOutcome {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, TerminateProcess, WaitForSingleObject,
    };
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

    if !matches!(manifest_key, "standard" | "ics-advent") || instance.is_empty() {
        return RepairOutcome::Failed("invalid selected device".into());
    }
    let Ok(exe) = std::env::current_exe() else {
        return RepairOutcome::Failed("could not resolve the installed application path".into());
    };
    let Some(parent) = exe.parent() else {
        return RepairOutcome::Failed("installed application path has no parent".into());
    };
    let Ok(parent) = parent.canonicalize() else {
        return RepairOutcome::Failed(
            "could not validate the installed application directory".into(),
        );
    };
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath};
    let Ok(program_files_wide) =
        (unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, Default::default(), None) })
    else {
        return RepairOutcome::Failed("Windows Program Files directory is unavailable".into());
    };
    let Ok(program_files) = (unsafe { program_files_wide.to_string() }) else {
        unsafe {
            CoTaskMemFree(Some(program_files_wide.0 as *const _));
        }
        return RepairOutcome::Failed("Windows Program Files directory is unavailable".into());
    };
    unsafe {
        CoTaskMemFree(Some(program_files_wide.0 as *const _));
    }
    let Ok(expected_parent) = std::path::Path::new(&program_files)
        .join("REIKA-escpos")
        .canonicalize()
    else {
        return RepairOutcome::Failed(
            "the protected REIKA installation directory is unavailable".into(),
        );
    };
    if parent != expected_parent {
        return RepairOutcome::Failed(
            "driver repair is available only from the protected Program Files installation".into(),
        );
    }
    let Ok(helper) = parent.join("reika-driver-setup.exe").canonicalize() else {
        return RepairOutcome::Failed("the installed driver helper is unavailable".into());
    };
    if helper.file_name().and_then(|name| name.to_str()) != Some("reika-driver-setup.exe") {
        return RepairOutcome::Failed("the installed driver helper path is invalid".into());
    }
    let file = OsStr::new(&helper)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let params = format!("{} {}", quote_arg(instance), quote_arg(manifest_key));
    let params = OsStr::new(&params)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let verb = OsStr::new("runas")
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: 1,
        ..Default::default()
    };
    if let Err(error) = unsafe { ShellExecuteExW(&mut info) } {
        return if error.code().0 as u32 == 1223 {
            RepairOutcome::Cancelled
        } else {
            RepairOutcome::Failed(format!(
                "Windows could not start the elevated helper: {error}"
            ))
        };
    }
    let Some(process) = (!info.hProcess.is_invalid()).then_some(info.hProcess) else {
        return RepairOutcome::Failed("Windows did not return a helper process".into());
    };
    let wait = unsafe { WaitForSingleObject(process, 300_000) };
    if wait == WAIT_TIMEOUT {
        unsafe {
            let _ = TerminateProcess(process, 1);
        }
        // An elevated process may deny PROCESS_TERMINATE to the standard-user
        // caller. Keep the handle open and wait until Windows confirms that the
        // helper has exited before allowing the owner lease to resume.
        loop {
            let terminated = unsafe { WaitForSingleObject(process, 30_000) };
            if terminated == WAIT_OBJECT_0 {
                unsafe {
                    let _ = CloseHandle(process);
                }
                return timeout_outcome(HelperProcessState::Exited)
                    .expect("confirmed helper timeout must produce an outcome");
            }
        }
    }
    if wait != WAIT_OBJECT_0 {
        // A failed wait does not prove that the process has stopped. Continue
        // waiting for the same confirmation before reading its exit code.
        loop {
            if unsafe { WaitForSingleObject(process, 30_000) } == WAIT_OBJECT_0 {
                break;
            }
        }
    }
    let mut code = 1u32;
    let read_ok = unsafe { GetExitCodeProcess(process, &mut code) }.is_ok();
    unsafe {
        let _ = CloseHandle(process);
    }
    if !read_ok {
        return RepairOutcome::Failed("could not read the helper result".into());
    }
    match code {
        0 => RepairOutcome::Ready,
        2 => RepairOutcome::RebootRequired,
        3 => RepairOutcome::ReplugRequired,
        4 => RepairOutcome::Cancelled,
        5 => RepairOutcome::Failed(
            "signed driver assets are unavailable; use the documented recovery path".into(),
        ),
        _ => RepairOutcome::Failed(format!("helper exited with code {code}")),
    }
}

#[cfg(not(target_os = "windows"))]
fn launch_repair_blocking(_instance: &str, _manifest_key: &str) -> RepairOutcome {
    RepairOutcome::Failed("Windows driver repair is unavailable on this platform".into())
}

#[cfg(target_os = "windows")]
fn quote_arg(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverState {
    Absent,
    ReadyWinUsb,
    VerificationPending,
    UsbPrint,
    LegacyLibusb,
    Ambiguous,
    AccessDenied,
    Unsupported,
}

impl fmt::Display for DriverState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Absent => "Printer not present",
            Self::ReadyWinUsb => "WinUSB binding detected",
            Self::VerificationPending => "Binding needs verification",
            Self::UsbPrint => "Windows USB printer driver detected",
            Self::LegacyLibusb => "Legacy libusb driver detected",
            Self::Ambiguous => "Multiple matching devices need selection",
            Self::AccessDenied => "Device details require permission",
            Self::Unsupported => "Device is outside the supported identifier list",
        };
        f.write_str(text)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupportedHardware {
    pub vendor_id: u16,
    pub product_id: u16,
    pub preset: &'static str,
}

pub const SUPPORTED_HARDWARE: &[SupportedHardware] = &[
    SupportedHardware {
        vendor_id: 0x0483,
        product_id: 0x5840,
        preset: "Standard (XP-58IIH)",
    },
    SupportedHardware {
        vendor_id: 0x0FE6,
        product_id: 0x811E,
        preset: "ICS Advent",
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceDiagnostic {
    pub state: DriverState,
    pub preset: Option<&'static str>,
    pub hardware_id: String,
    pub instance_id: String,
    pub interface: String,
    pub service: String,
    pub provider: String,
    pub version: String,
    pub detail: String,
}

impl DeviceDiagnostic {
    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self {
            state: DriverState::VerificationPending,
            preset: None,
            hardware_id: String::new(),
            instance_id: String::new(),
            interface: String::new(),
            service: String::new(),
            provider: String::new(),
            version: String::new(),
            detail: detail.into(),
        }
    }
}

/// Return the known hardware entry for a VID/PID pair.
pub fn supported_hardware(vendor_id: u16, product_id: u16) -> Option<SupportedHardware> {
    SUPPORTED_HARDWARE
        .iter()
        .copied()
        .find(|item| item.vendor_id == vendor_id && item.product_id == product_id)
}

/// Classify read-only device metadata.  The caller must provide the complete
/// instance/interface record so an ID match alone is never reported as ready.
pub fn classify_device(
    vendor_id: u16,
    product_id: u16,
    service: &str,
    provider: &str,
    instance_count: usize,
) -> DriverState {
    if instance_count == 0 {
        return DriverState::Absent;
    }
    if instance_count > 1 {
        return DriverState::Ambiguous;
    }
    if supported_hardware(vendor_id, product_id).is_none() {
        return DriverState::Unsupported;
    }
    let binding = format!("{} {}", service, provider).to_ascii_lowercase();
    if binding.contains("winusb") {
        DriverState::ReadyWinUsb
    } else if binding.contains("usbprint") || binding.contains("usb printer") {
        DriverState::UsbPrint
    } else if binding.contains("libusb") {
        DriverState::LegacyLibusb
    } else {
        DriverState::VerificationPending
    }
}

/// Enumerate Windows device metadata without elevation or mutation.
#[cfg(target_os = "windows")]
pub fn inventory() -> Vec<DeviceDiagnostic> {
    use std::process::Command;
    let output = Command::new("pnputil")
        .args(["/enum-devices", "/connected", "/class", "USB"])
        .output();
    let Ok(output) = output else {
        return vec![DeviceDiagnostic::unavailable(
            "Could not run pnputil for read-only inventory",
        )];
    };
    if !output.status.success() {
        return vec![DeviceDiagnostic::unavailable(String::from_utf8_lossy(
            &output.stderr,
        ))];
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_pnputil_inventory(&text)
}

#[cfg(not(target_os = "windows"))]
pub fn inventory() -> Vec<DeviceDiagnostic> {
    vec![DeviceDiagnostic::unavailable(
        "Windows driver inventory is available in the Windows build",
    )]
}

#[cfg(target_os = "windows")]
fn parse_pnputil_inventory(text: &str) -> Vec<DeviceDiagnostic> {
    // pnputil output is localized, so preserve raw records and only classify
    // fields when their English labels are present.  Unknown records remain
    // verification-pending instead of being called ready.
    let mut records = Vec::new();
    let mut current = Vec::<(&str, &str)>::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !current.is_empty() {
                let get = |key| {
                    current
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, v)| v.trim())
                        .unwrap_or("")
                };
                let hardware_id = get("Hardware IDs").to_string();
                let (vendor_id, product_id) = parse_vid_pid(&hardware_id).unwrap_or((0, 0));
                let service = get("Service");
                let provider = get("Driver Provider");
                records.push(DeviceDiagnostic {
                    state: classify_device(vendor_id, product_id, service, provider, 1),
                    preset: supported_hardware(vendor_id, product_id).map(|x| x.preset),
                    hardware_id,
                    instance_id: get("Instance ID").to_string(),
                    interface: String::new(),
                    service: service.to_string(),
                    provider: provider.to_string(),
                    version: get("Driver Version").to_string(),
                    detail: "Read-only pnputil inventory; no binding operation performed".into(),
                });
                current.clear();
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            current.push((key.trim(), value.trim()));
        }
    }
    records
}

#[cfg(target_os = "windows")]
fn parse_vid_pid(value: &str) -> Option<(u16, u16)> {
    let upper = value.to_ascii_uppercase();
    let vid = upper.split("VID_").nth(1)?.get(..4)?.parse::<u16>().ok()?;
    let pid = upper.split("PID_").nth(1)?.get(..4)?.parse::<u16>().ok()?;
    Some((vid, pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_ids_are_explicit_and_classified() {
        assert_eq!(
            supported_hardware(0x0483, 0x5840).unwrap().preset,
            "Standard (XP-58IIH)"
        );
        assert_eq!(
            classify_device(0x0483, 0x5840, "WinUSB", "Microsoft", 1),
            DriverState::ReadyWinUsb
        );
        assert_eq!(
            classify_device(0x0483, 0x5840, "USBPRINT", "Microsoft", 1),
            DriverState::UsbPrint
        );
        assert_eq!(
            classify_device(0x0483, 0x5840, "libusbK", "libusb", 1),
            DriverState::LegacyLibusb
        );
        assert_eq!(
            classify_device(0x0483, 0x5840, "", "", 2),
            DriverState::Ambiguous
        );
        assert_eq!(
            classify_device(0x1234, 0x5678, "WinUSB", "Microsoft", 1),
            DriverState::Unsupported
        );
        assert_eq!(
            classify_device(0x0483, 0x5840, "", "", 0),
            DriverState::Absent
        );
    }

    #[test]
    fn timeout_cannot_resume_maintenance_until_helper_exit_is_confirmed() {
        assert_eq!(timeout_outcome(HelperProcessState::Running), None);
        assert_eq!(
            timeout_outcome(HelperProcessState::Exited),
            Some(RepairOutcome::TimedOut)
        );
    }
}
