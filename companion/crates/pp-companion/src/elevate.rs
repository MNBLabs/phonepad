//! The two things the companion hands to Windows' shell: running one narrow
//! action with administrator rights, and opening a link in the browser.

use std::os::windows::ffi::OsStrExt;

use windows::core::{w, HRESULT, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{
    ShellExecuteExW, ShellExecuteW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};

/// The command-line switch for the elevated action. The elevated copy of the
/// companion does exactly this and exits; it never opens a window or a socket.
pub const TRUST_NETWORK: &str = "--trust-network";

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// Run this executable again, elevated, with `args`, and wait for its exit
/// code. Windows shows its own consent prompt; declining it is an `Err`.
///
/// Elevating a copy of ourselves for one validated argument, rather than the
/// whole window, keeps the part that runs as administrator to a few lines.
pub fn run_self_elevated(args: &str) -> Result<u32, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate PhonePad: {e}"))?;
    let exe_w = wide(exe.as_os_str());
    let args_w = wide(std::ffi::OsStr::new(args));

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe_w.as_ptr()),
        lpParameters: PCWSTR(args_w.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
            "Windows did not get permission, so nothing was changed.".to_string()
        } else {
            format!("could not ask Windows for permission: {e}")
        });
    }
    if info.hProcess.is_invalid() {
        return Err("Windows did not start the change".into());
    }

    let mut code = 1u32;
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
    }
    Ok(code)
}

/// Open an https link in the default browser. Anything else is ignored, so a
/// value from the network can never become a command or a local file.
pub fn open_https(url: &str) {
    if !url.starts_with("https://") || url.chars().any(|c| c.is_control() || c == ' ') {
        return;
    }
    let url_w = wide(std::ffi::OsStr::new(url));
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(url_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}
