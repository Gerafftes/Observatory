use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// OTA update port on ESP32 nodes.
pub const OTA_PORT: u16 = 8032;
const OTA_PATH: &str = "/ota";

/// Validate the standard ESP-IDF app extent and its appended image digest.
/// File SHA, running-image SHA, and physical readback remain distinct evidence.
fn image_digest(data: &[u8]) -> Result<String, String> {
    if data.len() < 24
        || data.len() > 0x200000
        || data[0] != 0xe9
        || data[23] != 1
        || !(1..=16).contains(&data[1])
    {
        return Err("Kein unterstütztes ESP-IDF-Appimage mit SHA-256-Anhang im 2-MiB-Slot".into());
    }
    let mut offset = 24usize;
    let mut checksum = 0xefu8;
    for _ in 0..data[1] {
        let header = data
            .get(offset..offset + 8)
            .ok_or("Abgeschnittener Image-Segmentkopf")?;
        let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(8)
            .and_then(|v| v.checked_add(length))
            .ok_or("Image-Längenüberlauf")?;
        for byte in data
            .get(offset + 8..end)
            .ok_or("Abgeschnittenes Image-Segment")?
        {
            checksum ^= byte;
        }
        offset = end;
    }
    let digest_start = offset.checked_add(16).ok_or("Image-Längenüberlauf")? & !15;
    if data.get(digest_start - 1) != Some(&checksum) {
        return Err("Image-Prüfsumme stimmt nicht".into());
    }
    let digest = Sha256::digest(
        data.get(..digest_start)
            .ok_or("Abgeschnittene Image-Prüfsumme")?,
    );
    if data.get(digest_start..digest_start + 32) != Some(digest.as_slice()) {
        return Err("Image-SHA-256 stimmt nicht".into());
    }
    Ok(hex::encode(digest))
}

async fn bounded_json(mut response: reqwest::Response) -> Result<serde_json::Value, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "HTTP-Antwort unvollständig")?
    {
        if bytes.len() + chunk.len() > 16384 {
            return Err("Geräteantwort überschreitet 16 KiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Ungültige JSON-Geräteantwort".into())
}

async fn status(
    client: &reqwest::Client,
    origin: &str,
    token: &str,
) -> Result<serde_json::Value, String> {
    let response = client
        .get(format!("{origin}/ota/status"))
        .timeout(Duration::from_secs(3))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| "Authentisierte OTA-Statusabfrage fehlgeschlagen")?;
    if !response.status().is_success() {
        return Err(format!("OTA-Status HTTP {}", response.status()));
    }
    bounded_json(response).await
}

fn verified_boot(before: &serde_json::Value, after: &serde_json::Value, image_sha: &str) -> bool {
    let boot = |v: &serde_json::Value| {
        v.get("boot_id")
            .filter(|id| {
                id.as_str()
                    .is_some_and(|s| u64::from_str_radix(s, 16).is_ok_and(|v| v != 0))
                    || id.as_u64().is_some_and(|v| v != 0)
            })
            .cloned()
    };
    boot(before).zip(boot(after)).is_some_and(|(a, b)| a != b)
        && after.get("ota_state").and_then(|v| v.as_str()) == Some("VALID")
        && after.get("image_sha256").and_then(|v| v.as_str()) == Some(image_sha)
}

/// Exactly one raw, authenticated application upload. No automatic write retry.
pub async fn ota_update(
    node_ip: String,
    firmware_path: String,
    psk: Option<String>,
) -> Result<OtaResult, String> {
    let ip: std::net::IpAddr = node_ip.parse().map_err(|_| "Ungültige Geräte-IP")?;
    let data = tokio::fs::read(firmware_path)
        .await
        .map_err(|_| "Firmware kann nicht gelesen werden")?;
    let token = psk
        .filter(|key| !key.is_empty())
        .ok_or("Provisionierter OTA-Bearer-Token fehlt")?;
    let origin = format!("http://{}", std::net::SocketAddr::new(ip, OTA_PORT));
    update_at(node_ip, &origin, data, &token, Duration::from_secs(30)).await
}

async fn update_at(
    node_ip: String,
    origin: &str,
    data: Vec<u8>,
    token: &str,
    boot_timeout: Duration,
) -> Result<OtaResult, String> {
    let start = std::time::Instant::now();
    let image_sha = image_digest(&data)?;
    let file_sha = sha256_bytes(&data);
    let size = data.len();
    let upload_seconds = (size as u64).div_ceil(8192) + 5;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(upload_seconds + 5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "HTTP-Client konnte nicht erstellt werden")?;
    let before = status(&client, origin, token).await?;
    if before.get("ota_state").and_then(|v| v.as_str()) != Some("VALID") {
        return Err("Laufende App ist nicht nachweislich VALID; kein OTA gestartet".into());
    }
    if before
        .get("max_size")
        .and_then(|v| v.as_u64())
        .is_none_or(|v| v < size as u64)
    {
        return Err("Inaktiver App-Slot fehlt oder ist zu klein".into());
    }
    if before.get("protocol_version").and_then(|v| v.as_u64()) == Some(4) {
        let caps = client
            .get(format!("{origin}/capabilities"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| "RX-Capabilities fehlen")?;
        if !caps.status().is_success() {
            return Err("Authentisierte RX-Capabilities fehlen".into());
        }
        let caps = bounded_json(caps).await?;
        if caps.get("schema").and_then(|v| v.as_str()) != Some("observatory.rx-capabilities.v1")
            || caps.get("wire_version").and_then(|v| v.as_u64()) != Some(4)
            || caps.get("measurement_active").and_then(|v| v.as_bool()) != Some(false) {
            return Err("RX-Messlease aktiv oder unbekannt; kein OTA gestartet".into());
        }
        let response = client.get(format!("{origin}/config")).bearer_auth(token).send().await
            .map_err(|_| "Aktive RX-Konfiguration fehlt")?;
        if !response.status().is_success() { return Err("Aktive RX-Konfiguration fehlt".into()); }
        let config = bounded_json(response).await?;
        if config.get("schema").and_then(|v| v.as_str()) != Some("observatory.rx-config.v1")
            || config.get("reboot_required").and_then(|v| v.as_bool()) != Some(false) {
            return Err("RX-Konfiguration ist unbekannt oder wartet auf einen Boot; kein OTA gestartet".into());
        }
    }
    let response = client
        .post(format!("{origin}{OTA_PATH}"))
        .bearer_auth(token)
        .header("Content-Type", "application/octet-stream")
        .body(data)
        .send()
        .await
        .map_err(|_| {
            "OTA-Ausgang unklar; Status/Boot prüfen. Kein zweiter Upload wurde gestartet"
        })?;
    if !response.status().is_success() {
        return Err(format!(
            "OTA HTTP {}; kein Retry, gespeicherten Gerätezustand prüfen",
            response.status()
        ));
    }
    let receipt = bounded_json(response)
        .await
        .map_err(|_| "OTA-Antwort unklar; Status/Boot prüfen, nicht erneut senden")?;
    if receipt.get("status").and_then(|v| v.as_str()) != Some("ok")
        || receipt.get("received").and_then(|v| v.as_u64()) != Some(size as u64)
        || receipt.get("written").and_then(|v| v.as_u64()) != Some(size as u64)
    {
        return Err("OTA-Abschlussbeleg unvollständig; kein Retry, Gerätezustand prüfen".into());
    }
    let deadline = tokio::time::Instant::now() + boot_timeout;
    let mut boot_verified = false;
    while tokio::time::Instant::now() < deadline {
        if let Ok(after) = status(&client, origin, token).await {
            if verified_boot(&before, &after, &image_sha) {
                boot_verified = true;
                break;
            }
        }
        tokio::time::sleep(
            Duration::from_secs(1)
                .min(deadline.saturating_duration_since(tokio::time::Instant::now())),
        )
        .await;
    }
    Ok(OtaResult {
        success: boot_verified,
        upload_confirmed: true,
        boot_verified,
        node_ip,
        firmware_hash: Some(file_sha),
        duration_secs: Some(start.elapsed().as_secs_f64()),
        message: if boot_verified {
            "Upload abgeschlossen; neuer Boot mit passender Imagekennung und VALID belegt. Physischer Readback separat.".into()
        } else {
            "Upload abgeschlossen; neuer passender VALID-Boot nicht belegt. Nicht erneut senden; Gerätezustand prüfen.".into()
        },
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
                    upload_confirmed: false,
                    boot_verified: false,
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
                    upload_confirmed: false,
                    boot_verified: false,
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
        .no_proxy()
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
            psk_required: matches!(
                response.status(),
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
            ),
        }),
        Err(_) => Ok(OtaEndpointInfo {
            reachable: false,
            ota_supported: false,
            current_version: None,
            psk_required: false,
        }),
    }
}

pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaResult {
    pub upload_confirmed: bool,
    pub boot_verified: bool,
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
    fn test_sha256_hash() {
        assert_eq!(sha256_bytes(b"test firmware data").len(), 64);
    }
    fn synthetic_image() -> Vec<u8> {
        let mut image=vec![0;24]; image[0]=0xe9; image[1]=1; image[23]=1;
        image.extend_from_slice(&0x3c040000u32.to_le_bytes());
        image.extend_from_slice(&4u32.to_le_bytes()); image.extend_from_slice(b"test");
        let checksum=b"test".iter().fold(0xef,|a,b|a^b);
        let end=(image.len()+16)&!15; image.resize(end-1,0); image.push(checksum);
        let digest=Sha256::digest(&image); image.extend_from_slice(&digest); image
    }

    #[test]
    fn standard_image_digest_is_not_file_hash_and_rejects_corruption() {
        let image=synthetic_image(); let digest=image_digest(&image).unwrap();
        assert_ne!(digest,sha256_bytes(&image));
        for offset in [0,1,23,28,35,47,63] {
            let mut corrupted=image.clone(); corrupted[offset]^=1;
            assert!(image_digest(&corrupted).is_err(),"offset {offset}");
        }
        assert!(image_digest(&[]).is_err());
    }

    #[test]
    fn same_boot_pending_state_or_wrong_image_never_verify_a_reboot() {
        let before=serde_json::json!({"boot_id":"0000000000000007"});
        let correct=serde_json::json!({"boot_id":"0000000000000008","ota_state":"VALID","image_sha256":"expected"});
        assert!(verified_boot(&before,&correct,"expected"));
        assert!(!verified_boot(&before,&correct,"other"));
        let mut wrong=correct.clone(); wrong["boot_id"]=before["boot_id"].clone();
        assert!(!verified_boot(&before,&wrong,"expected"));
        wrong=correct; wrong["ota_state"]=serde_json::json!("PENDING_VERIFY");
        assert!(!verified_boot(&before,&wrong,"expected"));
    }

    #[tokio::test]
    async fn raw_bearer_upload_is_single_shot_and_lease_or_missing_boot_stays_unverified() {
        use axum::{extract::State, body::Bytes, http::HeaderMap, Json, routing::{get,post}, Router};
        use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
        struct Context { image:Vec<u8>, posts:AtomicUsize, lease:bool, wrong_image:bool }
        async fn status(State(c):State<Arc<Context>>,headers:HeaderMap)->Json<serde_json::Value> {
            assert_eq!(headers["authorization"],"Bearer test-token");
            let boot=if c.posts.load(Ordering::SeqCst)==0 {"0000000000000007"} else {"0000000000000008"};
            Json(serde_json::json!({"boot_id":boot,"protocol_version":4,"ota_state":"VALID","max_size":0x200000,
                "image_sha256":if c.wrong_image {"wrong".to_string()} else {image_digest(&c.image).unwrap()}}))
        }
        async fn caps(State(c):State<Arc<Context>>)->Json<serde_json::Value> {
            Json(serde_json::json!({"schema":"observatory.rx-capabilities.v1","wire_version":4,"measurement_active":c.lease}))
        }
        async fn config()->Json<serde_json::Value> { Json(serde_json::json!({"schema":"observatory.rx-config.v1","reboot_required":false})) }
        async fn upload(State(c):State<Arc<Context>>,headers:HeaderMap,body:Bytes)->Json<serde_json::Value> {
            assert_eq!(headers["authorization"],"Bearer test-token");
            assert_eq!(headers["content-type"],"application/octet-stream"); assert_eq!(&body[..],&c.image);
            assert_eq!(c.posts.fetch_add(1,Ordering::SeqCst),0);
            Json(serde_json::json!({"status":"ok","received":body.len(),"written":body.len()}))
        }
        for (lease,wrong_image) in [(false,false),(true,false),(false,true)] {
            let c=Arc::new(Context{image:synthetic_image(),posts:AtomicUsize::new(0),lease,wrong_image});
            let router=Router::new().route("/ota/status",get(status)).route("/capabilities",get(caps))
                .route("/config",get(config)).route("/ota",post(upload)).with_state(c.clone());
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin=format!("http://{}",listener.local_addr().unwrap());
            let server=tokio::spawn(async move { axum::serve(listener,router).await.unwrap() });
            let result=update_at("127.0.0.1".into(),&origin,c.image.clone(),"test-token",Duration::from_millis(20)).await;
            server.abort(); let _=server.await;
            if lease { assert!(result.is_err()); assert_eq!(c.posts.load(Ordering::SeqCst),0); }
            else { let result=result.unwrap(); assert!(result.upload_confirmed);
                assert_eq!(result.success,!wrong_image); assert_eq!(result.boot_verified,!wrong_image);
                assert_eq!(c.posts.load(Ordering::SeqCst),1); }
        }
    }

}
