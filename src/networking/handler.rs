use std::sync::Arc;

use tracing::info;

use crate::{
    block::Blocker,
    config::Config,
    dns::recursive_lookup,
    protocol::{
        byte_packet_buffer::BytePacketBuffer, dns_packet::DnsPacket, dns_question::DnsQuestion,
        result_code::ResultCode, Result,
    },
    rewrites::Rewrites,
    Cache,
};

use super::peer::UdpPeer;

pub async fn handle_query(
    config: &Config,
    question: &DnsQuestion,
    out: &mut DnsPacket,
    cache: &Cache,
    blocker: &Blocker,
    rewrites: &Rewrites,
) {
    if !config.rewrites.is_empty() {
        if let Some(rewrite) = rewrites.get_rewrite(&question.name).await {
            info!("Rewriting query for {}", question.name);
            out.header.rescode = ResultCode::NOERROR;
            out.answers.push(rewrite);
            return;
        }
    }

    if config.block.enabled && blocker.is_blocked(&question.name).await {
        info!("Blocked query for {}", question.name);
        out.header.rescode = ResultCode::NXDOMAIN;
        return;
    }

    let mirror_ns = config.mirror.servers.first().unwrap().as_str();

    if config.mirror.enabled {
        if question.name.ends_with(".home.arpa") {
            out.header.rescode = ResultCode::NXDOMAIN;
            return
        }

        let result = recursive_lookup(mirror_ns, &question.name, question.qtype, cache);

        if let Ok(result) = result {
            out.header.rescode = result.header.rescode;

            if result.header.rescode == ResultCode::NOERROR {
                for rec in result.answers {
                    out.answers.push(rec);
                }

                for rec in result.authorities {
                    out.authorities.push(rec);
                }

                for rec in result.resources {
                    out.resources.push(rec);
                }
            }
        } else {
            out.header.rescode = ResultCode::SERVFAIL;
        }
    }
}

pub async fn handle_request(
    config: &Config,
    peer: &Arc<UdpPeer>,
    buffer: &mut BytePacketBuffer,
    cache: &Cache,
    blocker: &Blocker,
    rewrites: &Rewrites,
) -> Result<()> {
    let mut request = DnsPacket::from_buffer(buffer)?;

    let mut packet = DnsPacket::new();
    packet.header.id = request.header.id;
    packet.header.recursion_desired = true;
    packet.header.recursion_available = true;
    packet.header.response = true;

    if let Some(question) = request.questions.pop() {
        info!(
            "Client {} requested {:?} {}",
            peer.addr, question.qtype, question.name,
        );

        packet.questions.push(question.clone());
        handle_query(config, &question, &mut packet, cache, blocker, rewrites).await;
    } else {
        packet.header.rescode = ResultCode::FORMERR;
    }

    let mut res_buffer = BytePacketBuffer::new();
    packet.write(&mut res_buffer)?;

    let len = res_buffer.pos();
    let data = res_buffer.get_range(0, len)?;

    peer.send(data).await?;
    Ok(())
}
