use std::{net::IpAddr, sync::Arc};

use dashmap::DashMap;
use serde_derive::Deserialize;
use tokio::sync::Mutex;

use crate::protocol::dns_record::DnsRecord;

#[derive(Clone, Deserialize)]
pub struct RewriteRule {
    pub host: String,
    pub ip: IpAddr,
}

#[derive(Clone)]
pub struct Rewrites {
    data: Arc<RewritesData>,
}

pub struct RewritesData {
    pub rewrites: DashMap<String, DnsRecord>,
    pub from_k8s: Mutex<Vec<String>>,
}

impl Rewrites {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RewritesData {
                rewrites: DashMap::new(),
                from_k8s: Mutex::new(Vec::new()),
            }),
        }
    }

    pub async fn add_rewrite(&self, rule: &RewriteRule) {
        self.data.rewrites.insert(
            rule.host.to_string(),
            match rule.ip {
                IpAddr::V4(ip) => DnsRecord::A {
                    domain: rule.host.to_string(),
                    addr: ip,
                    ttl: 500,
                },
                IpAddr::V6(ip) => DnsRecord::AAAA {
                    domain: rule.host.to_string(),
                    addr: ip,
                    ttl: 500,
                },
            },
        );
    }

    pub async fn add_k8s_rewrites(&self, rules: Vec<RewriteRule>) {
        // remove all rewrites from k8s
        let existing = self.data.from_k8s.lock().await;
        self.data.rewrites.retain(|k, _| !existing.contains(k));
        let mut new = Vec::new();
        for rule in rules {
            new.push(rule.host.clone());
            self.add_rewrite(&rule).await;
        }
        drop(existing);
        *self.data.from_k8s.lock().await = new;
    }

    pub async fn remove_rewrite(&self, host: &str) {
        self.data.rewrites.remove(host);
    }

    pub async fn get_rewrite(&self, host: &str) -> Option<DnsRecord> {
        self.data.rewrites.get(host).map(|r| r.value().clone())
    }

    pub async fn get_rewrites(&self) -> DashMap<String, DnsRecord> {
        self.data.rewrites.clone()
    }
}
