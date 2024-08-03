use std::net::Ipv4Addr;
use std::pin::pin;
use std::str::FromStr;
use std::sync::Arc;

use block::Blocker;
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::ListParams;
use kube::runtime::{watcher, WatchStreamExt};
use kube::{Api, Client};
use protocol::Result;
use rewrites::{RewriteRule, Rewrites};
use tokio::join;
use tracing::info;

use crate::config::Config;
use crate::networking::handler::handle_request;
use crate::networking::udp_serv::UdpServer;
use crate::protocol::byte_packet_buffer::BytePacketBuffer;

mod block;
mod config;
mod dns;
mod networking;
mod protocol;
mod rewrites;

pub type Cache = dashmap::DashMap<String, (std::time::SystemTime, protocol::dns_packet::DnsPacket)>;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration file.
    let config = config::load_config_relative("./mindns.yaml");
    tracing_subscriber::fmt::init();
    info!("Loaded configuration file.");

    // Start DNS server.
    let raw_addr = format!("{}:{}", config.server.bind, config.server.port);
    info!("Starting DNS server at udp://{}", raw_addr);

    let cache = Arc::new(Cache::new());
    let blocker = Blocker::new(config.block.lists.clone());
    blocker.process_lists().await;
    let rewrites = Rewrites::new();
    for rule in config.rewrites.iter() {
        rewrites.add_rewrite(rule).await;
    }

    let k8s = k8s(rewrites.clone());

    let server = UdpServer::new(raw_addr, move |peer, mut reader, config: Config| {
        let cache = cache.clone();
        let blocker = blocker.clone();
        let rewrites = rewrites.clone();
        async move {
            let mut buffer = BytePacketBuffer::new();
            while let Some(Ok(data)) = reader.recv().await {
                buffer.pos = 0;
                buffer.buf[..data.len()].copy_from_slice(&data);

                handle_request(&config, &peer, &mut buffer, &cache, &blocker, &rewrites).await?;
            }

            Ok(())
        }
    })?
    .set_peer_timeout_sec(20);

    let _ = join!(server.start(config), k8s);

    Ok(())
}

async fn k8s(rewrites: Rewrites) {
    async fn ingress_rewrites(ingress: Ingress) -> Vec<RewriteRule> {
        let mut ret = vec![];
        let Some(spec) = ingress.spec else {
            return ret;
        };
        let Some(rules) = spec.rules else {
            return ret;
        };
        let Some(status) = ingress.status else {
            return ret;
        };
        let Some(lb) = status.load_balancer else {
            return ret;
        };
        let Some(ingress_ip) = lb
            .ingress
            .unwrap_or_default()
            .first()
            .and_then(|i| i.ip.clone())
        else {
            return ret;
        };
        for rule in rules {
            if let Some(host) = rule.host {
                ret.push(RewriteRule {
                    host,
                    ip: std::net::IpAddr::V4(Ipv4Addr::from_str(&ingress_ip).unwrap()),
                });
            }
        }
        ret
    }
    use futures::TryStreamExt;
    info!("Connecting to k8s API");
    let client = Client::try_default().await.unwrap();
    let ingress: Api<Ingress> = Api::all(client.clone());

    let existing = ingress.list(&ListParams::default()).await.unwrap();
    for i in existing {
        rewrites.add_k8s_rewrites(ingress_rewrites(i).await).await;
    }

    let obs = watcher(ingress, kube::runtime::watcher::Config::default())
        .default_backoff()
        .applied_objects();
    let mut obs = pin!(obs);

    while obs.try_next().await.unwrap().is_some() {
        // I am too lazy to do this correctly, so just redo the whole thing.
        let ingress: Api<Ingress> = Api::all(client.clone());
        let existing = ingress.list(&ListParams::default()).await.unwrap();
        for i in existing {
            rewrites.add_k8s_rewrites(ingress_rewrites(i).await).await;
        }
    }
}
