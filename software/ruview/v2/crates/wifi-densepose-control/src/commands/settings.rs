use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Settings persisted by the local helper. Secrets are never returned by the
/// browser API; only the presence of an OTA key is exposed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub server_http_port: u16,
    pub server_ws_port: u16,
    pub server_udp_port: u16,
    pub bind_address: String,
    pub ui_path: String,
    #[serde(default)]
    pub ota_psk: String,
    pub auto_discover: bool,
    pub discover_interval_ms: u32,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            server_http_port: 8080,
            server_ws_port: 8765,
            server_udp_port: 5005,
            bind_address: "127.0.0.1".to_string(),
            ui_path: String::new(),
            ota_psk: String::new(),
            auto_discover: true,
            discover_interval_ms: 10_000,
            theme: "dark".to_string(),
        }
    }
}

pub fn default_settings_path() -> PathBuf {
    if let Some(path) = std::env::var_os("RUVIEW_CONTROL_CONFIG_DIR") {
        return PathBuf::from(path).join("settings.json");
    }
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("RuView")
                .join("settings.json");
        }
    }
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config_home)
            .join("ruview")
            .join("settings.json");
    }
    std::env::temp_dir().join("ruview").join("settings.json")
}

pub fn load(path: &Path) -> Result<Option<AppSettings>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|error| format!("Einstellungen konnten nicht gelesen werden: {error}"))?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|error| format!("Einstellungen sind ungültig: {error}"))
}

pub fn save(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Einstellungspfad hat kein Verzeichnis".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| {
        format!("Einstellungsverzeichnis konnte nicht erstellt werden: {error}")
    })?;
    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("Einstellungen konnten nicht serialisiert werden: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, contents)
        .map_err(|error| format!("Einstellungen konnten nicht geschrieben werden: {error}"))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("Einstellungen konnten nicht aktiviert werden: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_without_losing_fields() {
        let settings = AppSettings::default();
        let encoded = serde_json::to_string(&settings).unwrap();
        let decoded: AppSettings = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.server_http_port, 8080);
        assert!(decoded.auto_discover);
    }
}
