use serde_derive::Deserialize;

use crate::rewrites::RewriteRule;

use super::{BlockSettings, Config, MirrorSettings, ServerSettings};

#[derive(Clone, Default, Deserialize)]
pub struct ServerSettingsFile {
    port: Option<u16>,
    bind: Option<String>,
}

impl From<ServerSettingsFile> for ServerSettings {
    fn from(val: ServerSettingsFile) -> Self {
        Self {
            port: val.port.unwrap_or(53),
            bind: val.bind.unwrap_or("0.0.0.0".to_string()),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct MirrorSettingsFile {
    enabled: Option<bool>,
    servers: Vec<String>,
}

impl From<MirrorSettingsFile> for MirrorSettings {
    fn from(val: MirrorSettingsFile) -> Self {
        if matches!(val.enabled, Some(true) if val.servers.is_empty()) {
            panic!("Mirror servers must be provided if mirror is enabled");
        }
        Self {
            enabled: val.enabled.unwrap_or(true),
            servers: val.servers,
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct BlockSettingsFile {
    enabled: Option<bool>,
    lists: Vec<String>,
}

impl From<BlockSettingsFile> for BlockSettings {
    fn from(val: BlockSettingsFile) -> Self {
        if matches!(val.enabled, Some(true) if val.lists.is_empty()) {
            panic!("Block lists must be provided if block is enabled");
        }
        Self {
            enabled: val.enabled.unwrap_or(true),
            lists: val.lists,
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct ConfigFile {
    server: Option<ServerSettingsFile>,
    mirror: Option<MirrorSettingsFile>,
    block: Option<BlockSettingsFile>,
    rewrites: Vec<RewriteRule>,
}

impl From<ConfigFile> for Config {
    fn from(val: ConfigFile) -> Self {
        Self {
            server: val.server.unwrap_or_default().into(),
            mirror: val.mirror.unwrap_or_default().into(),
            block: val.block.unwrap_or_default().into(),
            rewrites: val.rewrites,
        }
    }
}
