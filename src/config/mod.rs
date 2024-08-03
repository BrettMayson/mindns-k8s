use std::path::PathBuf;

use files::ConfigFile;

use crate::rewrites::RewriteRule;

mod files;

#[derive(Clone)]
pub struct ServerSettings {
    pub port: u16,
    pub bind: String,
}

#[derive(Clone)]
pub struct MirrorSettings {
    pub enabled: bool,
    pub servers: Vec<String>,
}

#[derive(Clone)]
pub struct BlockSettings {
    pub enabled: bool,
    pub lists: Vec<String>,
}

#[derive(Clone)]
pub struct Config {
    pub server: ServerSettings,
    pub mirror: MirrorSettings,
    pub block: BlockSettings,
    pub rewrites: Vec<RewriteRule>,
}

pub fn load_config(path: PathBuf) -> Config {
    let config = std::fs::read_to_string(path).unwrap();
    let configfile: ConfigFile = serde_yaml::from_str(&config).unwrap();
    configfile.into()
}

pub fn load_config_relative(path: &str) -> Config {
    let current_dir = std::env::current_dir().unwrap();
    let path = current_dir.join(path);
    load_config(path)
}
