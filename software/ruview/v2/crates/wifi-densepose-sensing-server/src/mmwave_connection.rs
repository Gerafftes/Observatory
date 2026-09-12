//! Bounded, anonymous LAN discovery and actionable transport diagnostics.
//! Discovery never sends credentials or changes the host's network settings.
use std::io::Read;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ConnectionStatus {
    pub(crate) node_id: Option<String>,
    pub(crate) node_url: Option<String>,
    pub(crate) reachable: bool,
    pub(crate) target: Option<String>,
    pub(crate) receiver: Option<String>,
    pub(crate) hint: String,
}

pub(crate) fn load_token(env_name: &str) -> Option<String> {
    if let Ok(token) = std::env::var(env_name) {
        return (!token.trim().is_empty()).then(|| token.trim().to_string());
    }
    if env_name != "MMWAVE_NODE_TOKEN" {
        return None;
    }
    // Documented local secret locations; never serialize these contents.
    [
        "data/mmwave-node-token.txt",
        "../private/mmwave-ota-token.txt",
        "../../private/mmwave-ota-token.txt",
    ]
    .iter()
    .find_map(|path| {
        std::fs::read_to_string(path)
            .ok()
            .filter(|token| !token.trim().is_empty())
            .map(|token| token.trim().to_string())
    })
}

pub(crate) struct Probe {
    pub(crate) status: ConnectionStatus,
    pub(crate) diagnostics: Result<super::mmwave_calibration::NodeDiagnostics, String>,
}

fn private_neighbor(text: &str) -> Option<Ipv4Addr> {
    let ip: Ipv4Addr = text.trim_matches(|c| c == '(' || c == ')').parse().ok()?;
    (ip.is_private() && ip.octets()[3] != 0 && ip.octets()[3] != 255).then_some(ip)
}

fn neighbors() -> Vec<String> {
    let text = if cfg!(target_os = "linux") {
        std::fs::read_to_string("/proc/net/arp").unwrap_or_default()
    } else {
        std::process::Command::new("arp")
            .arg("-an")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default()
    };
    let mut urls: Vec<_> = text
        .split_whitespace()
        .filter_map(private_neighbor)
        .map(|ip| format!("http://{ip}:8032"))
        .collect();
    urls.sort();
    urls.dedup();
    urls.truncate(16);
    urls
}

fn read_status(url: &str) -> Result<Value, String> {
    let response = ureq::AgentBuilder::new()
        .redirects(0)
        .build()
        .get(&format!("{}/ota/status", url.trim_end_matches('/')))
        .timeout(Duration::from_millis(1200))
        .call()
        .map_err(|_| "mmWave-Knoten antwortet nicht auf /ota/status.".to_string())?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|_| "mmWave-Status konnte nicht gelesen werden.".to_string())?;
    if bytes.len() > 16_384 {
        return Err("mmWave-Status ist zu groß.".to_string());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Ungültiger mmWave-Status.".to_string())?;
    if value["sensor"] != "HLK-LD2450" || value["node_id"].as_str().is_none() {
        return Err("Kein HLK-LD2450-Knoten an dieser Adresse.".to_string());
    }
    Ok(value)
}

fn receiver_address(url: &str, port: u16) -> Option<String> {
    let authority = url.strip_prefix("http://")?.split('/').next()?;
    let ip: Ipv4Addr = authority.split(':').next()?.parse().ok()?;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // UDP connect only selects the outgoing interface; no packet is sent.
    socket.connect(SocketAddrV4::new(ip, 8032)).ok()?;
    Some(format!("{}:{port}", socket.local_addr().ok()?.ip()))
}

fn transport_hint(target: Option<&str>, receiver: Option<&str>, token: bool) -> String {
    let mut hints = Vec::new();
    if let (Some(target), Some(receiver)) = (target, receiver) {
        if target != receiver {
            hints.push(format!("Radar sendet an {target}; dieser Server empfängt unter {receiver}. Empfangs-IP bzw. UDP-Port angleichen. Diese Firmware bietet keine automatische Zieländerung."));
        }
    }
    if !token {
        hints.push("Radar-Adresse automatisch erkannt. Für Steuerbefehle fehlt MMWAVE_NODE_TOKEN; der reine UDP-Empfang braucht keinen Token.".to_string());
    }
    hints.join(" ")
}

pub(crate) fn probe(
    preferred: Option<&str>,
    expected_node: Option<&str>,
    port: u16,
    token: bool,
) -> Probe {
    let mut found =
        preferred.and_then(|url| read_status(url).ok().map(|value| (url.to_string(), value)));
    if found
        .as_ref()
        .is_some_and(|(_, value)| expected_node.is_some_and(|id| value["node_id"] != id))
    {
        found = None;
    }
    if found.is_none() {
        let candidates: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = neighbors()
                .into_iter()
                .map(|url| scope.spawn(move || read_status(&url).ok().map(|value| (url, value))))
                .collect();
            handles
                .into_iter()
                .filter_map(|handle| handle.join().ok().flatten())
                .filter(|(_, value)| expected_node.is_none_or(|id| value["node_id"] == id))
                .collect()
        });
        if candidates.len() == 1 {
            found = candidates.into_iter().next();
        } else if candidates.len() > 1 {
            return Probe {
                status: ConnectionStatus {
                    hint: "Mehrere Radar-Knoten gefunden. MMWAVE_NODE_URL explizit auswählen."
                        .to_string(),
                    ..Default::default()
                },
                diagnostics: Err("Radar-Auswahl ist mehrdeutig.".to_string()),
            };
        }
    }
    let Some((url, value)) = found else {
        return Probe {
            status: ConnectionStatus { hint: "Kein Radar-Knoten erreichbar. Stromversorgung und gemeinsames WLAN prüfen; bei unbekannter Adresse MMWAVE_NODE_URL setzen. Automatische Suche wird wiederholt.".to_string(), ..Default::default() },
            diagnostics: Err("Radar-Knoten nicht erreichbar.".to_string()),
        };
    };
    let target = value["target"].as_str().map(str::to_string);
    let receiver = receiver_address(&url, port);
    let mut hint = transport_hint(target.as_deref(), receiver.as_deref(), token);
    let diagnostics = serde_json::from_value(value["diagnostics"].clone()).map_err(|_| {
        "Knoten erreichbar; Firmware liefert keine UART-/Radar-Diagnosezähler.".to_string()
    });
    if diagnostics.is_err() {
        hint.push_str(" Firmware liefert keine Diagnosezähler; Sensorfunktion muss anhand empfangener UDP-Pakete geprüft werden.");
    }
    Probe {
        status: ConnectionStatus {
            node_id: value["node_id"].as_str().map(str::to_string),
            node_url: Some(url),
            reachable: true,
            target,
            receiver,
            hint,
        },
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_is_bounded_to_private_neighbors() {
        assert_eq!(
            private_neighbor("(192.168.4.2)"),
            Some(Ipv4Addr::new(192, 168, 4, 2))
        );
        for value in ["127.0.0.1", "8.8.8.8", "192.168.4.255", "garbage"] {
            assert_eq!(private_neighbor(value), None);
        }
    }

    #[test]
    fn diagnoses_ip_and_port_mismatch_without_rx() {
        let hint = transport_hint(Some("192.168.4.50:5010"), Some("192.168.4.3:5010"), false);
        assert!(hint.contains("192.168.4.50:5010"));
        assert!(hint.contains("192.168.4.3:5010"));
        assert!(hint.contains("MMWAVE_NODE_TOKEN"));
        assert!(
            !transport_hint(Some("192.168.4.3:5011"), Some("192.168.4.3:5010"), true).is_empty()
        );
        assert!(
            transport_hint(Some("192.168.4.3:5010"), Some("192.168.4.3:5010"), true).is_empty()
        );
    }
}
