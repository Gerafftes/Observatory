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
        "../../../private/mmwave-ota-token.txt",
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
    neighbor_urls(&text)
}

fn neighbor_urls(text: &str) -> Vec<String> {
    let mut urls: Vec<_> = text
        .split_whitespace()
        .filter_map(private_neighbor)
        .map(|ip| format!("http://{ip}:8032"))
        .collect();
    urls.sort();
    urls.dedup();
    urls
}

fn read_status(url: &str) -> Result<Value, String> {
    let response = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(Duration::from_millis(1200))
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

fn receiver_address(url: &str, bind_addr: Ipv4Addr, port: u16) -> Option<String> {
    if !bind_addr.is_unspecified() {
        return Some(SocketAddrV4::new(bind_addr, port).to_string());
    }
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
            if token {
                hints.push(format!("Radar sendet an {target}; dieser Server empfängt unter {receiver}. Der Server versucht, das Ziel über den authentifizierten Transport-Fix automatisch anzugleichen."));
            } else {
                hints.push(format!("Radar sendet an {target}; dieser Server empfängt unter {receiver}. Ohne MMWAVE_NODE_TOKEN kann der Server das Ziel nur erkennen, nicht ändern; das Radar-Ziel bleibt unverändert."));
            }
        }
    }
    if !token {
        hints.push("Radar-Adresse automatisch erkannt. Für Steuerbefehle fehlt MMWAVE_NODE_TOKEN; der reine UDP-Empfang braucht keinen Token.".to_string());
    }
    hints.join(" ")
}

fn repair_transport(url: &str, token: &str, receiver: &str) -> Result<(), String> {
    let (target_host, target_port) = receiver
        .rsplit_once(':')
        .ok_or_else(|| "lokale UDP-Empfangsadresse ist ungültig".to_string())?;
    let target_port = target_port
        .parse::<u16>()
        .map_err(|_| "lokaler UDP-Port ist ungültig".to_string())?;
    let payload = serde_json::json!({
        "target_host": target_host,
        "target_port": target_port,
    });
    let response = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_millis(1500))
        .build()
        .put(&format!("{}/transport", url.trim_end_matches('/')))
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_string(&payload.to_string())
        .map_err(|error| format!("HTTP-Fehler beim automatischen Transport-Fix: {error}"))?;
    if !(200..300).contains(&response.status()) {
        return Err(format!("HTTP {} beim automatischen Transport-Fix", response.status()));
    }
    Ok(())
}

pub(crate) fn probe(
    preferred: Option<&str>,
    expected_node: Option<&str>,
    bind_addr: Ipv4Addr,
    port: u16,
    token: Option<&str>,
    auto_repair: bool,
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
        let mut candidates = Vec::new();
        // Bound concurrent requests, not the set of discoverable nodes. On
        // hosts with LAN and WiFi, the radar can sort after 16 LAN neighbors.
        for batch in neighbors().chunks(16) {
            candidates.extend(std::thread::scope(|scope| {
                let handles: Vec<_> = batch
                    .iter()
                    .map(|url| {
                        scope.spawn(move || read_status(url).ok().map(|value| (url.clone(), value)))
                    })
                    .collect();
                handles
                    .into_iter()
                    .filter_map(|handle| handle.join().ok().flatten())
                    .filter(|(_, value)| expected_node.is_none_or(|id| value["node_id"] == id))
                    .collect::<Vec<_>>()
            }));
        }
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
    let Some((url, mut value)) = found else {
        return Probe {
            status: ConnectionStatus { hint: "Kein Radar-Knoten erreichbar. Stromversorgung und gemeinsames WLAN prüfen; bei unbekannter Adresse MMWAVE_NODE_URL setzen. Automatische Suche wird wiederholt.".to_string(), ..Default::default() },
            diagnostics: Err("Radar-Knoten nicht erreichbar.".to_string()),
        };
    };
    let target = value["target"].as_str().map(str::to_string);
    let receiver = receiver_address(&url, bind_addr, port);
    let mut repair_hint = None;
    if auto_repair {
        if let (Some(target), Some(receiver), Some(token)) =
            (target.as_deref(), receiver.as_deref(), token)
        {
            if target != receiver {
                match repair_transport(&url, token, receiver) {
                    Ok(()) => match read_status(&url) {
                        Ok(updated) => {
                            value = updated;
                            repair_hint = Some(format!(
                                "Transportziel automatisch auf {} gesetzt.",
                                receiver
                            ));
                        }
                        Err(error) => {
                            repair_hint = Some(format!(
                                "Transportziel wurde gesetzt, der neue Status konnte aber nicht gelesen werden: {error}"
                            ));
                        }
                    },
                    Err(error) => repair_hint = Some(error),
                }
            }
        }
    }
    let target = value["target"].as_str().map(str::to_string);
    let mut hint = transport_hint(target.as_deref(), receiver.as_deref(), token.is_some());
    if let Some(repair_hint) = repair_hint {
        if !hint.is_empty() {
            hint.push(' ');
        }
        hint.push_str(&repair_hint);
    }
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
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    fn probe_local_status(target: &str, token: Option<&str>) -> (Probe, Vec<(String, Value)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let server_stopped = stopped.clone();
        let mut target = target.to_string();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut requests = Vec::new();
            while !server_stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => panic!("local HTTP fixture: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let request = line.trim().to_string();
                let mut length = 0;
                loop {
                    line.clear();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse::<usize>().unwrap();
                        }
                    }
                }
                assert!(length <= 4096);
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let body = if bytes.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes).unwrap()
                };
                if request.starts_with("PUT /transport ") {
                    target = format!(
                        "{}:{}",
                        body["target_host"].as_str().unwrap(),
                        body["target_port"]
                    );
                }
                requests.push((request, body));
                let response = serde_json::json!({
                    "sensor": "HLK-LD2450", "node_id": "fixture-radar",
                    "target": target, "diagnostics": {},
                })
                .to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            }
            requests
        });
        let result = probe(
            Some(&url),
            Some("fixture-radar"),
            Ipv4Addr::new(192, 0, 2, 50),
            55010,
            token,
            true,
        );
        stopped.store(true, Ordering::SeqCst);
        (result, server.join().unwrap())
    }

    #[test]
    fn explicit_receiver_does_not_depend_on_node_url_or_route() {
        for url in ["http://127.0.0.1:8032", "not a radar URL"] {
            assert_eq!(
                receiver_address(url, Ipv4Addr::new(192, 0, 2, 50), 55010),
                Some("192.0.2.50:55010".to_string())
            );
        }
    }

    #[test]
    fn wildcard_receiver_keeps_route_selected_address() {
        assert_eq!(
            receiver_address("http://127.0.0.1:8032", Ipv4Addr::UNSPECIFIED, 55010),
            Some("127.0.0.1:55010".to_string())
        );
        assert_eq!(
            receiver_address("invalid", Ipv4Addr::UNSPECIFIED, 55010),
            None
        );
    }

    #[test]
    fn matching_explicit_receiver_has_no_mismatch_or_put() {
        let (probe, requests) = probe_local_status("192.0.2.50:55010", Some("fixture-token"));
        assert!(probe.status.reachable);
        assert_eq!(probe.status.receiver.as_deref(), Some("192.0.2.50:55010"));
        assert!(!probe.status.hint.contains("Radar sendet an"));
        assert_eq!(requests.len(), 1);
        assert!(requests[0].0.starts_with("GET /ota/status "));
    }

    #[test]
    fn tokenless_mismatch_never_puts() {
        let (probe, requests) = probe_local_status("192.0.2.9:55010", None);
        assert_eq!(probe.status.target.as_deref(), Some("192.0.2.9:55010"));
        assert_eq!(probe.status.receiver.as_deref(), Some("192.0.2.50:55010"));
        assert!(probe.status.hint.contains("nur erkennen, nicht ändern"));
        assert_eq!(requests.len(), 1);
        assert!(requests[0].0.starts_with("GET /ota/status "));
    }

    #[test]
    fn mismatched_target_repairs_to_explicit_receiver() {
        let (probe, requests) = probe_local_status("192.0.2.9:55010", Some("fixture-token"));
        assert_eq!(probe.status.target.as_deref(), Some("192.0.2.50:55010"));
        assert_eq!(probe.status.receiver.as_deref(), Some("192.0.2.50:55010"));
        assert!(!probe.status.hint.contains("Radar sendet an"));
        assert_eq!(requests.len(), 3);
        assert!(requests[0].0.starts_with("GET /ota/status "));
        assert!(requests[1].0.starts_with("PUT /transport "));
        assert_eq!(
            requests[1].1,
            serde_json::json!({"target_host": "192.0.2.50", "target_port": 55010})
        );
        assert!(requests[2].0.starts_with("GET /ota/status "));
    }

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
    fn discovery_keeps_radar_after_sixteen_lan_neighbors() {
        let mut arp = (1..=20)
            .map(|host| format!("? (192.168.178.{host}) at aa:bb:cc:dd:ee:ff on en5"))
            .collect::<Vec<_>>()
            .join("\n");
        arp.push_str("\n? (192.168.4.5) at aa:bb:cc:dd:ee:ff on en0");
        arp.push_str("\n? (192.168.4.5) at aa:bb:cc:dd:ee:ff on en0");
        arp.push_str("\n? (8.8.8.8) at aa:bb:cc:dd:ee:ff on en5");

        let urls = neighbor_urls(&arp);
        assert_eq!(urls.len(), 21);
        assert_eq!(urls.last().unwrap(), "http://192.168.4.5:8032");
        assert!(neighbor_urls("").is_empty());
    }

    #[test]
    fn diagnoses_ip_and_port_mismatch_without_rx() {
        let hint = transport_hint(Some("192.168.4.50:5010"), Some("192.168.4.3:5010"), false);
        assert!(hint.contains("192.168.4.50:5010"));
        assert!(hint.contains("192.168.4.3:5010"));
        assert!(hint.contains("MMWAVE_NODE_TOKEN"));
        assert!(hint.contains("nur erkennen, nicht ändern"));
        assert!(
            !transport_hint(Some("192.168.4.3:5011"), Some("192.168.4.3:5010"), true).is_empty()
        );
        assert!(
            transport_hint(Some("192.168.4.3:5010"), Some("192.168.4.3:5010"), true).is_empty()
        );
    }
}
