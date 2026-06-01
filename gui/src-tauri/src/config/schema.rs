use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Bare host:port (no scheme). Example: "example.com:443".
    pub server: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
    #[serde(default = "default_tun_name")]
    pub tun_name: String,
    #[serde(default = "default_tun_ip")]
    pub tun_ip: String,
    #[serde(default = "default_tun_ip6")]
    pub tun_ip6: String,
    #[serde(default = "default_tun_prefix")]
    pub tun_prefix: u8,
    #[serde(default = "default_tun_prefix6")]
    pub tun_prefix6: u8,
    #[serde(default = "default_tun_gw")]
    pub tun_gw: String,
    #[serde(default = "default_tun_gw6")]
    pub tun_gw6: String,
    #[serde(default)]
    pub phys_ip: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_socks_port() -> u16 {
    1080
}
fn default_admin_port() -> u16 {
    10991
}
fn default_tun_name() -> String {
    "tun0".into()
}
fn default_tun_ip() -> String {
    "198.18.0.1".into()
}
fn default_tun_ip6() -> String {
    "fd00::1".into()
}
fn default_tun_prefix() -> u8 {
    16
}
fn default_tun_prefix6() -> u8 {
    64
}
fn default_tun_gw() -> String {
    "198.18.0.2".into()
}
fn default_tun_gw6() -> String {
    "fd00::2".into()
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            name: "default".into(),
            enabled: true,
            server: String::new(),
            token: String::new(),
            socks_port: default_socks_port(),
            admin_port: default_admin_port(),
            tun_name: default_tun_name(),
            tun_ip: default_tun_ip(),
            tun_ip6: default_tun_ip6(),
            tun_prefix: default_tun_prefix(),
            tun_prefix6: default_tun_prefix6(),
            tun_gw: default_tun_gw(),
            tun_gw6: default_tun_gw6(),
            phys_ip: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuiConfig {
    #[serde(default)]
    pub nodes: Vec<NodeConfig>,
    #[serde(default)]
    pub active_node: usize,
}

impl GuiConfig {
    pub fn default_with_node() -> Self {
        Self {
            nodes: vec![NodeConfig::default()],
            active_node: 0,
        }
    }

    pub fn active_node(&self) -> Option<&NodeConfig> {
        self.nodes.get(self.active_node)
    }
}
