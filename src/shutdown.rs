//! Desligar o Windows com contagem cancelável.
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::*;
use windows::Win32::System::Shutdown::*;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub const COUNTDOWN_S: u32 = 30;

fn enable_privilege() -> bool {
    unsafe {
        let mut tok = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut tok).is_err() { return false; }
        let mut luid = LUID::default();
        let name: Vec<u16> = "SeShutdownPrivilege\0".encode_utf16().collect();
        let ok = LookupPrivilegeValueW(None, windows::core::PCWSTR(name.as_ptr()), &mut luid).is_ok();
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        let r = ok && AdjustTokenPrivileges(tok, false, Some(&tp), 0, None, None).is_ok();
        let _ = CloseHandle(tok);
        r
    }
}

/// `dry_run` (variável UPSPANEL_DRYRUN) só regista, não desliga.
pub fn dry_run() -> bool { std::env::var_os("UPSPANEL_DRYRUN").is_some() }

pub fn start(reason: &str) -> bool {
    if dry_run() { return true; }
    if !enable_privilege() { return false; }
    let mut msg: Vec<u16> = format!("Lite UPS Panel: {reason}. O computador vai desligar.\0").encode_utf16().collect();
    unsafe {
        InitiateSystemShutdownExW(
            None, PWSTR(msg.as_mut_ptr()), COUNTDOWN_S, true, false,
            SHUTDOWN_REASON(0x0006_0000 | 0x0000_000c), // MAJOR_POWER | MINOR_ENVIRONMENT
        ).is_ok()
    }
}

pub fn cancel() -> bool {
    if dry_run() { return true; }
    if !enable_privilege() { return false; }
    unsafe { AbortSystemShutdownW(None).is_ok() }
}
