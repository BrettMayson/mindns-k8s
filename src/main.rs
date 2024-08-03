use std::sync::Arc;

use protocol::Result;
use tracing::info;

use crate::config::Config;
use crate::networking::handler::handle_request;
use crate::networking::udp_serv::UdpServer;
use crate::protocol::byte_packet_buffer::BytePacketBuffer;

mod config;
mod dns;
mod networking;
mod protocol;

pub type Cache = dashmap::DashMap<String, (std::time::SystemTime, protocol::dns_packet::DnsPacket)>;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration file.
    let config = config::load_config_relative("./mindns.toml");
    tracing_subscriber::fmt::init();
    info!("Loaded configuration file.");

    // Start DNS server.
    let raw_addr = format!("{}:{}", config.server.bind, config.server.port);
    info!("Starting DNS server at udp://{}", raw_addr);

    let cache = Arc::new(Cache::new());

    UdpServer::new(raw_addr, move |peer, mut reader, config: Config| {
        let cache = cache.clone();
        async move {
            let mut buffer = BytePacketBuffer::new();
            while let Some(Ok(data)) = reader.recv().await {
                buffer.pos = 0;
                buffer.buf[..data.len()].copy_from_slice(&data);

                handle_request(&config, &peer, &mut buffer, &cache).await?;
            }

            Ok(())
        }
    })?
    .set_peer_timeout_sec(20)
    .start(config)
    .await?;

    Ok(())
}
