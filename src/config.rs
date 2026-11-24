use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const OFFICIAL_GITHUB_URL: &str = "https://github.com/AmgedAlfakih/CleanScreen-Guard";
pub const BUILTIN_MODEL_640M_ID: &str = "__builtin_nudenet_640m__";
pub const BUILTIN_MODEL_ID: &str = "__builtin_nudenet_320n__";
pub const NONE_MODEL_ID: &str = "";

const RUN_REG_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE_NAME: &str = "CleanScreenGuard";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub language: String,
    pub compute_device: String, // "power_saving" | "auto" | "high_performance" | "cpu"
    pub use_fp16: bool,

    pub primary_model: String,
    pub secondary_model: String,

    pub ai_scan_fps: u32,
    pub target_fps: u32,
    pub optical_flow_tracking: bool,
    pub adaptive_scan_rate: bool,
    pub scan_mode: String, // "fast" | "balanced" | "max_detail"

    pub confidence_threshold: f32,
    pub box_hold_seconds: f32,
    pub box_padding_px: i32,

    pub blur_strength: u32,
    pub debug_overlay: bool,

    pub blur_genitalia_exposed: bool,
    pub blur_breasts_exposed: bool,
    pub blur_buttocks_exposed: bool,
    pub blur_anus_exposed: bool,
    pub blur_bikini_covered: bool,
    pub blur_belly_exposed: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            compute_device: "power_saving".to_string(),
            use_fp16: true,
            primary_model: BUILTIN_MODEL_640M_ID.to_string(),
            secondary_model: NONE_MODEL_ID.to_string(),
            ai_scan_fps: 6,
            target_fps: 24,
            optical_flow_tracking: true,
            adaptive_scan_rate: true,
            scan_mode: "fast".to_string(),
            confidence_threshold: 0.22,
            box_hold_seconds: 0.35,
            box_padding_px: 28,
            blur_strength: 85,
            debug_overlay: false,
            blur_genitalia_exposed: true,
            blur_breasts_exposed: true,
            blur_buttocks_exposed: true,
            blur_anus_exposed: true,
            blur_bikini_covered: true,
            blur_belly_exposed: true,
        }
    }
}

impl AppConfig {
    pub fn active_labels(&self) -> HashSet<&'static str> {
        let mut labels = HashSet::new();
        if self.blur_genitalia_exposed {
            labels.insert("FEMALE_GENITALIA_EXPOSED");
            labels.insert("MALE_GENITALIA_EXPOSED");
        }
        if self.blur_breasts_exposed {
            labels.insert("FEMALE_BREAST_EXPOSED");
        }
        if self.blur_buttocks_exposed {
            labels.insert("BUTTOCKS_EXPOSED");
        }
        if self.blur_anus_exposed {
            labels.insert("ANUS_EXPOSED");
        }
        if self.blur_bikini_covered {
            labels.insert("FEMALE_GENITALIA_COVERED");
            labels.insert("FEMALE_BREAST_COVERED");
            labels.insert("BUTTOCKS_COVERED");
            labels.insert("ANUS_COVERED");
        }
        if self.blur_belly_exposed {
            labels.insert("BELLY_EXPOSED");
        }
        labels
    }

    pub fn load() -> Self {
        let path = get_settings_path();
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&text) {
                let lower = cfg.primary_model.to_lowercase();
                if lower.is_empty()
                    || lower.ends_with("640m.onnx")
                    || lower.ends_with("640m_fp16.onnx")
                {
                    cfg.primary_model = BUILTIN_MODEL_640M_ID.to_string();
                } else if lower.ends_with("320n.onnx") || lower.ends_with("320n_fp16.onnx") {
                    cfg.primary_model = BUILTIN_MODEL_ID.to_string();
                }
                return cfg;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = get_settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}

pub fn get_exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn get_data_dir() -> PathBuf {
    let exe_dir = get_exe_dir();
    if exe_dir.join("portable.txt").is_file() {
        return exe_dir;
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| exe_dir.clone());
    let dir = base.join("CleanScreenGuard");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn get_models_dir() -> PathBuf {
    let dir = get_data_dir().join("models");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn get_settings_path() -> PathBuf {
    get_data_dir().join("settings.json")
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_startup_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = to_wide(RUN_REG_PATH);
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), Some(0), KEY_READ, &mut hkey).is_err() {
            return false;
        }
        let val_name = to_wide(RUN_VALUE_NAME);
        let mut size: u32 = 0;
        let exists = RegQueryValueExW(hkey, PCWSTR(val_name.as_ptr()), None, None, None, Some(&mut size)).is_ok()
            && size > 2;
        let _ = RegCloseKey(hkey);
        exists
    }
}

pub fn set_startup_enabled(enabled: bool) {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = to_wide(RUN_REG_PATH);
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), Some(0), KEY_SET_VALUE, &mut hkey).is_err() {
            return;
        }
        let val_name = to_wide(RUN_VALUE_NAME);
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                let cmd = format!("\"{}\"", exe.display());
                let wide = to_wide(&cmd);
                let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                let _ = RegSetValueExW(hkey, PCWSTR(val_name.as_ptr()), Some(0), REG_SZ, Some(bytes));
            }
        } else {
            let _ = RegDeleteValueW(hkey, PCWSTR(val_name.as_ptr()));
        }
        let _ = RegCloseKey(hkey);
    }
}
