use json_patch::Patch;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// Simple ping, will get an Ok / Error response
    Ping,

    /// This fetches the full status for all devices
    GetStatus,

    Daemon(DaemonCommand),
    Pipewire(PipewireCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsocketRequest {
    pub id: u64,
    pub data: DaemonRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    Ok,
    Err(String),
    Patch(Patch),
    Status(DaemonStatus),
    Pipewire(PipewireCommandResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsocketResponse {
    pub id: u64,
    pub data: DaemonResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonCommand {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipewireCommand {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipewireCommandResponse {
    Ok,
    Err(String),
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub config: DaemonConfig,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub http_settings: HttpSettings,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct HttpSettings {
    pub enabled: bool,
    pub bind_address: String,
    pub cors_enabled: bool,
    pub port: u16,
}