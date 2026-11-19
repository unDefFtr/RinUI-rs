use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Desktop operating system family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsKind {
    Windows,
    MacOs,
    Linux,
    Other,
}

impl OsKind {
    /// Detect the current target operating system.
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(target_os = "macos")]
        {
            Self::MacOs
        }
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Self::Other
        }
    }
}

/// Parsed Windows NT version triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowsVersion {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
}

impl WindowsVersion {
    /// Windows 10 RTM or newer (build >= 10240).
    pub fn is_win10(self) -> bool {
        self.major > 10 || (self.major == 10 && self.build >= 10240)
    }

    /// Windows 11 21H2 or newer (build >= 22000).
    pub fn is_win11(self) -> bool {
        self.major > 10 || (self.major == 10 && self.build >= 22000)
    }

    /// Windows 11 22H2 or newer (build >= 22621), supporting `DWMWA_SYSTEMBACKDROP_TYPE`.
    pub fn is_win11_22h2(self) -> bool {
        self.major > 10 || (self.major == 10 && self.build >= 22621)
    }

    /// Query the real Windows version using `RtlGetVersion` (unaffected by manifest shims).
    #[cfg(target_os = "windows")]
    pub fn detect() -> Option<Self> {
        #[repr(C)]
        #[allow(non_snake_case)]
        struct OSVERSIONINFOW {
            dwOSVersionInfoSize: u32,
            dwMajorVersion: u32,
            dwMinorVersion: u32,
            dwBuildNumber: u32,
            dwPlatformId: u32,
            szCSDVersion: [u16; 128],
        }

        type RtlGetVersionFn = unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32;

        #[link(name = "kernel32")]
        extern "system" {
            fn GetModuleHandleW(lp_module_name: *const u16) -> *mut core::ffi::c_void;
            fn GetProcAddress(
                h_module: *mut core::ffi::c_void,
                lp_proc_name: *const u8,
            ) -> *const core::ffi::c_void;
        }

        // "ntdll.dll\0" in UTF-16
        const NTDLL_UTF16: [u16; 10] = [
            b'n' as u16,
            b't' as u16,
            b'd' as u16,
            b'l' as u16,
            b'l' as u16,
            b'.' as u16,
            b'd' as u16,
            b'l' as u16,
            b'l' as u16,
            0,
        ];

        unsafe {
            let module = GetModuleHandleW(NTDLL_UTF16.as_ptr());
            if module.is_null() {
                return None;
            }
            let proc = GetProcAddress(module, b"RtlGetVersion\0".as_ptr());
            if proc.is_null() {
                return None;
            }
            let rtl_get_version =
                std::mem::transmute::<*const core::ffi::c_void, RtlGetVersionFn>(proc);
            let mut info: OSVERSIONINFOW = std::mem::zeroed();
            info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
            if rtl_get_version(&mut info) == 0 {
                Some(Self {
                    major: info.dwMajorVersion,
                    minor: info.dwMinorVersion,
                    build: info.dwBuildNumber,
                })
            } else {
                None
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn detect() -> Option<Self> {
        None
    }
}

/// Configured RinUI theme preference (mirrors upstream `Theme` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Auto,
    Dark,
    Light,
}

impl ThemeMode {
    /// Canonical string representation expected by RinUI QML and `rin_ui.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    /// Parse exact upstream theme mode string (`"Auto"`, `"Dark"`, `"Light"`).
    pub fn parse_str(value: &str) -> Result<Self, ConfigError> {
        match value {
            "Auto" => Ok(Self::Auto),
            "Dark" => Ok(Self::Dark),
            "Light" => Ok(Self::Light),
            other => Err(ConfigError::InvalidTheme(other.to_owned())),
        }
    }
}

impl fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolved concrete color scheme (`Light` or `Dark`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolvedTheme {
    Light,
    Dark,
}

impl ResolvedTheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

impl fmt::Display for ResolvedTheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Window backdrop effect (mirrors upstream `BackdropEffect` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BackdropEffect {
    #[default]
    #[serde(rename = "none")]
    None,
    #[serde(rename = "acrylic")]
    Acrylic,
    #[serde(rename = "mica")]
    Mica,
    #[serde(rename = "tabbed")]
    Tabbed,
}

impl BackdropEffect {
    /// Canonical lowercase string expected by RinUI QML and `rin_ui.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Acrylic => "acrylic",
            Self::Mica => "mica",
            Self::Tabbed => "tabbed",
        }
    }

    /// Parse backdrop effect string (`"none"`, `"acrylic"`, `"mica"`, `"tabbed"`).
    pub fn parse_str(value: &str) -> Result<Self, ConfigError> {
        match value {
            "none" => Ok(Self::None),
            "acrylic" => Ok(Self::Acrylic),
            "mica" => Ok(Self::Mica),
            "tabbed" => Ok(Self::Tabbed),
            other => Err(ConfigError::InvalidBackdrop(other.to_owned())),
        }
    }
}

impl fmt::Display for BackdropEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when resolving a backdrop effect for a platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackdropSupportError {
    PlatformUnavailable,
    UnsupportedEffect(BackdropEffect),
}

impl BackdropSupportError {
    /// Upstream `ThemeManager.apply_backdrop_effect` return code (`-2` or `-1`).
    pub fn status_code(self) -> i32 {
        match self {
            Self::PlatformUnavailable => -2,
            Self::UnsupportedEffect(_) => -1,
        }
    }
}

/// Platform feature matrix derived from OS and Windows build version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub os: OsKind,
    pub windows_version: Option<WindowsVersion>,
}

impl PlatformCapabilities {
    /// Detect capabilities for the running platform.
    pub fn current() -> Self {
        let os = OsKind::current();
        let windows_version = if os == OsKind::Windows {
            WindowsVersion::detect()
        } else {
            None
        };
        Self {
            os,
            windows_version,
        }
    }

    /// Construct capabilities for an explicit OS and optional Windows version (useful in tests).
    pub fn for_os(os: OsKind, windows_version: Option<WindowsVersion>) -> Self {
        Self {
            os,
            windows_version,
        }
    }

    pub fn is_windows(&self) -> bool {
        self.os == OsKind::Windows
    }

    pub fn is_win10(&self) -> bool {
        self.is_windows() && self.windows_version.is_some_and(WindowsVersion::is_win10)
    }

    pub fn is_win11(&self) -> bool {
        self.is_windows() && self.windows_version.is_some_and(WindowsVersion::is_win11)
    }

    pub fn is_win11_22h2(&self) -> bool {
        self.is_windows()
            && self
                .windows_version
                .is_some_and(WindowsVersion::is_win11_22h2)
    }

    /// Default backdrop effect matching upstream `DEFAULT_CONFIG["backdrop_effect"]`.
    pub fn default_backdrop(&self) -> BackdropEffect {
        if self.is_win11() {
            BackdropEffect::Mica
        } else if self.is_win10() {
            BackdropEffect::Acrylic
        } else {
            BackdropEffect::None
        }
    }

    /// Whether the platform reports native support for `effect` before fallback.
    pub fn supports_backdrop(&self, effect: BackdropEffect) -> bool {
        match effect {
            BackdropEffect::None => true,
            BackdropEffect::Acrylic => self.is_win10(),
            BackdropEffect::Mica => self.is_win11(),
            BackdropEffect::Tabbed => self.is_win11_22h2(),
        }
    }

    /// Resolve a requested backdrop effect according to upstream `ThemeManager.apply_backdrop_effect`.
    pub fn resolve_backdrop(
        &self,
        requested: BackdropEffect,
    ) -> Result<BackdropEffect, BackdropSupportError> {
        if !self.is_windows() {
            return Err(BackdropSupportError::PlatformUnavailable);
        }

        // Win11 21H2 falls back to "mica" when "acrylic" or "tabbed" is requested.
        let effective = if self.is_win11()
            && !self.is_win11_22h2()
            && matches!(requested, BackdropEffect::Acrylic | BackdropEffect::Tabbed)
        {
            BackdropEffect::Mica
        } else {
            requested
        };

        if self.supports_backdrop(effective) {
            Ok(effective)
        } else {
            Err(BackdropSupportError::UnsupportedEffect(effective))
        }
    }

    /// Whether Windows 11 Snap Layouts are supported.
    pub fn supports_snap_layouts(&self) -> bool {
        self.is_win11()
    }

    /// Whether native macOS `NSWindow` titlebar integration is supported.
    pub fn supports_native_mac_frame(&self) -> bool {
        self.os == OsKind::MacOs
    }
}
