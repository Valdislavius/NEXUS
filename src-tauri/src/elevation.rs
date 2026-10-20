// The release manifest is primary. This guard also verifies the effective token
// before the app mutex, WebView, or Collector can be created. It only requests
// Windows UAC for the current executable, once, with a fixed retry argument.

#[cfg(any(test, all(windows, not(debug_assertions))))]
const RETRY_FLAG: &str = "--nexus-uac-retry";

#[cfg(any(test, all(windows, not(debug_assertions))))]
#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Continue,
    RequestElevation,
    StopRetry,
    StopQueryFailure,
}

#[cfg(any(test, all(windows, not(debug_assertions))))]
fn decision(token_elevation: Result<bool, u32>, already_retried: bool) -> Decision {
    match token_elevation {
        Ok(true) => Decision::Continue,
        Ok(false) if !already_retried => Decision::RequestElevation,
        Ok(false) => Decision::StopRetry,
        Err(_) => Decision::StopQueryFailure,
    }
}

#[cfg(any(test, all(windows, not(debug_assertions))))]
fn retry_flag_present(args: impl Iterator<Item = std::ffi::OsString>) -> bool {
    args.into_iter().any(|arg| arg == std::ffi::OsStr::new(RETRY_FLAG))
}

// Development sessions keep their usual token; release monitoring must verify
// administrator elevation. A non-Windows release has no supported UAC path.
#[cfg(debug_assertions)]
pub fn ensure_elevated() -> bool { true }

#[cfg(all(not(debug_assertions), not(windows)))]
pub fn ensure_elevated() -> bool { false }

#[cfg(all(windows, not(debug_assertions)))]
pub fn ensure_elevated() -> bool { native::ensure_elevated() }

#[cfg(all(windows, not(debug_assertions)))]
mod native {
    use super::{decision, retry_flag_present, Decision, RETRY_FLAG};
    use std::{ffi::{c_void, OsStr}, os::windows::ffi::OsStrExt, ptr};

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION: u32 = 20;
    const ERROR_INVALID_DATA: u32 = 13;
    const ERROR_CANCELLED: u32 = 1223;
    const COINIT_APARTMENTTHREADED_DISABLE_OLE1DDE: u32 = 0x2 | 0x4;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
        fn GetTokenInformation(token: *mut c_void, class: u32, info: *mut c_void, size: u32, returned: *mut u32) -> i32;
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(window: *mut c_void, verb: *const u16, file: *const u16, parameters: *const u16, directory: *const u16, show: i32) -> *mut c_void;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut c_void, text: *const u16, caption: *const u16, flags: u32) -> i32;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
        fn CoUninitialize();
    }

    struct Token(*mut c_void);
    impl Drop for Token {
        fn drop(&mut self) { unsafe { CloseHandle(self.0); } }
    }
    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) { unsafe { CoUninitialize(); } }
    }

    fn wide(value: &OsStr) -> Option<Vec<u16>> {
        let mut units: Vec<u16> = value.encode_wide().collect();
        if units.contains(&0) { return None; }
        units.push(0);
        Some(units)
    }

    fn show_failure(message: &str) {
        // Only fixed messages reach this function; never display OS paths,
        // user identifiers, command-line arguments, or provider error text.
        let text = wide(OsStr::new(message)).expect("fixed UAC message");
        let caption = wide(OsStr::new("NEXUS")).expect("fixed caption");
        unsafe { MessageBoxW(ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10); }
    }

    fn token_is_elevated() -> Result<bool, u32> {
        let mut handle = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
            return Err(unsafe { GetLastError() });
        }
        if handle.is_null() { return Err(ERROR_INVALID_DATA); }
        let token = Token(handle);
        // TOKEN_ELEVATION consists of one DWORD TokenIsElevated.
        let mut elevated: u32 = 0;
        let mut returned: u32 = 0;
        let size = std::mem::size_of::<u32>() as u32;
        if unsafe { GetTokenInformation(token.0, TOKEN_ELEVATION, (&mut elevated as *mut u32).cast(), size, &mut returned) } == 0 {
            return Err(unsafe { GetLastError() });
        }
        if returned != size { return Err(ERROR_INVALID_DATA); }
        Ok(elevated != 0)
    }

    fn request_current_exe_elevation() {
        let Ok(exe) = std::env::current_exe() else {
            eprintln!("NEXUS UAC: CURRENT_EXE_UNAVAILABLE");
            show_failure("Не удалось определить приложение для запроса прав администратора. Запуск отменён.");
            return;
        };
        let Some(parent) = exe.parent().filter(|_| exe.is_absolute()) else {
            show_failure("Не удалось определить приложение для запроса прав администратора. Запуск отменён.");
            return;
        };
        let (Some(file), Some(directory)) = (wide(exe.as_os_str()), wide(parent.as_os_str())) else {
            show_failure("Не удалось определить приложение для запроса прав администратора. Запуск отменён.");
            return;
        };
        let verb = wide(OsStr::new("runas")).expect("fixed UAC verb");
        let arguments = wide(OsStr::new(RETRY_FLAG)).expect("fixed retry flag");
        let com_status = unsafe { CoInitializeEx(ptr::null_mut(), COINIT_APARTMENTTHREADED_DISABLE_OLE1DDE) };
        if com_status < 0 {
            eprintln!("NEXUS UAC: COM_INITIALIZE_FAILED HRESULT_{:08X}", com_status as u32);
            show_failure("Не удалось подготовить запрос прав администратора. Запуск NEXUS отменён.");
            return;
        }
        let _com = ComApartment;
        // No original arguments are forwarded. The target is always this
        // executable; the Collector inherits the elevated app token later.
        let result = unsafe { ShellExecuteW(ptr::null_mut(), verb.as_ptr(), file.as_ptr(), arguments.as_ptr(), directory.as_ptr(), 1) } as isize;
        if result <= 32 {
            let error = unsafe { GetLastError() };
            eprintln!("NEXUS UAC: SHELL_EXECUTE_{result} WIN32_{error}");
            if error != ERROR_CANCELLED {
                show_failure("Windows не предоставила права администратора. Запуск NEXUS отменён.");
            }
        }
    }

    pub(super) fn ensure_elevated() -> bool {
        let elevation = token_is_elevated();
        let retried = retry_flag_present(std::env::args_os().skip(1));
        match decision(elevation, retried) {
            Decision::Continue => true,
            Decision::RequestElevation => {
                request_current_exe_elevation();
                // Original process stops before it owns the instance mutex or
                // creates UI/Collector, including on cancellation or failure.
                false
            }
            Decision::StopRetry => {
                eprintln!("NEXUS UAC: ELEVATION_RETRY_NOT_ELEVATED");
                show_failure("Windows не предоставила права администратора. Запуск NEXUS отменён.");
                false
            }
            Decision::StopQueryFailure => {
                if let Err(code) = elevation { eprintln!("NEXUS UAC: TOKEN_QUERY_FAILED WIN32_{code}"); }
                show_failure("Не удалось проверить права администратора. Запуск NEXUS отменён.");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elevation_decision_requires_effective_elevated_token() {
        assert_eq!(decision(Ok(true), false), Decision::Continue);
        assert_eq!(decision(Ok(true), true), Decision::Continue);
        assert_eq!(decision(Ok(false), false), Decision::RequestElevation);
        assert_eq!(decision(Ok(false), true), Decision::StopRetry);
        assert_eq!(decision(Err(5), false), Decision::StopQueryFailure);
        assert_eq!(decision(Err(5), true), Decision::StopQueryFailure);
    }
    #[test]
    fn retry_flag_is_exact_and_never_implies_elevation() {
        assert!(retry_flag_present([std::ffi::OsString::from(RETRY_FLAG)].into_iter()));
        assert!(!retry_flag_present([std::ffi::OsString::from("--nexus-uac-retry=1"), std::ffi::OsString::from("prefix--nexus-uac-retry")].into_iter()));
        assert_eq!(decision(Ok(false), true), Decision::StopRetry);
    }
}
