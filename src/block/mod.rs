use std::{collections::HashSet, sync::Arc};

use tokio::sync::RwLock;
use tracing::info;

pub struct BlockRegex {
    source: String,
    regex: regex::Regex,
}

impl BlockRegex {
    pub fn new(source: &str) -> Result<BlockRegex, regex::Error> {
        let regex = regex::Regex::new(source)?;
        Ok(BlockRegex {
            source: source.to_string(),
            regex,
        })
    }
}

impl PartialEq for BlockRegex {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for BlockRegex {}

impl std::hash::Hash for BlockRegex {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.source.hash(state);
    }
}

#[derive(Clone)]
pub struct Blocker {
    data: Arc<BlockerData>,
}

impl Blocker {
    pub fn new(lists: Vec<String>) -> Self {
        Self {
            data: Arc::new(BlockerData::new(lists)),
        }
    }

    pub async fn block(&self, host: &str, subdomains: bool) {
        self.data.blocks.write().await.push(BlockedDomain {
            host: host.to_string(),
            subdomains,
        });
    }

    pub async fn unblock(&self, host: &str) {
        self.data.allows.write().await.push(host.to_string());
    }

    pub async fn is_blocked(&self, host: &str) -> bool {
        let blocked = if self.data.blocks.read().await.iter().any(|b| {
            if b.subdomains {
                host.ends_with(&b.host)
            } else {
                host == b.host
            }
        }) {
            true
        } else {
            self.data
                .regex
                .read()
                .await
                .iter()
                .any(|r| r.regex.is_match(host))
        };
        if !blocked {
            return false;
        }
        !self.data
            .allows
            .read()
            .await
            .iter()
            .all(|a| !host.ends_with(a))
    }

    async fn parse_hosts(&self, content: &str) -> u64 {
        let mut blocked = 0;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(host) = line.strip_prefix("||") {
                let host = host.split_once("^").map(|(host, _)| host).unwrap_or(host);
                self.block(host, true).await;
                blocked += 1;
            } else if let Some(host) = line.strip_prefix("@@") {
                if let Some(host) = host.strip_prefix("||") {
                    let host = host.split_once("^").map(|(host, _)| host).unwrap_or(host);
                    self.unblock(host).await;
                } else {
                    let host = host.split_once("^").map(|(host, _)| host).unwrap_or(host);
                    self.unblock(host).await;
                }
            } else if line.starts_with("/") {
                let regex = &line[1..line.len() - 1];
                let compiled = BlockRegex::new(regex);
                if let Ok(compiled) = compiled {
                    self.data.regex.write().await.insert(compiled);
                }
            } else if let Some(host) = line.strip_prefix("127.0.0.1") {
                self.block(host.trim(), false).await;
                blocked += 1;
            } else if line.starts_with("!") || line.starts_with("#") {
                // ignore comments
            } else if !line.contains(" ") {
                let host = line.split_once("^").map(|(host, _)| host).unwrap_or(line);
                self.block(host, false).await;
                blocked += 1;
            } else {
                eprintln!("Unknown line: {}", line);
            }
        }
        blocked
    }

    pub async fn process_lists(&self) {
        for list in &self.data.lists {
            if list.starts_with("http") {
                let response = reqwest::get(list).await;
                if let Ok(response) = response {
                    let content = response.text().await;
                    if let Ok(content) = content {
                        let blocked = self.parse_hosts(&content).await;
                        info!("Blocked {} hosts from {}", blocked, list);
                    }
                }
            } else {
                let content = tokio::fs::read_to_string(list).await;
                if let Ok(content) = content {
                    let blocked = self.parse_hosts(&content).await;
                    info!("Blocked {} hosts from {}", blocked, list);
                }
            }
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
pub struct BlockedDomain {
    host: String,
    subdomains: bool,
}

pub struct BlockerData {
    lists: Vec<String>,
    blocks: RwLock<Vec<BlockedDomain>>,
    allows: RwLock<Vec<String>>,
    regex: RwLock<HashSet<BlockRegex>>,
}

impl BlockerData {
    pub fn new(lists: Vec<String>) -> Self {
        Self {
            lists,
            blocks: RwLock::new(Vec::new()),
            allows: RwLock::new(Vec::new()),
            regex: RwLock::new(HashSet::new()),
        }
    }
}
