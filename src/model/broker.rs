use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrokerProfile {
    pub id: String,
    pub name: String,
    pub protocol: BrokerProtocol,
    pub host: String,
    pub port: u16,
    pub websocket_path: Option<String>,
    pub tls: Option<TlsConfig>,
    pub auth: Option<AuthConfig>,
    pub keepalive_secs: u16,
    pub clean_session: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for BrokerProfile {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            name: String::new(),
            protocol: BrokerProtocol::Mqtt,
            host: "127.0.0.1".to_string(),
            port: 1883,
            websocket_path: None,
            tls: None,
            auth: None,
            keepalive_secs: 30,
            clean_session: true,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrokerProtocol {
    Mqtt,
    Mqtts,
    Ws,
    Wss,
}

impl Default for BrokerProtocol {
    fn default() -> Self {
        Self::Mqtt
    }
}

impl BrokerProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mqtt => "mqtt",
            Self::Mqtts => "mqtts",
            Self::Ws => "ws",
            Self::Wss => "wss",
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match value {
            "mqtts" => Self::Mqtts,
            "ws" => Self::Ws,
            "wss" => Self::Wss,
            _ => Self::Mqtt,
        }
    }

    pub fn is_websocket(self) -> bool {
        matches!(self, Self::Ws | Self::Wss)
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Mqtt => 1883,
            Self::Mqtts => 8883,
            Self::Ws => 8083,
            Self::Wss => 8084,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthConfig {
    UserPassword { username: String, password: String },
    ClientCert { cert_pem: String, key_pem: String },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TlsConfig {
    pub enabled: bool,
    pub ca_pem: Option<String>,
    pub server_name: Option<String>,
    pub insecure_skip_verify: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ca_pem: None,
            server_name: None,
            insecure_skip_verify: false,
        }
    }
}
