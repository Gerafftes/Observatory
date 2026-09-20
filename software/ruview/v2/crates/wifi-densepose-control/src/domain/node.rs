use serde::{Deserialize, Serialize};

/// Node health status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Online,
    Offline,
    Degraded,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Offline
    }
}

/// Chip type for ESP32 variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Chip {
    #[default]
    Esp32,
    Esp32s2,
    Esp32s3,
    Esp32c3,
    Esp32c6,
}

/// Node role in the mesh network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MeshRole {
    Coordinator,
    #[default]
    Node,
    Aggregator,
}

/// Discovery method used to find the node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMethod {
    #[default]
    Mdns,
    UdpProbe,
    HttpSweep,
    Manual,
}

/// Node capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeCapabilities {
    pub wasm: bool,
    pub ota: bool,
    pub csi: bool,
}

/// A discovered ESP32 CSI node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredNode {
    pub ip: String,
    pub mac: Option<String>,
    pub hostname: Option<String>,
    pub node_id: u8,
    pub firmware_version: Option<String>,
    pub health: HealthStatus,
    pub last_seen: String,
    // Extended fields
    pub chip: Chip,
    pub mesh_role: MeshRole,
    pub discovery_method: DiscoveryMethod,
    pub tdm_slot: Option<u8>,
    pub tdm_total: Option<u8>,
    pub edge_tier: Option<u8>,
    pub uptime_secs: Option<u64>,
    pub capabilities: Option<NodeCapabilities>,
    pub friendly_name: Option<String>,
    pub notes: Option<String>,
}

/// Aggregate root keyed by the firmware-provisioned node ID.
///
/// IP and MAC addresses are mutable connection metadata, never identity keys.
#[derive(Debug, Default)]
pub struct NodeRegistry {
    nodes: std::collections::HashMap<u8, DiscoveredNode>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update a node using its stable node ID.
    pub fn upsert(&mut self, node: DiscoveredNode) {
        self.nodes.insert(node.node_id, node);
    }

    /// Get a node by its stable firmware ID.
    pub fn get(&self, node_id: u8) -> Option<&DiscoveredNode> {
        self.nodes.get(&node_id)
    }

    /// List all known nodes.
    pub fn all(&self) -> Vec<&DiscoveredNode> {
        self.nodes.values().collect()
    }

    /// Number of registered nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
