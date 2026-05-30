use serde::{Deserialize, Serialize};

pub(crate) fn default_client() -> String {
    if cfg!(target_os = "windows") { ".\\client.exe".into() } else { "./client".into() }
}
pub(crate) fn default_proxy() -> String {
    if cfg!(target_os = "windows") { ".\\proxy.exe".into() } else { "./proxy".into() }
}
pub(crate) fn default_socks_addr() -> String { "127.0.0.1".into() }
pub(crate) fn default_socks_port() -> u16 { 1080 }
pub(crate) fn default_admin_port() -> u16 { 10991 }
pub(crate) fn default_tun_name() -> String {
    if cfg!(target_os = "macos") { "utun99".into() } else { "tun0".into() }
}
pub(crate) fn default_tun_ip() -> String { "198.18.0.1".into() }
pub(crate) fn default_tun_ip6() -> String { "fd00::1".into() }
pub(crate) fn default_tun_prefix() -> u8 { 16 }
pub(crate) fn default_tun_prefix6() -> u8 { 64 }
pub(crate) fn default_tun_gw() -> String { "198.18.0.2".into() }
pub(crate) fn default_tun_gw6() -> String { "fd00::2".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GuiConfig {
    #[serde(default = "default_client")]
    pub client: String,
    #[serde(default = "default_proxy")]
    pub proxy: String,
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_socks_addr")]
    pub socks_addr: String,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
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
    #[serde(default, alias = "phys_ip")]
    pub socks_outbound_ip: Option<String>,
    #[serde(default = "default_admin_port")]
    pub admin_port: u16,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            client: default_client(),
            proxy: default_proxy(),
            server: String::new(),
            token: String::new(),
            socks_addr: default_socks_addr(),
            socks_port: default_socks_port(),
            tun_name: default_tun_name(),
            tun_ip: default_tun_ip(),
            tun_ip6: default_tun_ip6(),
            tun_prefix: default_tun_prefix(),
            tun_prefix6: default_tun_prefix6(),
            tun_gw: default_tun_gw(),
            tun_gw6: default_tun_gw6(),
            socks_outbound_ip: None,
            admin_port: default_admin_port(),
        }
    }
}
