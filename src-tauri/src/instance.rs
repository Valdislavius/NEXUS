// A session-local mutex prevents parallel app instances and duplicate Collectors.
// No service, ACL or system configuration is modified.
#[cfg(windows)]
pub struct Instance(*mut std::ffi::c_void);
#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(attributes: *const std::ffi::c_void, owner: i32, name: *const u16) -> *mut std::ffi::c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}
#[cfg(windows)]
impl Instance {
    pub fn acquire() -> Option<Self> {
        let name: Vec<u16> = "Local\\NEXUS.Desktop.Monitor".encode_utf16().chain(Some(0)).collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        let error = unsafe { GetLastError() };
        if handle.is_null() { return None; }
        if error == 183 { unsafe { CloseHandle(handle); } return None; }
        Some(Self(handle))
    }
}
#[cfg(windows)]
impl Drop for Instance { fn drop(&mut self) { unsafe { CloseHandle(self.0); } } }
#[cfg(not(windows))]
pub struct Instance;
#[cfg(not(windows))]
impl Instance { pub fn acquire() -> Option<Self> { Some(Self) } }
