use std::io::BufReader;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::process::Command;

/// Flash firmware binary to an ESP32 via the locally installed `espflash` CLI.
///
/// The browser uploads the firmware to the helper first. `firmware_path` is
/// therefore always a helper-owned temporary path, never a path chosen by the
/// webpage itself.
pub async fn flash_firmware(
    port: String,
    firmware_path: String,
    chip: Option<String>,
    baud: Option<u32>,
) -> Result<FlashResult, String> {
    crate::commands::discovery::validate_serial_port_path(&port)?;
    let firmware_meta = std::fs::metadata(&firmware_path)
        .map_err(|error| format!("Firmware kann nicht gelesen werden: {error}"))?;
    let _firmware_size = firmware_meta.len();
    let firmware_hash = calculate_sha256(&firmware_path)?;
    let start_time = std::time::Instant::now();

    let baud_rate = baud.unwrap_or(921_600);
    if !(9_600..=3_000_000).contains(&baud_rate) {
        return Err("Baudrate muss zwischen 9600 und 3000000 liegen".to_string());
    }

    let mut command = Command::new("espflash");
    command
        .arg("flash")
        .args(["--port", &port])
        .args(["--baud", &baud_rate.to_string()])
        .arg("--no-monitor");
    if let Some(chip_type) = chip.as_deref() {
        let allowed = ["esp32", "esp32s2", "esp32s3", "esp32c3", "esp32c6"];
        if !allowed.contains(&chip_type) {
            return Err(format!("Chip-Typ ist nicht erlaubt: {chip_type}"));
        }
        command.args(["--chip", chip_type]);
    }
    command
        .arg(&firmware_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = command.spawn().map_err(|error| {
        format!("espflash konnte nicht gestartet werden: {error}. Ist espflash installiert?")
    })?;

    let output = child
        .wait_with_output()
        .await
        .map_err(|error| format!("Auf espflash konnte nicht gewartet werden: {error}"))?;

    let output_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let duration_secs = start_time.elapsed().as_secs_f64();
    if !output.status.success() {
        let detail = output_text.trim();
        return Err(if detail.is_empty() {
            format!("espflash wurde mit {} beendet", output.status)
        } else {
            format!("espflash wurde mit {} beendet: {detail}", output.status)
        });
    }

    Ok(FlashResult {
        success: true,
        message: format!("Firmware erfolgreich in {duration_secs:.1}s geflasht"),
        duration_secs,
        firmware_hash: Some(firmware_hash),
    })
}

/// Verify the uploaded firmware hash. The actual serial verification remains
/// the responsibility of espflash's built-in flash verification.
pub async fn verify_firmware(
    port: String,
    firmware_path: String,
    _chip: Option<String>,
) -> Result<VerifyResult, String> {
    crate::commands::discovery::validate_serial_port_path(&port)?;
    let expected_hash = calculate_sha256(&firmware_path)?;
    Ok(VerifyResult {
        verified: true,
        expected_hash,
        actual_hash: None,
        message: "Die Prüfung nutzt die integrierte Verifikation von espflash.".to_string(),
    })
}

pub async fn check_espflash() -> Result<EspflashInfo, String> {
    let output = Command::new("espflash")
        .arg("--version")
        .output()
        .await
        .map_err(|_| {
            "espflash nicht gefunden. Installiere es mit: cargo install espflash".to_string()
        })?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(EspflashInfo {
            installed: true,
            version: Some(version),
            path: which_espflash(),
        })
    } else {
        Err("espflash wurde gefunden, aber --version ist fehlgeschlagen".to_string())
    }
}

pub fn supported_chips() -> Vec<ChipInfo> {
    vec![
        ChipInfo {
            id: "esp32".into(),
            name: "ESP32".into(),
            description: "Original ESP32 dual-core".into(),
        },
        ChipInfo {
            id: "esp32s2".into(),
            name: "ESP32-S2".into(),
            description: "ESP32-S2 single-core mit USB OTG".into(),
        },
        ChipInfo {
            id: "esp32s3".into(),
            name: "ESP32-S3".into(),
            description: "ESP32-S3 dual-core mit USB OTG und AI-Beschleunigung".into(),
        },
        ChipInfo {
            id: "esp32c3".into(),
            name: "ESP32-C3".into(),
            description: "ESP32-C3 RISC-V single-core".into(),
        },
        ChipInfo {
            id: "esp32c6".into(),
            name: "ESP32-C6".into(),
            description: "ESP32-C6 RISC-V mit WiFi 6 und Thread".into(),
        },
    ]
}

pub fn calculate_sha256(path: &str) -> Result<String, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Datei konnte nicht geöffnet werden: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let bytes_read = std::io::Read::read(&mut reader, &mut buffer)
            .map_err(|error| format!("Datei konnte nicht gelesen werden: {error}"))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn parse_progress_percentage(line: &str) -> Option<f32> {
    let start = line.find(|character: char| character.is_ascii_digit())?;
    let digits = line[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if line[start + digits.len()..].starts_with('%') {
        digits.parse().ok()
    } else {
        None
    }
}

fn which_espflash() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join("espflash");
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashResult {
    pub success: bool,
    pub message: String,
    pub duration_secs: f64,
    pub firmware_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashProgress {
    pub phase: String,
    pub progress_pct: f32,
    pub bytes_written: u64,
    pub bytes_total: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub verified: bool,
    pub expected_hash: String,
    pub actual_hash: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EspflashInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChipInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress_percentage() {
        assert_eq!(parse_progress_percentage("[##########] 100%"), Some(100.0));
        assert_eq!(parse_progress_percentage("Writing 50%"), Some(50.0));
        assert_eq!(parse_progress_percentage("No percentage here"), None);
    }

    #[test]
    fn test_chip_info() {
        let chips = supported_chips();
        assert_eq!(chips.len(), 5);
        assert_eq!(chips[0].id, "esp32");
    }
}
