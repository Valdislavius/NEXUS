use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Thresholds {
    pub cpu_temperature: f64,
    pub gpu_temperature: f64,
    pub storage_temperature: f64,
    pub memory_load: f64,
}
impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cpu_temperature: 90.,
            gpu_temperature: 85.,
            storage_temperature: 60.,
            memory_load: 90.,
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Overlay {
    pub mode: String,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub opacity: f64,
    pub metric_ids: Vec<String>,
}
impl Default for Overlay {
    fn default() -> Self {
        Self {
            mode: "mini".into(),
            x: 24,
            y: 24,
            scale: 1.,
            opacity: 0.92,
            metric_ids: vec![
                "cpu.load",
                "cpu.temperature",
                "gpu.load",
                "gpu.temperature",
                "ram.load",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkeys {
    pub overlay: String,
    pub main: String,
}
impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            overlay: "Ctrl+Shift+F10".into(),
            main: "Ctrl+Shift+F9".into(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub profile: String,
    pub background_mode: bool,
    pub autostart: bool,
    pub close_behavior: String,
    pub retention_days: u32,
    pub overlay: Overlay,
    pub hotkeys: Hotkeys,
    pub thresholds: Thresholds,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: "balanced".into(),
            background_mode: true,
            autostart: false,
            close_behavior: "tray".into(),
            retention_days: 7,
            overlay: Overlay::default(),
            hotkeys: Hotkeys::default(),
            thresholds: Thresholds::default(),
        }
    }
}
pub struct Preferences {
    pub value: Mutex<Settings>,
    pub path: PathBuf,
}
impl Preferences {
    pub fn load(path: PathBuf) -> Self {
        let value = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .filter(|s| s.validate().is_ok())
            .unwrap_or_default();
        Self {
            value: Mutex::new(value),
            path,
        }
    }
    pub fn persist(&self, s: &Settings) -> Result<(), String> {
        s.validate()?;
        save_atomic(
            &self.path,
            &serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?,
        )
    }
}
pub fn save_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("tmp");
    use std::io::Write;
    let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
    Ok(())
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !["economy", "balanced", "detailed"].contains(&self.profile.as_str()) {
            return Err("Unknown monitoring profile".into());
        }
        if !["tray", "exit"].contains(&self.close_behavior.as_str())
            || !["mini", "advanced"].contains(&self.overlay.mode.as_str())
        {
            return Err("Invalid desktop mode".into());
        }
        if !(1..=90).contains(&self.retention_days)
            || !(0.5..=3.).contains(&self.overlay.scale)
            || !(0.2..=1.).contains(&self.overlay.opacity)
        {
            return Err("Settings are outside supported limits".into());
        }
        if self.overlay.x.unsigned_abs() > 32768
            || self.overlay.y.unsigned_abs() > 32768
            || self.overlay.metric_ids.is_empty()
            || self.overlay.metric_ids.len() > 12
        {
            return Err("Invalid overlay position or metrics".into());
        }
        if self.overlay.metric_ids.iter().any(|m| {
            ![
                "cpu.load",
                "cpu.temperature",
                "cpu.frequency",
                "gpu.load",
                "gpu.temperature",
                "gpu.memoryUsed",
                "ram.load",
                "ram.used",
                "system.uptime",
            ]
            .contains(&m.as_str())
        }) {
            return Err("Unsupported overlay metric".into());
        }
        for threshold in [
            self.thresholds.cpu_temperature,
            self.thresholds.gpu_temperature,
            self.thresholds.storage_temperature,
            self.thresholds.memory_load,
        ] {
            if !threshold.is_finite() || !(30.0..=110.0).contains(&threshold) {
                return Err("Thresholds must be between 30 and 110".into());
            }
        }
        if self.thresholds.memory_load > 100. {
            return Err("Memory threshold must not exceed 100%".into());
        }
        let overlay = parse_hotkey(&self.hotkeys.overlay)?;
        let main = parse_hotkey(&self.hotkeys.main)?;
        if overlay.is_some() && overlay == main {
            return Err("Hotkeys must be different".into());
        }
        Ok(())
    }
    pub fn interval_ms(&self) -> u64 {
        match self.profile.as_str() {
            "economy" => 5000,
            _ => 2000,
        }
    }
}
pub fn parse_hotkey(value: &str) -> Result<Option<(u32, u32)>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let mut modifiers = 0u32;
    let mut key = None;
    for part in value.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= 2,
            "shift" => modifiers |= 4,
            "alt" => modifiers |= 1,
            "win" | "super" => modifiers |= 8,
            k => {
                if key.is_some() {
                    return Err("Hotkey must contain one key".into());
                }
                let v = if k.len() == 1 && k.as_bytes()[0].is_ascii_alphanumeric() {
                    k.as_bytes()[0].to_ascii_uppercase() as u32
                } else if let Some(n) = k
                    .strip_prefix('f')
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|n| (1..=24).contains(n))
                {
                    0x70 + n - 1
                } else {
                    return Err("Use Ctrl, Shift, Alt, Win and A-Z, 0-9 or F1-F24".into());
                };
                key = Some(v);
            }
        }
    }
    let key = key.ok_or("Hotkey needs a key")?;
    if modifiers == 0 {
        return Err("Hotkey requires a modifier".into());
    }
    Ok(Some((modifiers | 0x4000, key)))
}
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::{ffi::c_void, os::windows::ffi::OsStrExt};
        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn RegCreateKeyExW(
                key: *mut c_void,
                path: *const u16,
                reserved: u32,
                class: *mut u16,
                options: u32,
                access: u32,
                security: *const c_void,
                result: *mut *mut c_void,
                disposition: *mut u32,
            ) -> i32;
            fn RegSetValueExW(
                key: *mut c_void,
                name: *const u16,
                reserved: u32,
                kind: u32,
                data: *const u8,
                size: u32,
            ) -> i32;
            fn RegDeleteValueW(key: *mut c_void, name: *const u16) -> i32;
            fn RegCloseKey(key: *mut c_void) -> i32;
        }
        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(Some(0)).collect()
        }
        let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let name = wide("NEXUS Desktop");
        let mut key = std::ptr::null_mut();
        let result = unsafe {
            RegCreateKeyExW(
                (-2147483647isize) as *mut c_void,
                path.as_ptr(),
                0,
                std::ptr::null_mut(),
                0,
                2,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if result != 0 {
            return Err(format!("Autostart registry error {result}"));
        }
        let result = if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let text = format!("\"{}\" --background", exe.display());
            let bytes: Vec<u16> = std::ffi::OsStr::new(&text)
                .encode_wide()
                .chain(Some(0))
                .collect();
            unsafe {
                RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    1,
                    bytes.as_ptr().cast(),
                    (bytes.len() * 2) as u32,
                )
            }
        } else {
            unsafe { RegDeleteValueW(key, name.as_ptr()) }
        };
        unsafe {
            RegCloseKey(key);
        }
        if result != 0 && !(result == 2 && !enabled) {
            return Err(format!("Autostart registry error {result}"));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("Windows autostart is unavailable".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hotkeys_are_validated_and_conflicts_rejected() {
        assert_eq!(
            parse_hotkey("Ctrl+Shift+F10").unwrap(),
            Some((0x4006, 0x79))
        );
        assert!(parse_hotkey("F1").is_err());
        assert!(parse_hotkey("Ctrl+X+Y").is_err());
        let mut s = Settings::default();
        s.hotkeys.main = s.hotkeys.overlay.clone();
        assert!(s.validate().is_err());
    }
    #[test]
    fn invalid_numbers_and_unbounded_retention_are_rejected() {
        let mut s = Settings::default();
        s.overlay.opacity = f64::NAN;
        assert!(s.validate().is_err());
        s = Settings::default();
        s.retention_days = 100;
        assert!(s.validate().is_err());
    }
}
