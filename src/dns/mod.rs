use std::net::{Ipv4Addr, UdpSocket};

use tracing::debug;

use crate::{
    protocol::{
        byte_packet_buffer::BytePacketBuffer, dns_packet::DnsPacket, dns_question::DnsQuestion,
        query_type::QueryType, result_code::ResultCode, Result,
    },
    Cache,
};

fn lookup(
    qname: &str,
    qtype: QueryType,
    server: (Ipv4Addr, u16),
    cache: &Cache,
) -> Result<DnsPacket> {
    if let Some(data) = cache.get(qname) {
        let (_, (fetched, packet)) = data.pair();
        let ttl = packet.answers.first().map(|x| x.ttl()).unwrap_or(0);
        if fetched.elapsed().unwrap().as_secs() < ttl as u64 {
            return Ok(packet.clone());
        }
    }

    let socket = UdpSocket::bind(("0.0.0.0", 43210))?;

    let mut packet = DnsPacket::new();

    packet.header.id = 6666;
    packet.header.questions = 1;
    packet.header.recursion_desired = true;
    packet
        .questions
        .push(DnsQuestion::new(qname.to_string(), qtype));

    let mut req_buffer = BytePacketBuffer::new();
    packet.write(&mut req_buffer)?;
    socket.send_to(&req_buffer.buf[0..req_buffer.pos], server)?;

    let mut res_buffer = BytePacketBuffer::new();
    // Add timeout to recv_from.
    socket.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    socket.recv_from(&mut res_buffer.buf)?;

    let packet = DnsPacket::from_buffer(&mut res_buffer);
    if let Ok(packet) = &packet {
        if !packet.answers.is_empty() {
            cache.insert(
                qname.to_string(),
                (std::time::SystemTime::now(), packet.clone()),
            );
        }
    }
    packet
}

pub fn recursive_lookup(
    dns_server: &str,
    qname: &str,
    qtype: QueryType,
    cache: &Cache,
) -> Result<DnsPacket> {
    // For now we're always starting with *a.root-servers.net*.
    let mut ns = dns_server.parse::<Ipv4Addr>().unwrap();

    // Since it might take an arbitrary number of steps, we enter an unbounded loop.
    loop {
        debug!("Attempting {:?} {} with ns {}", qtype, qname, ns);

        // The next step is to send the query to the active server.
        let ns_copy = ns;

        let server = (ns_copy, 53);
        let response = lookup(qname, qtype, server, cache)?;

        // If there are entries in the answer section, and no errors, we are done!
        if !response.answers.is_empty() && response.header.rescode == ResultCode::NOERROR {
            return Ok(response);
        }

        // We might also get a `NXDOMAIN` reply, which is the authoritative name servers
        // way of telling us that the name doesn't exist.
        if response.header.rescode == ResultCode::NXDOMAIN {
            return Ok(response);
        }

        // Otherwise, we'll try to find a new nameserver based on NS and a corresponding A
        // record in the additional section. If this succeeds, we can switch name server
        // and retry the loop.
        if let Some(new_ns) = response.get_resolved_ns(qname) {
            ns = new_ns;

            continue;
        }

        // If not, we'll have to resolve the ip of a NS record. If no NS records exist,
        // we'll go with what the last server told us.
        let new_ns_name = match response.get_unresolved_ns(qname) {
            Some(x) => x,
            None => return Ok(response),
        };

        // Here we go down the rabbit hole by starting _another_ lookup sequence in the
        // midst of our current one. Hopefully, this will give us the IP of an appropriate
        // name server.
        let recursive_response = recursive_lookup(dns_server, new_ns_name, QueryType::A, cache)?;

        // Finally, we pick a random ip from the result, and restart the loop. If no such
        // record is available, we again return the last result we got.
        if let Some(new_ns) = recursive_response.get_random_a() {
            ns = new_ns;
        } else {
            return Ok(response);
        }
    }
}
