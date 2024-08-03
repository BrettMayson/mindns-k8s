use std::sync::Arc;

use block::Blocker;
use protocol::Result;
use rewrites::Rewrites;
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
        info!("Adding rewrite rule for {} -> {}", rule.host, rule.ip);
        rewrites
            .add_rewrite(
                &rule.host,
                match rule.ip {
                    std::net::IpAddr::V4(ip) => protocol::dns_record::DnsRecord::A {
                        domain: rule.host.clone(),
                        addr: ip,
                        ttl: 500,
                    },
                    std::net::IpAddr::V6(ip) => protocol::dns_record::DnsRecord::AAAA {
                        domain: rule.host.clone(),
                        addr: ip,
                        ttl: 500,
                    },
                },
            )
            .await;
    }

    UdpServer::new(raw_addr, move |peer, mut reader, config: Config| {
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
    .set_peer_timeout_sec(20)
    .start(config)
    .await?;

    Ok(())
}
