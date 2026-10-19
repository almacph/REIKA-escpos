//! Elevated, selected-device driver setup helper.
//!
//! The command line is intentionally narrow: the GUI supplies an enumerated
//! instance ID and a package-relative manifest key.  It cannot supply an INF,
//! VID/PID, or arbitrary command.  The mutating path is Windows-only; the
//! policy and asset checks remain testable on every host.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct DeviceManifest {
    device: Vec<ManifestDevice>,
}

#[derive(serde::Deserialize)]
struct ManifestDevice {
    preset: String,
    vendor_id: String,
    product_id: String,
    interface: String,
    interface_guid: String,
    provider: String,
    signer_thumbprint: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetupResult {
    Ready,
    Absent,
    RebootRequired,
    ReplugRequired,
    ElevationRequired,
    Unavailable,
    Failed,
    BindingFailed,
    ReplugPending,
}

fn normalize_thumbprint(value: &str) -> Option<[u8; 32]> {
    let compact: String = value
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && *c != ':')
        .collect();
    if compact.len() != 64 || !compact.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&compact[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn signer_pin_matches(expected: &str, actual: &[u8]) -> bool {
    normalize_thumbprint(expected).is_some_and(|pin| pin.as_slice() == actual)
}

fn decode_bounded_utf16(bytes: &[u8], offset: usize, used_bytes: usize) -> Option<String> {
    if offset > used_bytes || used_bytes > bytes.len() || !(used_bytes - offset).is_multiple_of(2) {
        return None;
    }
    let words = &bytes[offset..used_bytes];
    let mut values = Vec::with_capacity(words.len() / 2);
    for pair in words.chunks_exact(2) {
        values.push(u16::from_ne_bytes([pair[0], pair[1]]));
    }
    let end = values.iter().position(|value| *value == 0)?;
    String::from_utf16(&values[..end]).ok()
}

fn decode_bounded_utf16_multisz(bytes: &[u8], used_bytes: usize) -> Option<Vec<String>> {
    if used_bytes > bytes.len() || !used_bytes.is_multiple_of(2) {
        return None;
    }
    let words: Vec<u16> = bytes[..used_bytes]
        .chunks_exact(2)
        .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
        .collect();
    let end = words.windows(2).position(|pair| pair == [0, 0])?;
    let mut result = Vec::new();
    for part in words[..end + 1].split(|value| *value == 0) {
        if part.is_empty() {
            continue;
        }
        result.push(String::from_utf16(part).ok()?);
    }
    Some(result)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BindingEvidence {
    instance_present: bool,
    service_winusb: bool,
    provider_matches: bool,
    inf_metadata_present: bool,
    interface_instance_matches: bool,
    interface_access: bool,
    reboot_required: bool,
    replug_pending: bool,
}

fn classify_binding(e: BindingEvidence) -> SetupResult {
    if e.reboot_required {
        return SetupResult::RebootRequired;
    }
    if e.replug_pending || !e.instance_present {
        return SetupResult::ReplugPending;
    }
    if e.service_winusb
        && e.provider_matches
        && e.inf_metadata_present
        && e.interface_instance_matches
        && e.interface_access
    {
        SetupResult::Ready
    } else {
        SetupResult::BindingFailed
    }
}

#[cfg(target_os = "windows")]
fn parse_guid(value: &str) -> Option<windows::core::GUID> {
    let value = value
        .trim()
        .trim_matches('{')
        .trim_matches('}')
        .replace('-', "");
    if value.len() != 32 {
        return None;
    }
    u128::from_str_radix(&value, 16)
        .ok()
        .map(windows::core::GUID::from_u128)
}

#[cfg(target_os = "windows")]
fn interface_probe(instance: &str, guid_text: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE,
        DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
        SP_DEVINFO_DATA,
    };
    use windows::Win32::Devices::Usb::{WinUsb_Free, WinUsb_Initialize};
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    let Some(guid) = parse_guid(guid_text) else {
        return false;
    };
    let Ok(set) = (unsafe {
        SetupDiGetClassDevsW(
            Some(&guid),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    }) else {
        return false;
    };
    let mut matched = false;
    for index in 0..64 {
        let mut iface = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        if unsafe { SetupDiEnumDeviceInterfaces(set, None, &guid, index, &mut iface) }.is_err() {
            break;
        }
        let mut required = 0;
        let _ = unsafe {
            SetupDiGetDeviceInterfaceDetailW(set, &iface, None, 0, Some(&mut required), None)
        };
        if required == 0 {
            continue;
        }
        let mut buffer = vec![
            0usize;
            (required as usize + std::mem::size_of::<usize>() - 1)
                / std::mem::size_of::<usize>()
        ];
        let allocated_bytes = buffer.len() * std::mem::size_of::<usize>();
        let bytes = buffer.as_mut_ptr() as *mut u8;
        let detail = bytes as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        unsafe {
            (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        }
        let mut data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        if unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set,
                &iface,
                Some(detail),
                required,
                Some(&mut required),
                Some(&mut data),
            )
        }
        .is_err()
        {
            continue;
        }
        let mut id = vec![0u16; 1024];
        let mut id_len = 0;
        if unsafe { SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut id), Some(&mut id_len)) }
            .is_err()
        {
            continue;
        }
        let id_end = id.iter().position(|v| *v == 0).unwrap_or(id.len());
        if !String::from_utf16_lossy(&id[..id_end]).eq_ignore_ascii_case(instance) {
            continue;
        }
        let path_offset = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
        let path = unsafe {
            decode_bounded_utf16(
                std::slice::from_raw_parts(bytes, allocated_bytes),
                path_offset,
                required as usize,
            )
        };
        let Some(path) = path else {
            continue;
        };
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        };
        if let Ok(handle) = handle {
            if handle != INVALID_HANDLE_VALUE && !handle.is_invalid() {
                let mut usb = unsafe { std::mem::zeroed() };
                matched = unsafe { WinUsb_Initialize(handle, &mut usb).is_ok() };
                if !usb.is_invalid() {
                    let _ = unsafe { WinUsb_Free(usb) };
                }
                let _ = unsafe { CloseHandle(handle) };
            }
        }
        break;
    }
    unsafe {
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    matched
}

#[cfg(not(target_os = "windows"))]
fn interface_probe(_instance: &str, _guid_text: &str) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn fixed_package_root() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, SHGetKnownFolderPath};
    let program_files =
        unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramFiles, Default::default(), None).ok()? };
    let base = unsafe { program_files.to_string().ok()? };
    unsafe {
        CoTaskMemFree(Some(program_files.0 as *const _));
    }
    let protected = PathBuf::from(base).join("REIKA-escpos");
    let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
    let expected = protected.canonicalize().ok()?;
    if executable.parent()? != expected {
        return None;
    }
    let root = expected.join("driver").canonicalize().ok()?;
    if !root.starts_with(&expected) {
        return None;
    }
    Some(root)
}

#[cfg(not(target_os = "windows"))]
fn fixed_package_root() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|p| p.join("driver"))
}

#[cfg(target_os = "windows")]
unsafe fn query_selected_identity(
    set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    data: &windows::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
) -> ObservedIdentity {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiGetDevicePropertyW, SetupDiGetDeviceRegistryPropertyW, SPDRP_COMPATIBLEIDS,
        SPDRP_HARDWAREID, SPDRP_SERVICE,
    };
    use windows::Win32::Devices::Properties::{DEVPKEY_Device_Parent, DEVPROP_TYPE_STRING};
    fn registry(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        data: &windows::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
        property: windows::Win32::Devices::DeviceAndDriverInstallation::SETUP_DI_REGISTRY_PROPERTY,
    ) -> Vec<String> {
        let mut buffer = vec![0u8; 4096];
        let mut required = 0;
        let Ok(()) = (unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set,
                data,
                property,
                None,
                Some(&mut buffer),
                Some(&mut required),
            )
        }) else {
            return Vec::new();
        };
        decode_bounded_utf16_multisz(&buffer, required as usize).unwrap_or_default()
    }
    let mut identity = ObservedIdentity {
        hardware_ids: registry(set, data, SPDRP_HARDWAREID),
        compatible_ids: registry(set, data, SPDRP_COMPATIBLEIDS),
        service: registry(set, data, SPDRP_SERVICE).into_iter().next(),
        ..Default::default()
    };
    let mut parent_buffer = vec![0u8; 1024];
    let mut parent_type = DEVPROP_TYPE_STRING;
    let mut parent_required = 0;
    if SetupDiGetDevicePropertyW(
        set,
        data,
        &DEVPKEY_Device_Parent,
        &mut parent_type,
        Some(&mut parent_buffer),
        Some(&mut parent_required),
        0,
    )
    .is_ok()
    {
        identity.parent = decode_bounded_utf16(&parent_buffer, 0, parent_required as usize);
    }
    if let Some(parent) = identity.parent.as_deref() {
        // The parent service is queried from the same SetupAPI set by instance
        // lookup; failure is intentionally treated as unknown by policy.
        identity.parent_service = find_service_for_instance(set, parent);
    }
    identity
}

#[cfg(target_os = "windows")]
unsafe fn find_service_for_instance(
    set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    instance: &str,
) -> Option<String> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiEnumDeviceInfo, SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW,
        SPDRP_SERVICE, SP_DEVINFO_DATA,
    };
    for index in 0..1024 {
        let mut data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        if SetupDiEnumDeviceInfo(set, index, &mut data).is_err() {
            break;
        }
        let mut id = vec![0u16; 1024];
        let mut required = 0;
        if SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut id), Some(&mut required)).is_err() {
            continue;
        }
        let end = id.iter().position(|value| *value == 0).unwrap_or(id.len());
        if !String::from_utf16_lossy(&id[..end]).eq_ignore_ascii_case(instance) {
            continue;
        }
        let mut service = vec![0u8; 512];
        let mut length = 0;
        if SetupDiGetDeviceRegistryPropertyW(
            set,
            &data,
            SPDRP_SERVICE,
            None,
            Some(&mut service),
            Some(&mut length),
        )
        .is_ok()
        {
            return decode_bounded_utf16(&service, 0, length as usize);
        }
    }
    None
}

fn parse_instance(instance: &str) -> Option<(u16, u16, u8)> {
    let (vid, pid, mi) = parse_instance_details(instance)?;
    Some((vid, pid, mi.unwrap_or(0)))
}

fn parse_instance_details(instance: &str) -> Option<(u16, u16, Option<u8>)> {
    let parts: Vec<_> = instance.split('\\').collect();
    if parts.len() != 3 || parts[0] != "USB" || parts[2].is_empty() {
        return None;
    }
    if parts[1].contains("PARENT") || parts[1].contains("ROOT") {
        return None;
    }
    let upper = parts[1].to_ascii_uppercase();
    let vid = u16::from_str_radix(upper.split("VID_").nth(1)?.get(..4)?, 16).ok()?;
    let pid = u16::from_str_radix(upper.split("PID_").nth(1)?.get(..4)?, 16).ok()?;
    let mi = upper
        .split("MI_")
        .nth(1)
        .and_then(|value| u8::from_str_radix(value.get(..2)?, 16).ok());
    Some((vid, pid, mi))
}

fn allowed_instance(instance: &str) -> bool {
    matches!(
        parse_instance(instance),
        Some((0x0483, 0x5840, _)) | Some((0x0FE6, 0x811E, _))
    )
}

fn manifest_entry(root: &Path, key: &str) -> Option<ManifestDevice> {
    let value = std::fs::read_to_string(root.join("supported-devices.toml")).ok()?;
    let manifest: DeviceManifest = toml::from_str(&value).ok()?;
    let entry = manifest
        .device
        .into_iter()
        .find(|item| item.preset == key)?;
    if entry.interface == "CAPTURE_REQUIRED"
        || entry.interface.contains("REQUIRED")
        || entry.interface_guid == "CAPTURE_REQUIRED"
        || entry.interface_guid.contains("REQUIRED")
        || entry.provider == "CAPTURE_REQUIRED"
        || entry.provider.contains("REQUIRED")
        || entry.signer_thumbprint == "CAPTURE_REQUIRED"
        || entry.signer_thumbprint.contains("REQUIRED")
        || normalize_thumbprint(&entry.signer_thumbprint).is_none()
        || entry.vendor_id.contains('_')
        || entry.product_id.contains('_')
    {
        return None;
    }
    Some(entry)
}

fn manifest_matches_instance(instance: &str, key: &str, entry: &ManifestDevice) -> bool {
    if entry.preset != key {
        return false;
    }
    let Some((vendor, product, instance_mi)) = parse_instance_details(instance) else {
        return false;
    };
    let Ok(expected_vendor) = u16::from_str_radix(&entry.vendor_id, 16) else {
        return false;
    };
    let Ok(expected_product) = u16::from_str_radix(&entry.product_id, 16) else {
        return false;
    };
    if vendor != expected_vendor || product != expected_product {
        return false;
    }
    let interface = entry.interface.trim().to_ascii_uppercase();
    if interface.contains("REQUIRED") || interface == "CAPTURE_REQUIRED" {
        return false;
    }
    if interface == "LEAF" || interface == "NONCOMPOSITE" {
        return instance_mi.is_none();
    }
    let expected_mi = interface
        .strip_prefix("MI_")
        .or_else(|| interface.strip_prefix("0X"))
        .unwrap_or(&interface);
    let Ok(expected_mi) = u8::from_str_radix(expected_mi, 16) else {
        return false;
    };
    instance_mi == Some(expected_mi)
}

#[derive(Debug, Clone, Default)]
struct ObservedIdentity {
    hardware_ids: Vec<String>,
    compatible_ids: Vec<String>,
    parent: Option<String>,
    parent_service: Option<String>,
    service: Option<String>,
    ambiguous: bool,
}

fn selected_identity_is_reviewed(
    instance: &str,
    key: &str,
    entry: &ManifestDevice,
    observed: &ObservedIdentity,
) -> bool {
    if observed.ambiguous
        || !manifest_matches_instance(instance, key, entry)
        || observed.hardware_ids.is_empty()
        || observed.compatible_ids.is_empty()
    {
        return false;
    }
    let Some((vid, pid, mi)) = parse_instance_details(instance) else {
        return false;
    };
    let identity = format!("USB\\VID_{vid:04X}&PID_{pid:04X}");
    let expected_hardware =
        mi.map_or_else(|| identity.clone(), |mi| format!("{identity}&MI_{mi:02X}"));
    let hardware_ok = observed.hardware_ids.iter().any(|id| {
        let normalized = id.to_ascii_uppercase();
        normalized == expected_hardware
    });
    let Some(service) = observed.service.as_deref() else {
        return false;
    };
    if !hardware_ok || service.eq_ignore_ascii_case("usbccgp") {
        return false;
    }
    let Some(parent) = observed.parent.as_deref() else {
        return false;
    };
    let Some(_parent_service) = observed.parent_service.as_deref() else {
        return false;
    };
    if mi.is_some() {
        let Some((parent_vid, parent_pid, parent_mi)) = parse_instance_details(parent) else {
            return false;
        };
        parent_mi.is_none() && parent_vid == vid && parent_pid == pid
    } else {
        // A reviewed leaf may legitimately be directly below a ROOT_HUB;
        // arbitrary root devnodes and unknown parents are not equivalent.
        let parent = parent.to_ascii_uppercase();
        parent.starts_with("USB\\ROOT_HUB")
    }
}

fn package_asset_is_usable(root: &Path, key: &str) -> bool {
    if !matches!(key, "standard" | "ics-advent") {
        return false;
    }
    let Some(entry) = manifest_entry(root, key) else {
        return false;
    };
    let Some(inf) = package_asset_path(root, &format!("{key}.inf")) else {
        return false;
    };
    let Some(cat) = package_asset_path(root, &format!("{key}.cat")) else {
        return false;
    };
    let (vendor, product, _) = parse_instance(&format!(
        "USB\\VID_{}&PID_{}&MI_00\\x",
        entry.vendor_id, entry.product_id
    ))
    .unwrap_or_default();
    vendor != 0
        && product != 0
        && inf.is_file()
        && cat.is_file()
        && verify_catalog_signature(&cat, &entry.signer_thumbprint)
}

fn package_asset_path(root: &Path, name: &str) -> Option<PathBuf> {
    if name.contains(['/', '\\']) || name == "." || name == ".." {
        return None;
    }
    let canonical_root = root.canonicalize().ok()?;
    let canonical_asset = root.join(name).canonicalize().ok()?;
    canonical_asset
        .starts_with(canonical_root)
        .then_some(canonical_asset)
}

fn inf_bytes_match(source: &Path, candidate: &Path) -> bool {
    std::fs::read(source)
        .ok()
        .zip(std::fs::read(candidate).ok())
        .is_some_and(|(a, b)| a == b)
}

#[cfg(target_os = "windows")]
fn verify_catalog_signature(catalog: &Path, expected_thumbprint: &str) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::Cryptography::{
        CertGetCertificateContextProperty, CERT_SHA256_HASH_PROP_ID,
    };
    use windows::Win32::Security::WinTrust::{
        WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
        WTHelperProvDataFromStateData, WinVerifyTrustEx, WINTRUST_DATA, WINTRUST_FILE_INFO,
        WTD_CHOICE_FILE, WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UI_NONE,
    };
    let wide: Vec<u16> = catalog
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: Default::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    file.cbStruct = std::mem::size_of::<WINTRUST_FILE_INFO>() as u32;
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: windows::Win32::Security::WinTrust::WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    // WINTRUST_ACTION_GENERIC_VERIFY_V2, expressed through the typed Windows
    // GUID ABI so its little-endian in-memory fields are correct.
    let mut action = GUID::from_u128(0x00AAC56B_CD44_11D0_8CC2_00C04FC295EE);
    let verified = unsafe { WinVerifyTrustEx(HWND::default(), &mut action, &mut data) == 0 };
    let mut pinned = false;
    if verified {
        let provider = unsafe { WTHelperProvDataFromStateData(data.hWVTStateData) };
        if !provider.is_null() {
            let signer = unsafe { WTHelperGetProvSignerFromChain(provider, 0, false.into(), 0) };
            if !signer.is_null() {
                let cert = unsafe { WTHelperGetProvCertFromChain(signer, 0) };
                if !cert.is_null() {
                    let context = unsafe { (*cert).pCert };
                    let mut size = 0u32;
                    if unsafe {
                        CertGetCertificateContextProperty(
                            context,
                            CERT_SHA256_HASH_PROP_ID,
                            None,
                            &mut size,
                        )
                    }
                    .is_ok()
                    {
                        let mut hash = vec![0u8; size as usize];
                        pinned =
                            unsafe {
                                CertGetCertificateContextProperty(
                                    context,
                                    CERT_SHA256_HASH_PROP_ID,
                                    Some(hash.as_mut_ptr() as *mut _),
                                    &mut size,
                                )
                                .is_ok()
                            } && signer_pin_matches(expected_thumbprint, &hash[..size as usize]);
                    }
                }
            }
        }
    }
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    let _ = unsafe { WinVerifyTrustEx(HWND::default(), &mut action, &mut data) };
    verified && pinned
}

#[cfg(not(target_os = "windows"))]
fn verify_catalog_signature(_catalog: &Path, _expected_thumbprint: &str) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn is_elevated() -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut length = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
        .is_ok();
        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(target_os = "windows"))]
fn is_elevated() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn install_selected_device(instance: &str, package_root: &Path, key: &str) -> SetupResult {
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiBuildDriverInfoList, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
        SetupDiEnumDriverInfoW, SetupDiGetClassDevsW, SetupDiGetDeviceInstallParamsW,
        SetupDiGetDeviceInstanceIdW, SetupDiGetDriverInfoDetailW, SetupDiInstallDevice,
        SetupDiSetSelectedDriverW, DIGCF_ALLCLASSES, DI_NEEDREBOOT, DI_NEEDRESTART, HDEVINFO,
        SPDIT_COMPATDRIVER, SP_DEVINFO_DATA, SP_DEVINSTALL_PARAMS_W, SP_DRVINFO_DATA_V2_W,
        SP_DRVINFO_DETAIL_DATA_W,
    };
    let Some(entry) = manifest_entry(package_root, key) else {
        return SetupResult::Unavailable;
    };
    if !manifest_matches_instance(instance, key, &entry) {
        return SetupResult::Failed;
    }
    // The Windows SDK exposes this transaction through SetupAPI.  The helper
    // first stages the fixed package, then selects the matching driver node on
    // the exact enumerated instance and calls DiInstallDevice.  The package
    // files are validated before this function is reached.
    let Some(inf) = package_asset_path(package_root, &format!("{key}.inf")) else {
        return SetupResult::Unavailable;
    };
    let set: HDEVINFO =
        match unsafe { SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES) } {
            Ok(value) => value,
            Err(_) => return SetupResult::Failed,
        };
    let mut selected: Option<SP_DEVINFO_DATA> = None;
    let mut index = 0;
    loop {
        let mut data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let ok = unsafe { SetupDiEnumDeviceInfo(set, index, &mut data).is_ok() };
        if !ok {
            break;
        }
        let mut buffer = vec![0u16; 1024];
        let mut required = 0;
        let got = unsafe {
            SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut buffer), Some(&mut required))
        };
        if got.is_ok() {
            let end = buffer.iter().position(|x| *x == 0).unwrap_or(buffer.len());
            let candidate = String::from_utf16_lossy(&buffer[..end]);
            if candidate.eq_ignore_ascii_case(instance) {
                selected = Some(data);
                break;
            }
        }
        index += 1;
    }
    let Some(mut data) = selected else {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
        return SetupResult::Absent;
    };
    let observed = unsafe { query_selected_identity(set, &data) };
    if !selected_identity_is_reviewed(instance, key, &entry, &observed) {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
        return SetupResult::BindingFailed;
    }
    // Stage only after the exact selected devnode has passed the reviewed
    // hardware, parent, service, and ambiguity checks.
    let stage = std::process::Command::new("pnputil")
        .args(["/add-driver", inf.to_string_lossy().as_ref()])
        .output();
    if !matches!(stage, Ok(ref output) if output.status.success()) {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
        return SetupResult::Failed;
    }
    let selected_driver =
        unsafe { SetupDiBuildDriverInfoList(set, Some(&mut data), SPDIT_COMPATDRIVER) }
            .ok()
            .and_then(|_| {
                for index in 0..256 {
                    let mut candidate = SP_DRVINFO_DATA_V2_W {
                        cbSize: std::mem::size_of::<SP_DRVINFO_DATA_V2_W>() as u32,
                        ..Default::default()
                    };
                    if unsafe {
                        SetupDiEnumDriverInfoW(
                            set,
                            Some(&data),
                            SPDIT_COMPATDRIVER,
                            index,
                            &mut candidate,
                        )
                    }
                    .is_err()
                    {
                        break;
                    }
                    let mut detail = SP_DRVINFO_DETAIL_DATA_W {
                        cbSize: std::mem::size_of::<SP_DRVINFO_DETAIL_DATA_W>() as u32,
                        ..Default::default()
                    };
                    let _ = unsafe {
                        SetupDiGetDriverInfoDetailW(
                            set,
                            Some(&data),
                            &candidate,
                            Some(&mut detail),
                            std::mem::size_of::<SP_DRVINFO_DETAIL_DATA_W>() as u32,
                            None,
                        )
                    };
                    let provider_end = candidate
                        .ProviderName
                        .iter()
                        .position(|x| *x == 0)
                        .unwrap_or(candidate.ProviderName.len());
                    let hardware_end = detail
                        .HardwareID
                        .iter()
                        .position(|x| *x == 0)
                        .unwrap_or(detail.HardwareID.len());
                    let provider =
                        String::from_utf16_lossy(&candidate.ProviderName[..provider_end]);
                    let hardware_id = String::from_utf16_lossy(&detail.HardwareID[..hardware_end]);
                    let inf_end = detail
                        .InfFileName
                        .iter()
                        .position(|x| *x == 0)
                        .unwrap_or(detail.InfFileName.len());
                    let candidate_inf =
                        PathBuf::from(String::from_utf16_lossy(&detail.InfFileName[..inf_end]));
                    let expected_hardware = instance
                        .rsplit_once('\\')
                        .map(|(identity, _)| identity)
                        .unwrap_or(instance);
                    if provider.trim().eq_ignore_ascii_case(&entry.provider)
                        && hardware_id.trim().eq_ignore_ascii_case(expected_hardware)
                        && inf_bytes_match(&inf, &candidate_inf)
                    {
                        return Some(candidate);
                    }
                }
                None
            });
    let Some(mut selected_driver) = selected_driver else {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
        return SetupResult::Failed;
    };
    if unsafe { SetupDiSetSelectedDriverW(set, Some(&mut data), Some(&mut selected_driver)) }
        .is_err()
    {
        unsafe {
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
        return SetupResult::Failed;
    }
    let installed = unsafe { SetupDiInstallDevice(set, &mut data).is_ok() };
    let mut install_params = SP_DEVINSTALL_PARAMS_W {
        cbSize: std::mem::size_of::<SP_DEVINSTALL_PARAMS_W>() as u32,
        ..Default::default()
    };
    let reboot_required = unsafe {
        SetupDiGetDeviceInstallParamsW(set, Some(&data), &mut install_params).is_ok()
            && (install_params.Flags.0 & (DI_NEEDREBOOT.0 | DI_NEEDRESTART.0)) != 0
    };
    unsafe {
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    if !installed {
        return SetupResult::Failed;
    }
    if reboot_required {
        return SetupResult::RebootRequired;
    }
    if !manifest_matches_instance(instance, key, &entry) {
        return SetupResult::BindingFailed;
    }
    verify_installed_binding(instance, &entry)
}

#[cfg(target_os = "windows")]
fn verify_installed_binding(instance: &str, expected: &ManifestDevice) -> SetupResult {
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW, SetupDiGetDeviceRegistryPropertyW,
        DIGCF_ALLCLASSES, HDEVINFO, SPDRP_DRIVER, SPDRP_SERVICE, SP_DEVINFO_DATA,
    };
    use windows::Win32::Devices::Properties::{DEVPKEY_Device_DriverProvider, DEVPROP_TYPE_STRING};
    let Ok(set): Result<HDEVINFO, _> =
        (unsafe { SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES) })
    else {
        return SetupResult::BindingFailed;
    };
    let mut index = 0;
    let mut evidence = BindingEvidence {
        instance_present: false,
        service_winusb: false,
        provider_matches: false,
        inf_metadata_present: false,
        interface_instance_matches: false,
        interface_access: false,
        reboot_required: false,
        replug_pending: false,
    };
    loop {
        let mut data = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        if unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) }.is_err() {
            break;
        }
        let mut id = vec![0u16; 1024];
        let mut required = 0;
        if unsafe { SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut id), Some(&mut required)) }
            .is_ok()
        {
            let end = id.iter().position(|v| *v == 0).unwrap_or(id.len());
            if String::from_utf16_lossy(&id[..end]).eq_ignore_ascii_case(instance) {
                evidence.instance_present = true;
                let mut service = vec![0u8; 512];
                let mut driver = vec![0u8; 512];
                let mut service_len = 0;
                let mut driver_len = 0;
                let service_ok = unsafe {
                    SetupDiGetDeviceRegistryPropertyW(
                        set,
                        &data,
                        SPDRP_SERVICE,
                        None,
                        Some(&mut service),
                        Some(&mut service_len),
                    )
                }
                .is_ok();
                let driver_ok = unsafe {
                    SetupDiGetDeviceRegistryPropertyW(
                        set,
                        &data,
                        SPDRP_DRIVER,
                        None,
                        Some(&mut driver),
                        Some(&mut driver_len),
                    )
                }
                .is_ok();
                let service_text = String::from_utf16_lossy(unsafe {
                    std::slice::from_raw_parts(
                        service.as_ptr() as *const u16,
                        service_len as usize / 2,
                    )
                });
                let driver_text = String::from_utf16_lossy(unsafe {
                    std::slice::from_raw_parts(
                        driver.as_ptr() as *const u16,
                        driver_len as usize / 2,
                    )
                });
                let mut provider = vec![0u8; 512];
                let mut provider_type = DEVPROP_TYPE_STRING;
                let mut provider_len = 0;
                let provider_ok = unsafe {
                    SetupDiGetDevicePropertyW(
                        set,
                        &data,
                        &DEVPKEY_Device_DriverProvider,
                        &mut provider_type,
                        Some(&mut provider),
                        Some(&mut provider_len),
                        0,
                    )
                }
                .is_ok();
                let provider_text = String::from_utf16_lossy(unsafe {
                    std::slice::from_raw_parts(
                        provider.as_ptr() as *const u16,
                        provider_len as usize / 2,
                    )
                });
                evidence.service_winusb = service_ok
                    && service_text
                        .trim_end_matches('\0')
                        .eq_ignore_ascii_case("winusb");
                evidence.provider_matches = provider_ok
                    && provider_text
                        .trim_end_matches('\0')
                        .trim()
                        .eq_ignore_ascii_case(&expected.provider);
                evidence.inf_metadata_present =
                    driver_ok && !driver_text.trim_matches('\0').trim().is_empty();
                break;
            }
        }
        index += 1;
    }
    unsafe {
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    evidence.interface_access = interface_probe(instance, &expected.interface_guid);
    evidence.interface_instance_matches = evidence.interface_access;
    classify_binding(evidence)
}

#[cfg(not(target_os = "windows"))]
fn install_selected_device(_instance: &str, _package_root: &Path, _key: &str) -> SetupResult {
    SetupResult::Unavailable
}

#[cfg(not(target_os = "windows"))]
fn verify_installed_binding(_instance: &str, _expected: &ManifestDevice) -> SetupResult {
    SetupResult::Unavailable
}

fn run(instance: &str, key: &str) -> SetupResult {
    if !is_elevated() {
        return SetupResult::ElevationRequired;
    }
    if !allowed_instance(instance) {
        return SetupResult::Failed;
    }
    let Some(root) = fixed_package_root() else {
        return SetupResult::Unavailable;
    };
    if !package_asset_is_usable(&root, key) {
        return SetupResult::Unavailable;
    }
    install_selected_device(instance, &root, key)
}

fn parse_cli_args<I>(args: I) -> Option<(String, String)>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let instance = args.next()?;
    let key = args.next()?;
    if instance.is_empty() || key.is_empty() || args.next().is_some() {
        return None;
    }
    Some((instance, key))
}

fn exit_code(result: SetupResult) -> i32 {
    match result {
        SetupResult::Ready => 0,
        SetupResult::RebootRequired => 2,
        SetupResult::ReplugRequired | SetupResult::ReplugPending => 3,
        SetupResult::ElevationRequired => 4,
        SetupResult::Unavailable => 5,
        SetupResult::Failed => 6,
        SetupResult::BindingFailed => 7,
        SetupResult::Absent => 8,
    }
}

fn main() {
    let Some((instance, key)) = parse_cli_args(std::env::args().skip(1)) else {
        eprintln!("usage: reika-driver-setup.exe <selected-instance-id> <manifest-key>");
        std::process::exit(6);
    };
    let result = run(&instance, &key);
    println!("{result:?}");
    std::process::exit(exit_code(result));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_arbitrary_instance_and_key() {
        assert!(!allowed_instance("USB\\VID_1234&PID_5678\\x"));
        assert!(!package_asset_is_usable(Path::new("/tmp"), "../../evil"));
    }

    #[test]
    fn helper_cli_requires_exactly_instance_and_manifest_key() {
        let valid = parse_cli_args(vec![
            "USB\\VID_0483&PID_5840\\ABC".into(),
            "standard".into(),
        ]);
        assert_eq!(
            valid
                .as_ref()
                .map(|(instance, key)| (instance.as_str(), key.as_str())),
            Some(("USB\\VID_0483&PID_5840\\ABC", "standard"))
        );
        assert!(parse_cli_args(Vec::<String>::new()).is_none());
        assert!(parse_cli_args(vec!["instance".into()]).is_none());
        assert!(
            parse_cli_args(vec!["instance".into(), "standard".into(), "extra".into()]).is_none()
        );
        assert!(parse_cli_args(vec!["".into(), "standard".into()]).is_none());
    }

    #[test]
    fn helper_exit_codes_are_stable_and_distinguish_outcomes() {
        assert_eq!(exit_code(SetupResult::Ready), 0);
        assert_eq!(exit_code(SetupResult::RebootRequired), 2);
        assert_eq!(exit_code(SetupResult::ReplugPending), 3);
        assert_eq!(exit_code(SetupResult::ElevationRequired), 4);
        assert_eq!(exit_code(SetupResult::Unavailable), 5);
        assert_eq!(exit_code(SetupResult::Failed), 6);
        assert_eq!(exit_code(SetupResult::BindingFailed), 7);
        assert_eq!(exit_code(SetupResult::Absent), 8);
    }
    #[test]
    fn recognizes_only_allowlisted_hardware() {
        assert!(allowed_instance("USB\\VID_0483&PID_5840&MI_00\\ABC"));
        assert!(allowed_instance("USB\\VID_0FE6&PID_811E&MI_01\\ABC"));
    }

    fn manifest(
        preset: &str,
        vendor_id: &str,
        product_id: &str,
        interface: &str,
    ) -> ManifestDevice {
        ManifestDevice {
            preset: preset.into(),
            vendor_id: vendor_id.into(),
            product_id: product_id.into(),
            interface: interface.into(),
            interface_guid: "{00000000-0000-0000-0000-000000000000}".into(),
            provider: "Reviewed Provider".into(),
            signer_thumbprint: "00".repeat(32),
        }
    }

    #[test]
    fn manifest_policy_cross_matches_preset_vid_pid_and_interface() {
        let standard = manifest("standard", "0483", "5840", "MI_00");
        assert!(manifest_matches_instance(
            "USB\\VID_0483&PID_5840&MI_00\\ABC",
            "standard",
            &standard
        ));
        assert!(!manifest_matches_instance(
            "USB\\VID_0483&PID_5840&MI_01\\ABC",
            "standard",
            &standard
        ));
        assert!(!manifest_matches_instance(
            "USB\\VID_0FE6&PID_811E&MI_00\\ABC",
            "standard",
            &standard
        ));
        assert!(!manifest_matches_instance(
            "USB\\VID_0483&PID_5840\\ABC",
            "standard",
            &standard
        ));
    }

    #[test]
    fn manifest_policy_allows_only_explicit_noncomposite_leaf_mode() {
        let leaf = manifest("standard", "0483", "5840", "LEAF");
        assert!(manifest_matches_instance(
            "USB\\VID_0483&PID_5840\\ABC",
            "standard",
            &leaf
        ));
        assert!(!manifest_matches_instance(
            "USB\\VID_0483&PID_5840&MI_00\\ABC",
            "standard",
            &leaf
        ));
        let pending = manifest("standard", "0483", "5840", "CAPTURE_REQUIRED");
        assert!(!manifest_matches_instance(
            "USB\\VID_0483&PID_5840&MI_00\\ABC",
            "standard",
            &pending
        ));
    }

    #[test]
    fn selected_identity_rejects_wrong_hardware_parent_and_ambiguous_devices() {
        let composite = manifest("standard", "0483", "5840", "MI_00");
        let base = ObservedIdentity {
            hardware_ids: vec!["USB\\VID_0483&PID_5840&MI_00".into()],
            compatible_ids: vec!["USB\\Class_00".into()],
            parent: Some("USB\\VID_0483&PID_5840\\PARENT".into()),
            parent_service: Some("usbser".into()),
            service: Some("winusb".into()),
            ambiguous: false,
        };
        assert!(selected_identity_is_reviewed(
            "USB\\VID_0483&PID_5840&MI_00\\ABC",
            "standard",
            &composite,
            &base
        ));
        let mut wrong_parent = base.clone();
        wrong_parent.parent = Some("USB\\VID_1234&PID_5678\\PARENT".into());
        assert!(!selected_identity_is_reviewed(
            "USB\\VID_0483&PID_5840&MI_00\\ABC",
            "standard",
            &composite,
            &wrong_parent
        ));
        let mut root = base.clone();
        root.parent = Some("USB\\ROOT_HUB30\\ROOT".into());
        assert!(!selected_identity_is_reviewed(
            "USB\\VID_0483&PID_5840&MI_00\\ABC",
            "standard",
            &composite,
            &root
        ));
        let mut parent = base.clone();
        parent.service = Some("USBCCGP".into());
        assert!(!selected_identity_is_reviewed(
            "USB\\VID_0483&PID_5840&MI_00\\ABC",
            "standard",
            &composite,
            &parent
        ));
        let mut ambiguous = base;
        ambiguous.ambiguous = true;
        assert!(!selected_identity_is_reviewed(
            "USB\\VID_0483&PID_5840&MI_00\\ABC",
            "standard",
            &composite,
            &ambiguous
        ));

        let leaf = manifest("standard", "0483", "5840", "LEAF");
        let leaf_observed = ObservedIdentity {
            hardware_ids: vec!["USB\\VID_0483&PID_5840".into()],
            compatible_ids: vec!["USB\\Class_00".into()],
            parent: Some("USB\\ROOT_HUB30\\ROOT".into()),
            parent_service: Some("USBHUB3".into()),
            service: Some("winusb".into()),
            ambiguous: false,
        };
        assert!(selected_identity_is_reviewed(
            "USB\\VID_0483&PID_5840\\ABC",
            "standard",
            &leaf,
            &leaf_observed
        ));
        let mut unknown_leaf_parent = leaf_observed.clone();
        unknown_leaf_parent.parent = Some("HTREE\\ROOT\\0".into());
        assert!(!selected_identity_is_reviewed(
            "USB\\VID_0483&PID_5840\\ABC",
            "standard",
            &leaf,
            &unknown_leaf_parent
        ));
    }

    fn evidence() -> BindingEvidence {
        BindingEvidence {
            instance_present: true,
            service_winusb: true,
            provider_matches: true,
            inf_metadata_present: true,
            interface_instance_matches: true,
            interface_access: true,
            reboot_required: false,
            replug_pending: false,
        }
    }

    #[test]
    fn binding_result_requires_every_postinstall_check() {
        assert_eq!(classify_binding(evidence()), SetupResult::Ready);
        for mutate in [
            |e: &mut BindingEvidence| e.service_winusb = false,
            |e: &mut BindingEvidence| e.provider_matches = false,
            |e: &mut BindingEvidence| e.inf_metadata_present = false,
            |e: &mut BindingEvidence| e.interface_instance_matches = false,
            |e: &mut BindingEvidence| e.interface_access = false,
        ] {
            let mut value = evidence();
            mutate(&mut value);
            assert_eq!(classify_binding(value), SetupResult::BindingFailed);
        }
        let mut reboot = evidence();
        reboot.reboot_required = true;
        assert_eq!(classify_binding(reboot), SetupResult::RebootRequired);
        let mut replug = evidence();
        replug.replug_pending = true;
        assert_eq!(classify_binding(replug), SetupResult::ReplugPending);
    }

    #[test]
    fn signer_thumbprint_requires_exact_sha256_hex_and_matches_bytes() {
        let pin = "AA:bb ".to_string() + &"11".repeat(30);
        let bytes = [0xAA, 0xBB, 0x11]
            .into_iter()
            .chain([0x11; 29])
            .collect::<Vec<_>>();
        assert!(signer_pin_matches(&pin, &bytes));
        assert!(!signer_pin_matches("CAPTURE_REQUIRED", &bytes));
        assert!(!signer_pin_matches("00", &bytes));
        assert!(!signer_pin_matches(&("00".repeat(32) + "00"), &bytes));
    }

    #[test]
    fn bounded_utf16_decoder_rejects_missing_nul_and_truncation() {
        let mut bytes = vec![0xAA, 0xBB, 0, 0, b'A', 0, b'B', 0, 0, 0];
        assert_eq!(
            decode_bounded_utf16(&bytes, 4, bytes.len()).as_deref(),
            Some("AB")
        );
        assert!(decode_bounded_utf16(&bytes, 4, bytes.len() - 2).is_none());
        bytes[8] = b'X';
        assert!(decode_bounded_utf16(&bytes, 4, bytes.len()).is_none());
        assert!(decode_bounded_utf16(&bytes, 3, bytes.len()).is_none());

        let long = "受付🖨️".repeat(128);
        let mut long_bytes = vec![0xCC, 0xDD];
        long_bytes.extend(long.encode_utf16().flat_map(u16::to_ne_bytes));
        long_bytes.extend([0, 0]);
        assert_eq!(
            decode_bounded_utf16(&long_bytes, 2, long_bytes.len()).as_deref(),
            Some(long.as_str())
        );
    }

    #[test]
    fn inf_identity_accepts_oem_renamed_identical_bytes_only() {
        let root = std::env::temp_dir().join(format!("reika-inf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("standard.inf");
        let oem = root.join("oem42.inf");
        let other = root.join("oem43.inf");
        std::fs::write(&source, b"[Version]\nSignature=\"$Windows NT$\"\n").unwrap();
        std::fs::copy(&source, &oem).unwrap();
        std::fs::write(&other, b"different").unwrap();
        assert!(inf_bytes_match(&source, &oem));
        assert!(!inf_bytes_match(&source, &other));
        let _ = std::fs::remove_dir_all(root);
    }
}
