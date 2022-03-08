use crate::*;

use std::time::Duration;

use std::net::IpAddr;
use std::sync::{Arc, Mutex};

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
