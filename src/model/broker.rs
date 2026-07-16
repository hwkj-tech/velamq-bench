use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrokerProfile {
    pub id: String,
    pub name: String,
    pub protocol: BrokerProtocol,
    pub mqtt_version: MqttVersion,
    pub host: String,
    pub port: u16,
    pub websocket_path: Option<String>,
    pub tls: Option<TlsConfig>,
    pub auth: Option<AuthConfig>,
    pub keepalive_secs: u16,
    pub connection_timeout_secs: u16,
    pub clean_session: bool,
    pub mqtt5: Option<Mqtt5Config>,
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
            mqtt_version: MqttVersion::V3_1_1,
            host: "127.0.0.1".to_string(),
            port: 1883,
            websocket_path: None,
            tls: None,
            auth: None,
            keepalive_secs: 30,
            connection_timeout_secs: 10,
            clean_session: true,
            mqtt5: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl BrokerProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.host.trim().is_empty() || self.host.len() > 253 {
            return Err("broker host is required and must be <= 253 characters".to_string());
        }
        if self.port == 0 {
            return Err("broker port must be between 1 and 65535".to_string());
        }
        if self.keepalive_secs == 0 || self.keepalive_secs > 3600 {
            return Err("keepalive_secs must be between 1 and 3600".to_string());
        }
        if self.connection_timeout_secs == 0 || self.connection_timeout_secs > 300 {
            return Err("connection_timeout_secs must be between 1 and 300".to_string());
        }
        if let Some(AuthConfig::UserPassword { username, password }) = &self.auth {
            if username.len() > u16::MAX as usize || password.len() > u16::MAX as usize {
                return Err("MQTT username and password must be <= 65535 bytes".to_string());
            }
        }
        if let Some(tls) = &self.tls {
            let has_cert = tls
                .client_cert_pem
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
            let has_key = tls
                .client_key_pem
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
            if has_cert != has_key {
                return Err(
                    "both client certificate and private key are required for mTLS".to_string(),
                );
            }
            if tls
                .alpn_protocols
                .iter()
                .any(|value| value.is_empty() || value.len() > 255)
            {
                return Err("each ALPN protocol must contain 1 to 255 bytes".to_string());
            }
        }
        if self.mqtt_version == MqttVersion::V5_0 {
            if self.mqtt5.as_ref().and_then(|value| value.receive_maximum) == Some(0) {
                return Err("MQTT 5 receive maximum must be greater than zero".to_string());
            }
            if self
                .mqtt5
                .as_ref()
                .and_then(|value| value.maximum_packet_size)
                == Some(0)
            {
                return Err("MQTT 5 maximum packet size must be greater than zero".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MqttVersion {
    V3_1_1,
    V5_0,
}

impl Default for MqttVersion {
    fn default() -> Self {
        Self::V3_1_1
    }
}

impl MqttVersion {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V3_1_1 => "v3_1_1",
            Self::V5_0 => "v5_0",
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match value {
            "v5_0" => Self::V5_0,
            _ => Self::V3_1_1,
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
    pub client_cert_pem: Option<String>,
    pub client_key_pem: Option<String>,
    pub server_name: Option<String>,
    pub insecure_skip_verify: bool,
    pub alpn_protocols: Vec<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ca_pem: None,
            client_cert_pem: None,
            client_key_pem: None,
            server_name: None,
            insecure_skip_verify: false,
            alpn_protocols: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Mqtt5Config {
    pub session_expiry_interval_secs: Option<u32>,
    pub receive_maximum: Option<u16>,
    pub maximum_packet_size: Option<u32>,
    pub topic_alias_maximum: Option<u16>,
    pub request_problem_information: bool,
}

impl Default for Mqtt5Config {
    fn default() -> Self {
        Self {
            session_expiry_interval_secs: None,
            receive_maximum: None,
            maximum_packet_size: None,
            topic_alias_maximum: None,
            request_problem_information: true,
        }
    }
}
