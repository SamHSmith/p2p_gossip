use std::net::SocketAddr;
use std::time::{Duration, Instant};

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};

use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

#[cfg(test)]
mod tests;

const INITIAL_CONNECTION_MAGIC: &str =
    "Hello there I am a peer from the fantastic p2p_gossip program version v0.1";
const READ_AND_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const ASK_FOR_PEERS_TIME: Duration = Duration::from_millis(1000);

const PEER_CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(2);
const ALREADY_HEARD_GOSSIP_DECAY_TIME: Duration = Duration::from_secs(50);

const PEER_DATA_PACKET_ADDRESS_COUNT_MAX: u16 = 5;

#[derive(Debug)]
struct Peer {
    stream: TcpStream,
    addr: SocketAddr,
    last_ask_for_peer_list_instant: Instant,

    confirmed : bool,
    connect_instant : Instant,
}

impl Peer {
    fn new(stream: TcpStream, addr: SocketAddr) -> Self {
        Peer {
            stream,
            addr,
            last_ask_for_peer_list_instant: Instant::now(),
            confirmed : false,
            connect_instant : Instant::now(),
        }
        // we pass now as the last_ask_for_peer_list_instant because we don't want to spam the network with requests every time
        // we get a new peer. When we have stayed in communication with a peer for ASK_FOR_PEERS_TIME we will ask for peer information.
    }
}

fn connect_to_peer(con_addr: &SocketAddr, listener_addr: &SocketAddr) -> Option<Peer> {
    let stream_res = TcpStream::connect_timeout(con_addr, Duration::from_secs(10));
    if stream_res.is_err()
    { return None; }
    let mut stream = stream_res.unwrap();
    let peer_addr = stream
        .peer_addr()
        .expect("failed to get peer connected address");
    stream
        .set_read_timeout(Some(READ_AND_WRITE_TIMEOUT))
        .expect("failed to set read timeout when connecting");
    stream
        .set_write_timeout(Some(READ_AND_WRITE_TIMEOUT))
        .expect("failed to set write timeout when connecting");
    stream
        .set_nodelay(true)
        .expect("failed to set no delay when connecting");

    if stream
        .write_all(&INITIAL_CONNECTION_MAGIC.as_bytes())
        .is_err()
    {
        return None;
    }

    match listener_addr.ip() {
        IpAddr::V6(addr6) => {
            // write 1 to indicate a ipv6 address
            if stream.write_u8(1).is_err() {
                return None;
            }

            for segment in addr6.segments() {
                if stream.write_u16::<BigEndian>(segment).is_err() {
                    return None;
                }
            }

            if stream.write_u16::<BigEndian>(listener_addr.port()).is_err() {
                return None;
            }
        }
        IpAddr::V4(addr4) => {
            // write 0 to indicate a ipv4 address
            if stream.write_u8(0).is_err() {
                return None;
            }

            for octet in addr4.octets() {
                if stream.write_u8(octet).is_err() {
                    return None;
                }
            }

            if stream.write_u16::<BigEndian>(listener_addr.port()).is_err() {
                return None;
            }
        }
    }

    Some(Peer::new(stream, peer_addr))
}

fn accept_connection(mut stream: TcpStream) -> Option<Peer> {
    stream
        .set_read_timeout(Some(READ_AND_WRITE_TIMEOUT))
        .expect("failed to set read timeout when accepting");
    stream
        .set_write_timeout(Some(READ_AND_WRITE_TIMEOUT))
        .expect("failed to set write timeout when accepting");
    stream
        .set_nodelay(true)
        .expect("failed to set no delay when accepting");

    let mut read_buf = [0; INITIAL_CONNECTION_MAGIC.len()];
    if stream.read_exact(&mut read_buf).is_err() {
        return None;
    }

    if read_buf != INITIAL_CONNECTION_MAGIC.as_bytes() {
        return None;
    }

    if stream.write_u8(4).is_err() ||
        stream
        .write_all(&INITIAL_CONNECTION_MAGIC.as_bytes())
        .is_err()
    {
        return None;
    }

    let is_ipv6_res = stream.read_u8();
    if is_ipv6_res.is_err() {
        return None;
    }
    let is_ipv6 = is_ipv6_res.unwrap();

    if is_ipv6 == 1
    // ipv6
    {
        let mut address_buf: [u16; 8] = [0; 8];
        for i in 0..8 {
            let res = stream.read_u16::<BigEndian>();
            if res.is_err() {
                return None;
            }
            address_buf[i] = res.unwrap();
        }
        let addr = IpAddr::V6(Ipv6Addr::new(
            address_buf[0],
            address_buf[1],
            address_buf[2],
            address_buf[3],
            address_buf[4],
            address_buf[5],
            address_buf[6],
            address_buf[7],
        ));

        let port_res = stream.read_u16::<BigEndian>();
        if port_res.is_err() {
            return None;
        }
        let port = port_res.unwrap();

        let remote_addr = SocketAddr::new(addr, port);
        return Some(Peer::new(stream, remote_addr));
    } else if is_ipv6 == 0
    // ipv4
    {
        let mut address_buf: [u8; 4] = [0; 4];
        for i in 0..4 {
            let res = stream.read_u8();
            if res.is_err() {
                return None;
            }
            address_buf[i] = res.unwrap();
        }
        let addr = IpAddr::V4(Ipv4Addr::new(
            address_buf[0],
            address_buf[1],
            address_buf[2],
            address_buf[3],
        ));

        let port_res = stream.read_u16::<BigEndian>();
        if port_res.is_err() {
            return None;
        }
        let port = port_res.unwrap();

        let remote_addr = SocketAddr::new(addr, port);
        return Some(Peer::new(stream, remote_addr));
    } else {
        return None;
    } // protocol failure
}

const GOSSIP_LEN: usize = 10;

fn send_gossip(peer: &mut Peer, gossip: &[u8; GOSSIP_LEN]) -> bool {
    if peer.stream.write_u8(1).is_err() {
        return false;
    }
    if peer.stream.write_all(gossip).is_err() {
        return false;
    }
    return true;
}

fn do_peer(
    use_ipv6: bool,
    gossip_period: Duration,
    port: u16,
    initial_connect_to_peer: Option<&SocketAddr>,
    self_destruct_time: Option<Duration>,
    gossip_awareness: &mut Vec<[u8; GOSSIP_LEN]>,
    should_use_gossip_awareness: bool,
) -> Option<()> {
    let address_port_combo: (IpAddr, u16) = if use_ipv6 {
        ("::1".parse().unwrap(), port)
    } else {
        ("127.0.0.1".parse().unwrap(), port)
    };
    let listener_res = TcpListener::bind(address_port_combo);
    if listener_res.is_err()
    {
        println!("Error: Failed to open tcp listen socket on port {}.", port);
        return Some(());
    }
    let listener = listener_res.unwrap();
    listener
        .set_nonblocking(true)
        .expect("Failed to set listener to nonblocking");
    let listener_addr = listener
        .local_addr()
        .expect("failed to get listener local address");
    println!("I'm doing peer({})!", listener_addr);

    let start_instant = Instant::now();
    let mut remote_peers = Vec::<Peer>::new();

    if initial_connect_to_peer.is_some() {
        let con_addr = initial_connect_to_peer.unwrap();
        let new_peer =
            connect_to_peer(con_addr, &listener_addr)?;
        println!(
            "I({}) have connected to my initial peer, {}",
            listener_addr, new_peer.addr
        );
        remote_peers.push(new_peer);
    }

    let mut already_heard_gossips = HashMap::<[u8; GOSSIP_LEN], Instant>::new();
    let mut last_self_gossip_instant = Instant::now();
    loop {
        if self_destruct_time.is_some() && start_instant.elapsed() > self_destruct_time.unwrap() {
            return Some(());
        }

        let incomming = listener.accept();
        if incomming.is_ok()
        // handle connection
        {
            let (stream, remote_addr) = incomming.unwrap();
            stream
                .set_nonblocking(false)
                .expect("failed to set connecting stream to blocking");

            let maybe_peer = accept_connection(stream);
            if maybe_peer.is_some() {
                let mut peer = maybe_peer.unwrap();
                println!(
                    "{}: New peer({}) has connected to me",
                    listener_addr, peer.addr
                );
                peer.confirmed = true;
                remote_peers.push(peer);
            } else {
                println!(
                    "{}: Rejected incomming connection from {}",
                    listener_addr, remote_addr
                );
            }
        } else {
            let error = incomming.err().unwrap();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                std::thread::sleep(Duration::from_millis(10)); // do some waiting
            } else {
                eprintln!("There was a accept error, exiting... : {}", error);
            }
        }

        // decay old gossip to save memory
        let mut remove_gossips = Vec::new();
        for (gossip, receive_moment) in already_heard_gossips.iter()
        {
            if receive_moment.elapsed() > ALREADY_HEARD_GOSSIP_DECAY_TIME
            {
                remove_gossips.push(*gossip);
            }
        }
        for gossip in remove_gossips
        {
            already_heard_gossips.remove_entry(&gossip);
        }

        let mut to_broadcast_gossip = Vec::<[u8; GOSSIP_LEN]>::new();

        let mut known_addresses = Vec::<SocketAddr>::new();
        for peer in &remote_peers {
            known_addresses.push(peer.addr);
        }
        let mut new_addresses = Vec::<SocketAddr>::new();

        let mut keep_peers = Vec::<Peer>::new();
        for mut peer in remote_peers {

            if peer.connect_instant.elapsed() > PEER_CONFIRMATION_TIMEOUT && !peer.confirmed
            { continue; } // peer failed to confirm in time, dropping

            let mut read_buf: [u8; 1] = [0; 1];
            peer.stream
                .set_nonblocking(true)
                .expect("Failed to juggle stream into nonblocking");
            let read_res = peer.stream.read(&mut read_buf);
            peer.stream
                .set_nonblocking(false)
                .expect("Failed to juggle out of nonblocking");
            let read_count;
            if read_res.is_err() {
                read_count = 0;
            }
            // the read call will error if there is no data, so we really have no idea what this means.
            else {
                read_count = read_res.unwrap();
            }

            if read_count == 0 {
                keep_peers.push(peer);
                continue;
            } // no activity, keep the peer

            let request_type = read_buf[0];
            if request_type != 4 && !peer.confirmed
            { eprintln!("confirmation violation {:?}", peer); continue; }
            match request_type {
                1 =>
                // gossip
                {
                    let mut gossip_buf: [u8; GOSSIP_LEN] = [0; GOSSIP_LEN];
                    if peer.stream.read_exact(&mut gossip_buf).is_err() {
                        continue;
                    } // read error, drop the peer

                    if !already_heard_gossips.contains_key(&gossip_buf)
                    // new gossip
                    {
                        let mut s = String::with_capacity(2 * GOSSIP_LEN);
                        for byte in gossip_buf.iter() {
                            write!(s, "{:02X}", byte).unwrap();
                        }
                        println!(
                            "{}: Received fresh gossip, 0x{}, from {}",
                            listener_addr, s, peer.addr
                        );
                        to_broadcast_gossip.push(gossip_buf);
                        already_heard_gossips.insert(gossip_buf, Instant::now());
                        // we tag the instant so that we can purge very old gossips later

                        // awareness
                        if should_use_gossip_awareness
                        { gossip_awareness.push(gossip_buf); }
                    }
                }
                2 =>
                // peer request
                {
                    if peer.stream.write_u8(3).is_err() {
                        continue;
                    }
                    let mut send_address_count: u16 = PEER_DATA_PACKET_ADDRESS_COUNT_MAX;
                    if (known_addresses.len() as u16) < send_address_count {
                        send_address_count = known_addresses.len() as u16;
                    }
                    if peer
                        .stream
                        .write_u16::<BigEndian>(send_address_count)
                        .is_err()
                    {
                        continue;
                    }

                    for i in 0..send_address_count {
                        match known_addresses[i as usize].ip() {
                            IpAddr::V6(addr6) => {
                                // write 1 to indicate a ipv6 address
                                if peer.stream.write_u8(1).is_err() {
                                    continue;
                                }

                                for segment in addr6.segments() {
                                    if peer.stream.write_u16::<BigEndian>(segment).is_err() {
                                        continue;
                                    }
                                }

                                if peer
                                    .stream
                                    .write_u16::<BigEndian>(known_addresses[i as usize].port())
                                    .is_err()
                                {
                                    continue;
                                }
                            }
                            IpAddr::V4(addr4) => {
                                // write 0 to indicate a ipv4 address
                                if peer.stream.write_u8(0).is_err() {
                                    continue;
                                }

                                for octet in addr4.octets() {
                                    if peer.stream.write_u8(octet).is_err() {
                                        continue;
                                    }
                                }

                                if peer
                                    .stream
                                    .write_u16::<BigEndian>(known_addresses[i as usize].port())
                                    .is_err()
                                {
                                    continue;
                                }
                            }
                        }
                    }
                }
                3 =>
                // peer data packet
                {
                    let receive_address_count_res = peer.stream.read_u16::<BigEndian>();
                    if receive_address_count_res.is_err() {
                        continue;
                    }
                    let receive_address_count = receive_address_count_res.unwrap();
                    if receive_address_count > PEER_DATA_PACKET_ADDRESS_COUNT_MAX {
                        continue;
                    }

                    for _ in 0..receive_address_count {
                        let is_ipv6_res = peer.stream.read_u8();
                        if is_ipv6_res.is_err() {
                            continue;
                        }
                        let is_ipv6 = is_ipv6_res.unwrap();

                        if is_ipv6 == 1
                        // ipv6
                        {
                            let mut address_buf: [u16; 8] = [0; 8];
                            for i in 0..8 {
                                let res = peer.stream.read_u16::<BigEndian>();
                                if res.is_err() {
                                    continue;
                                }
                                address_buf[i] = res.unwrap();
                            }
                            let addr = IpAddr::V6(Ipv6Addr::new(
                                address_buf[0],
                                address_buf[1],
                                address_buf[2],
                                address_buf[3],
                                address_buf[4],
                                address_buf[5],
                                address_buf[6],
                                address_buf[7],
                            ));

                            let port_res = peer.stream.read_u16::<BigEndian>();
                            if port_res.is_err() {
                                continue;
                            }
                            let port = port_res.unwrap();

                            let remote_addr = SocketAddr::new(addr, port);
                            if !known_addresses.contains(&remote_addr) && remote_addr != listener_addr {
                                known_addresses.push(remote_addr);
                                new_addresses.push(remote_addr);
                            }
                        } else if is_ipv6 == 0
                        // ipv4
                        {
                            let mut address_buf: [u8; 4] = [0; 4];
                            for i in 0..4 {
                                let res = peer.stream.read_u8();
                                if res.is_err() {
                                    continue;
                                }
                                address_buf[i] = res.unwrap();
                            }
                            let addr = IpAddr::V4(Ipv4Addr::new(
                                address_buf[0],
                                address_buf[1],
                                address_buf[2],
                                address_buf[3],
                            ));

                            let port_res = peer.stream.read_u16::<BigEndian>();
                            if port_res.is_err() {
                                continue;
                            }
                            let port = port_res.unwrap();

                            let remote_addr = SocketAddr::new(addr, port);
                            if !known_addresses.contains(&remote_addr) && remote_addr != listener_addr {
                                known_addresses.push(remote_addr);
                                new_addresses.push(remote_addr);
                            }
                        } else {
                            continue; // protocol failure
                        }
                    }
                }
                4 => // peer confirmation
                {
                    let mut read_buf = [0; INITIAL_CONNECTION_MAGIC.len()];
                    if peer.stream.read_exact(&mut read_buf).is_err() {
                        continue;
                    }

                    if read_buf != INITIAL_CONNECTION_MAGIC.as_bytes() {
                        continue;
                    }
                    peer.confirmed = true;
                }
                _ => {
                    eprintln!("violation");
                    continue;
                } // protocol violation
            }
            // done, now we can keep the peer
            keep_peers.push(peer);
        }
        remote_peers = keep_peers;

        for addr in new_addresses
        {
            let maybe_peer = connect_to_peer(&addr, &listener_addr);
            if maybe_peer.is_some()
            {
                remote_peers.push(maybe_peer.unwrap());
            }
        }

        // if we should gossip, send some random gossip
        if last_self_gossip_instant.elapsed() > gossip_period {
            let mut gossip_buf: [u8; GOSSIP_LEN] = [0; GOSSIP_LEN];
            for i in 0..GOSSIP_LEN {
                gossip_buf[i] = rand::random();
            }
            to_broadcast_gossip.push(gossip_buf);
            already_heard_gossips.insert(gossip_buf, Instant::now()); // we already know about our own gossip
            last_self_gossip_instant = Instant::now();

            let mut s = String::with_capacity(2 * GOSSIP_LEN);
            for byte in gossip_buf.iter() {
                write!(s, "{:02X}", byte).unwrap();
            }
            println!(
                "{}: Sending random fresh gossip to all peers, 0x{}",
                listener_addr, s
            );

            // awareness
            if should_use_gossip_awareness
            { gossip_awareness.push(gossip_buf); }
        }

        let mut keep_peers = Vec::<Peer>::new(); // we do the same thing again but now with gossip sending business
        'peer_loop: for mut peer in remote_peers {
            // send gossips
            for gossip in &to_broadcast_gossip {
                if !send_gossip(&mut peer, &gossip)
                // if we fail, drop the peer
                {
                    continue 'peer_loop;
                }
            }
            // done, now we can keep the peer
            keep_peers.push(peer);
        }
        remote_peers = keep_peers;
        to_broadcast_gossip.clear();

        let mut keep_peers = Vec::<Peer>::new(); // ask for peer data
        for mut peer in remote_peers {
            if peer.last_ask_for_peer_list_instant.elapsed() > ASK_FOR_PEERS_TIME {
                if peer.stream.write_u8(2).is_err() {
                    continue;
                } // on error drop peer
                peer.last_ask_for_peer_list_instant = Instant::now();
            }
            // done, now we can keep the peer
            keep_peers.push(peer);
        }
        remote_peers = keep_peers;
    }
}

use std::str::FromStr;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args.len() > 5
    {
        println!("Wrong amount of arguments.");
        println!("Correct usage: p2p_gossip %options%");
        println!("Options:");
        println!("--period=%seconds between random gossip sending% (Required)");
        println!("--port=%the tcp port to start the peer on%       (Required)");
        println!("--connect=%IP and port of peer to connect to%    (Optional)");
        println!("    Ex. --connect=\"127.0.0.1:12542\"  or  --connect=\"[::1]:12433\"");
        println!("--use-ipv6   Tells the peer to start on ipv6. Not needed if you provide an ipv6 connect address");
        return;
    }

    let mut period_maybe : Option<u64> = None;
    let mut port_maybe : Option<u16> = None;
    let mut connect_addr_maybe : Option<SocketAddr> = None;
    let mut use_ipv6 = false;

    let mut first_arg = true;
    for arg in args
    {
        if first_arg { first_arg = false; continue; }

        if arg.starts_with("--period=")
        {
            if period_maybe.is_some()
            {
                println!("Error, already assigned --period");
                return;
            }
            let parse_string = arg.strip_prefix("--period=").unwrap_or("");
            let period_res = u64::from_str(parse_string);
            if period_res.is_err()
            {
                println!("Error while parsing --period={}. Remember that period should be a positive integer", parse_string);
                return;
            }
            period_maybe = Some(period_res.unwrap());
        }
        else if arg.starts_with("--port=")
        {
            if port_maybe.is_some()
            {
                println!("Error, already assigned --port");
                return;
            }
            let parse_string = arg.strip_prefix("--port=").unwrap_or("");
            let port_res = u16::from_str(parse_string);
            if port_res.is_err()
            {
                println!("Error while parsing --port={}. Remember that period should be a positive integer", parse_string);
                return;
            }
            port_maybe = Some(port_res.unwrap());
        }
        else if arg.starts_with("--connect=")
        {
            if connect_addr_maybe.is_some()
            {
                println!("Error, already assigned --connect");
                return;
            }
            let parse_string = arg.strip_prefix("--connect=").unwrap_or("");
            let connect_addr_res = SocketAddr::from_str(parse_string);
            if connect_addr_res.is_err()
            {
                println!("Error while parsing --connect={}. Remember that period should be a valid IPV4/IPV6 address plus port", parse_string);
                return;
            }
            connect_addr_maybe = Some(connect_addr_res.unwrap());
        }
        else if arg == "--use-ipv6"
        {
            if use_ipv6
            {
                println!("You can't provide the --use-ipv6 flag twice.");
                return;
            }
            use_ipv6 = true;
        }
        else
        {
            println!("Failed to parse argument: {}", &arg);
            return;
        }
    }

    if period_maybe.is_none()
    {
        println!("You must assign a period.");
        return;
    }
    if port_maybe.is_none()
    {
        println!("You must assign a port");
        return;
    }

    use_ipv6 |= connect_addr_maybe.is_some() && connect_addr_maybe.unwrap().is_ipv6();

    if do_peer(use_ipv6, Duration::from_secs(period_maybe.unwrap()), port_maybe.unwrap(), connect_addr_maybe.as_ref(), None, &mut Vec::new(), false).is_none()
    {
        println!("Failed to connect to initial peer");
        return;
    }
}
