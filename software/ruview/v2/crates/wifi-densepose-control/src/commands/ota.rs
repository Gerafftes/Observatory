use std::time::Duration;

use hmac::{Hmac, Mac};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// OTA update port on ESP32 nodes.
pub const OTA_PORT: u16 = 8032;
const OTA_PATH: &str = "/ota/upload";
const OTA_TIMEOUT_SECS: u64 = 120;

type HmacSha256 = Hmac<Sha256>;

/// Push firmware to one node via HTTP OTA.
pub async fn ota_update(
    node_ip: String,
    firmware_path: String,
    psk: Option<String>,
) -> Result<OtaResult, String> {
    let start_time = std::time::Instant::now();
    let firmware_data = tokio::fs::read(&firmware_path)
        .await
        .map_err(|error| format!("Firmware konnte nicht gelesen werden: {error}"))?;
    let firmware_size = firmware_data.len();
    if firmware_size == 0 {
        return Err("Firmware-Datei ist leer".to_string());
    }

    let firmware_hash = sha256_bytes(&firmware_data);
    let signature = psk
        .filter(|key| !key.is_empty())
        .map(|key| {
            let mut mac = HmacSha256::new_from_slice(key.as_bytes())
                .map_err(|error| format!("Ungültiger OTA-PSK: {error}"))?;
            mac.update(&firmware_data);
            Ok::<_, String>(hex::encode(mac.finalize().into_bytes()))
        })
        .transpose()?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(OTA_TIMEOUT_SECS))
        .build()
        .map_err(|error| format!("HTTP-Client konnte nicht erstellt werden: {error}"))?;
    let firmware_part = Part::bytes(firmware_data)
        .file_name("firmware.bin")
        .mime_str("application/octet-stream")
        .map_err(|error| format!("Multipart konnte nicht erstellt werden: {error}"))?;
    let form = Form::new()
        .part("firmware", firmware_part)
        .text("sha256", firmware_hash.clone())
        .text("size", firmware_size.to_string());

    let url = format!("http://{}:{}{}", node_ip, OTA_PORT, OTA_PATH);
    let mut request = client
        .post(&url)
        .multipart(form)
        .header("X-OTA-SHA256", &firmware_hash);
    if let Some(signature) = signature {
        request = request.header("X-OTA-Signature", signature);
    }

    let response = request
        .send()
        .await
        .map_err(|error| format!("OTA-Upload fehlgeschlagen: {error}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("OTA fehlgeschlagen mit HTTP {status}: {body}"));
    }

    let reboot_ok = wait_for_reboot(&client, &node_ip, Duration::from_secs(30)).await;
    let duration_secs = start_time.elapsed().as_secs_f64();
    Ok(OtaResult {
        success: true,
        node_ip,
        message: if reboot_ok {
            format!("OTA erfolgreich in {duration_secs:.1}s abgeschlossen")
        } else {
            "OTA übertragen; die Neustart-Bestätigung ist abgelaufen".to_string()
        },
        firmware_hash: Some(firmware_hash),
        duration_secs: Some(duration_secs),
    })
}

/// Update multiple nodes sequentially or with bounded parallelism.
pub async fn batch_ota_update(
    node_ips: Vec<String>,
    firmware_path: String,
    psk: Option<String>,
    strategy: Option<String>,
    max_concurrent: Option<usize>,
) -> Result<BatchOtaResult, String> {
    let start_time = std::time::Instant::now();
    let total = node_ips.len();
    if total == 0 {
        return Ok(BatchOtaResult {
            total: 0,
            completed: 0,
            failed: 0,
            results: Vec::new(),
            duration_secs: 0.0,
        });
    }
    let strategy = strategy.unwrap_or_else(|| "sequential".to_string());
    if !["sequential", "tdm_safe", "parallel"].contains(&strategy.as_str()) {
        return Err(format!("Unbekannte OTA-Strategie: {strategy}"));
    }

    let mut results = Vec::with_capacity(total);
    if strategy == "parallel" {
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(
            max_concurrent.unwrap_or(1).clamp(1, 8),
        ));
        let firmware_path = std::sync::Arc::new(firmware_path);
        let psk = std::sync::Arc::new(psk);
        let tasks = node_ips.into_iter().map(|node_ip| {
            let semaphore = semaphore.clone();
            let firmware_path = firmware_path.clone();
            let psk = psk.clone();
            async move {
                let _permit = semaphore
                    .acquire_owned()
                    .await
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(ota_update(node_ip, (*firmware_path).clone(), (*psk).clone()).await)
            }
        });
        for result in futures::future::join_all(tasks).await {
            match result? {
                Ok(result) => results.push(result),
                Err(error) => results.push(OtaResult {
                    success: false,
                    node_ip: "unknown".to_string(),
                    message: error,
                    firmware_hash: None,
                    duration_secs: None,
                }),
            }
        }
    } else {
        for node_ip in node_ips {
            match ota_update(node_ip.clone(), firmware_path.clone(), psk.clone()).await {
                Ok(result) => results.push(result),
                Err(error) => results.push(OtaResult {
                    success: false,
                    node_ip,
                    message: error,
                    firmware_hash: None,
                    duration_secs: None,
                }),
            }
        }
    }

    let completed = results.iter().filter(|result| result.success).count();
    Ok(BatchOtaResult {
        total,
        completed,
        failed: total - completed,
        results,
        duration_secs: start_time.elapsed().as_secs_f64(),
    })
}

pub async fn check_ota_endpoint(node_ip: String) -> Result<OtaEndpointInfo, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|error| format!("HTTP-Client konnte nicht erstellt werden: {error}"))?;
    let url = format!("http://{}:{}/ota/status", node_ip, OTA_PORT);
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => {
            let body = response.text().await.unwrap_or_default();
            let version = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| value.get("version")?.as_str().map(ToString::to_string));
            Ok(OtaEndpointInfo {
                reachable: true,
                ota_supported: true,
                current_version: version,
                psk_required: false,
            })
        }
        Ok(response) => Ok(OtaEndpointInfo {
            reachable: true,
            ota_supported: response.status() != reqwest::StatusCode::NOT_FOUND,
            current_version: None,
            psk_required: response.status() == reqwest::StatusCode::UNAUTHORIZED,
        }),
        Err(_) => Ok(OtaEndpointInfo {
            reachable: false,
            ota_supported: false,
            current_version: None,
            psk_required: false,
        }),
    }
}

async fn wait_for_reboot(
    client: &reqwest::Client,
    node_ip: &str,
    timeout_duration: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout_duration;
    let url = format!("http://{}:{}/ota/status", node_ip, OTA_PORT);
    while tokio::time::Instant::now() < deadline {
        if client
            .get(&url)
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
        {
            return true;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    false
}

pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaResult {
    pub success: bool,
    pub node_ip: String,
    pub message: String,
    pub firmware_hash: Option<String>,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OtaProgress {
    pub node_ip: String,
    pub phase: String,
    pub progress_pct: f32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchOtaResult {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub results: Vec<OtaResult>,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchOtaProgress {
    pub phase: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub current_node: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OtaEndpointInfo {
    pub reachable: bool,
    pub ota_supported: bool,
    pub current_version: Option<String>,
    pub psk_required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_signature() {
        let mut mac = HmacSha256::new_from_slice(b"test_psk_key").unwrap();
        mac.update(b"firmware_hash");
        assert_eq!(hex::encode(mac.finalize().into_bytes()).len(), 64);
    }

    #[test]
    fn test_sha256_hash() {
        assert_eq!(sha256_bytes(b"test firmware data").len(), 64);
    }
}
