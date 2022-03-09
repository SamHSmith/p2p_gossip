use crate::*;

use std::time::Duration;

use std::net::IpAddr;
use std::sync::{Arc, Mutex};

/// This test is used to make sure the networks peer discovery and disconnect handling works.
/// It does this by building a network shaped like a chain, every node is initially connected
/// to only 2 others. The two edge nodes are used to test the network, one will send gossip,
/// the other will record the gossip it "hears". During the test non edge nodes are taken offline
/// without notice to the other peers until the network only has two peers left, the edge peers.
/// This is done using the `self_destruct_time` optional argument to `do_peer`.
///
/// Since one node is sending and one node is receiving we can simplify the problem of testing
/// to gossip *awareness*. Passing `should_use_gossip_awareness` as true on the edge nodes causes
/// them to accumulate the gossips they send and receive in the associated `gossip_awareness` array.
/// The two awareness arrays of the edge nodes are compared at the end and if the receiving node has
/// not received all the sent gossips the test fails.
///
/// If any of the nodes either fails to start listening on a tcp port or fails to connect
/// to their initial peer there will be a panic and the test will fail. As is the nature with these
/// things, the tests could fail because the ports are *in use* by another process on the machine.
///
/// It may be wise to move away panics and instead simply assert a Result type returned from do_peer.
#[test]
fn dying_chain_ipv4_test() {
    let base_port = 11400;
    std::thread::spawn(move || {
        do_peer(
            false,
            Duration::from_secs(2000),
            base_port + 1,
            None,
            Some(Duration::from_secs(30)),
            &mut Vec::new(),
            false,
        ).unwrap();
    });

    std::thread::sleep(Duration::from_millis(20));

    let middle_count = 13u16;
    for i in 1..middle_count {
        std::thread::spawn(move || {
            do_peer(
                false,
                Duration::from_secs(2000),
                base_port + 1 + i,
                Some(&SocketAddr::new(
                    IpAddr::from("127.0.0.1".parse::<Ipv4Addr>().unwrap()),
                    base_port + i,
                )),
                Some(Duration::from_secs(30)),
                &mut Vec::new(),
                false,
            ).unwrap();
        });
        std::thread::sleep(Duration::from_millis(1500));
    }

    let received_gossips = Arc::new(Mutex::new(Vec::new()));
    let received_gossips2 = received_gossips.clone();

    std::thread::spawn(move || {
        let mut array = received_gossips.lock().unwrap();
        do_peer(
            false,
            Duration::from_secs(2000),
            base_port + 1 + middle_count,
            Some(&SocketAddr::new(
                IpAddr::from("127.0.0.1".parse::<Ipv4Addr>().unwrap()),
                base_port + middle_count,
            )),
            Some(Duration::from_secs(35)),
            &mut *array,
            true,
        ).unwrap();
    });

    std::thread::sleep(Duration::from_millis(800));

    let mut sent_gossips = Vec::new();

    do_peer(
        false,
        Duration::from_secs(1),
        base_port,
        Some(&SocketAddr::new(
            IpAddr::from("127.0.0.1".parse::<Ipv4Addr>().unwrap()),
            base_port + 1,
        )),
        Some(Duration::from_secs(30)),
        &mut sent_gossips,
        true,
    ).unwrap();

    let array = received_gossips2.lock().unwrap();
    for gossip in sent_gossips
    {
        assert!(array.contains(&gossip));
        let mut s = String::with_capacity(2 * GOSSIP_LEN);
        for byte in gossip.iter() {
            write!(s, "{:02X}", byte).unwrap();
        }
        println!("0x{} was sent and received", s);
    }
}

/// This test is similar to `dying_chain_ipv4_test` except it only has 3 nodes and so it
/// does not test the peer discovering capabilities. It uses ipv6 instead of ipv4 and so it
/// might fail on a system without ipv6 support.
#[test]
fn three_way_ipv6_test() {
    let base_port = 11500;
    std::thread::spawn(move || {
        do_peer(
            true,
            Duration::from_secs(2000),
            base_port + 1,
            None,
            Some(Duration::from_secs(20)),
            &mut Vec::new(),
            false,
        ).unwrap();
    });

    std::thread::sleep(Duration::from_millis(20));

    let received_gossips = Arc::new(Mutex::new(Vec::new()));
    let received_gossips2 = received_gossips.clone();

    std::thread::spawn(move || {
        let mut array = received_gossips.lock().unwrap();
        do_peer(
            true,
            Duration::from_secs(2000),
            base_port + 1 + 1,
            Some(&SocketAddr::new(
                IpAddr::from("::1".parse::<Ipv6Addr>().unwrap()),
                base_port + 1,
            )),
            Some(Duration::from_secs(20)),
            &mut *array,
            true,
        ).unwrap();
    });

    std::thread::sleep(Duration::from_millis(300));

    let mut sent_gossips = Vec::new();

    do_peer(
        true,
        Duration::from_secs(1),
        base_port,
        Some(&SocketAddr::new(
            IpAddr::from("::1".parse::<Ipv6Addr>().unwrap()),
            base_port + 1,
        )),
        Some(Duration::from_secs(15)),
        &mut sent_gossips,
        true,
    ).unwrap();

    let array = received_gossips2.lock().unwrap();
    for gossip in sent_gossips
    {
        assert!(array.contains(&gossip));
        let mut s = String::with_capacity(2 * GOSSIP_LEN);
        for byte in gossip.iter() {
            write!(s, "{:02X}", byte).unwrap();
        }
        println!("0x{} was sent and received", s);
    }
}
