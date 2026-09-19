//! Authenticated LAN discovery for the HLK-LD2450 transport.
//!
//! The ESP32 node keeps using DHCP.  It periodically broadcasts a small
//! request so a collector can answer from whatever address DHCP assigned to
//! it.  The response is authenticated with the same secret already used by
//! the node's HTTP control endpoints; an unauthenticated packet can therefore
//! never overwrite the persisted UDP target.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

pub(crate) const DEFAULT_DISCOVERY_PORT: u16 = 5011;
pub(crate) const REQUEST_SCHEMA: &str = "ruview.mmwave.discovery.request.v1";
pub(crate) const RESPONSE_SCHEMA: &str = "ruview.mmwave.discovery.response.v1";
const MAX_PACKET_BYTES: usize = 1024;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiscoveryRequest {
    pub(crate) schema: String,
    pub(crate) node_id: String,
    pub(crate) nonce: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiscoveryResponse {
    pub(crate) schema: String,
    pub(crate) node_id: String,
    pub(crate) nonce: u32,
    pub(crate) collector_port: u16,
    pub(crate) auth: String,
}

pub(crate) fn spawn_listener(discovery_port: u16, collector_port: u16, token: Option<String>) {
    tokio::spawn(async move {
        let Some(token) = token.filter(|token| !token.is_empty()) else {
            warn!("mmWave discovery disabled because no bearer token is configured");
            return;
        };
        let address = format!("0.0.0.0:{discovery_port}");
        let socket = match UdpSocket::bind(&address).await {
            Ok(socket) => socket,
            Err(error) => {
                warn!("Could not bind mmWave discovery listener to {address}: {error}");
                return;
            }
        };
        info!(
            "mmWave authenticated discovery listening on {address}; collector UDP port={collector_port}"
        );
        serve(socket, collector_port, token).await;
    });
}

async fn serve(socket: UdpSocket, collector_port: u16, token: String) {
    let mut buffer = [0_u8; MAX_PACKET_BYTES];
    loop {
        let (length, source) = match socket.recv_from(&mut buffer).await {
            Ok(result) => result,
            Err(error) => {
                warn!("mmWave discovery receive failed: {error}");
                continue;
            }
        };
        let Some(request) = parse_request(&buffer[..length]) else {
            debug!("Ignored invalid mmWave discovery request from {source}");
            continue;
        };
        let response = response_for(&request, collector_port, &token);
        let payload = match serde_json::to_vec(&response) {
            Ok(payload) => payload,
            Err(error) => {
                warn!("Could not serialize mmWave discovery response: {error}");
                continue;
            }
        };
        if let Err(error) = socket.send_to(&payload, source).await {
            warn!("Could not send mmWave discovery response to {source}: {error}");
        }
    }
}

pub(crate) fn parse_request(bytes: &[u8]) -> Option<DiscoveryRequest> {
    if bytes.is_empty() || bytes.len() > MAX_PACKET_BYTES {
        return None;
    }
    let request = serde_json::from_slice::<DiscoveryRequest>(bytes).ok()?;
    (request.schema == REQUEST_SCHEMA
        && !request.node_id.is_empty()
        && request.node_id.len() <= 23
        && request
            .node_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:.".contains(&byte)))
    .then_some(request)
}

pub(crate) fn response_for(
    request: &DiscoveryRequest,
    collector_port: u16,
    token: &str,
) -> DiscoveryResponse {
    let auth = hmac_hex(token, &auth_message(request, collector_port));
    DiscoveryResponse {
        schema: RESPONSE_SCHEMA.to_string(),
        node_id: request.node_id.clone(),
        nonce: request.nonce,
        collector_port,
        auth,
    }
}

pub(crate) fn verify_response(
    response: &DiscoveryResponse,
    expected_node_id: &str,
    expected_nonce: u32,
    token: &str,
) -> bool {
    response.schema == RESPONSE_SCHEMA
        && response.node_id == expected_node_id
        && response.nonce == expected_nonce
        && response.collector_port != 0
        && response.auth
            == hmac_hex(
                token,
                &auth_message(
                    &DiscoveryRequest {
                        schema: REQUEST_SCHEMA.to_string(),
                        node_id: expected_node_id.to_string(),
                        nonce: expected_nonce,
                    },
                    response.collector_port,
                ),
            )
}

fn auth_message(request: &DiscoveryRequest, collector_port: u16) -> String {
    format!(
        "{RESPONSE_SCHEMA}\n{}\n{}\n{collector_port}",
        request.node_id, request.nonce
    )
}

fn hmac_hex(token: &str, message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(token.as_bytes())
        .expect("HMAC-SHA256 accepts keys of every length");
    mac.update(message.as_bytes());
    let bytes = mac.finalize().into_bytes();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::time::{timeout, Duration};

    fn request() -> DiscoveryRequest {
        DiscoveryRequest {
            schema: REQUEST_SCHEMA.to_string(),
            node_id: "MMWAVE1".to_string(),
            nonce: 0x1234_5678,
        }
    }

    #[test]
    fn response_authenticates_exact_node_and_nonce() {
        let request = request();
        let response = response_for(&request, 5010, "test-secret");
        assert!(verify_response(
            &response,
            "MMWAVE1",
            0x1234_5678,
            "test-secret"
        ));
        assert!(!verify_response(
            &response,
            "OTHER",
            0x1234_5678,
            "test-secret"
        ));
        assert!(!verify_response(
            &response,
            "MMWAVE1",
            0x1234_5679,
            "test-secret"
        ));
        assert!(!verify_response(
            &response,
            "MMWAVE1",
            0x1234_5678,
            "wrong-secret"
        ));
    }

    #[test]
    fn parser_rejects_wrong_schema_and_unsafe_node_ids() {
        let valid = serde_json::to_vec(&request()).expect("request serializes");
        assert!(parse_request(&valid).is_some());

        let wrong_schema = br#"{"schema":"other","node_id":"MMWAVE1","nonce":1}"#;
        assert!(parse_request(wrong_schema).is_none());
        let unsafe_id =
            br#"{"schema":"ruview.mmwave.discovery.request.v1","node_id":"MMWAVE/1","nonce":1}"#;
        assert!(parse_request(unsafe_id).is_none());
    }

    #[tokio::test]
    async fn listener_answers_a_real_local_udp_request() {
        let server = UdpSocket::bind("127.0.0.1:0").await.expect("server binds");
        let server_address = server.local_addr().expect("server address");
        let server_task = tokio::spawn(serve(server, 5010, "test-secret".to_string()));

        let client = UdpSocket::bind("127.0.0.1:0").await.expect("client binds");
        let request = request();
        let payload = serde_json::to_vec(&request).expect("request serializes");
        client
            .send_to(&payload, server_address)
            .await
            .expect("request sends");
        let mut response_bytes = [0_u8; MAX_PACKET_BYTES];
        let (length, source): (usize, SocketAddr) = timeout(
            Duration::from_secs(1),
            client.recv_from(&mut response_bytes),
        )
        .await
        .expect("response arrives")
        .expect("response receives");
        assert_eq!(source, server_address);
        let response: DiscoveryResponse =
            serde_json::from_slice(&response_bytes[..length]).expect("response parses");
        assert!(verify_response(
            &response,
            "MMWAVE1",
            0x1234_5678,
            "test-secret"
        ));
        server_task.abort();
    }
}
