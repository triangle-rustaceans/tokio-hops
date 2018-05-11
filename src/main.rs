extern crate bytes;
extern crate clap;
extern crate futures;
extern crate rand;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate tokio;
extern crate tokio_io;

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::{BufMut, BytesMut};
use clap::{Arg, App};
use rand::{Rng, thread_rng, seq};
use serde_json::{de, ser};
use tokio::io;
use tokio::net::{UdpSocket, UdpFramed};
use tokio::prelude::*;
use tokio::timer::Delay;
use tokio_io::codec::{Encoder, Decoder};


pub struct Codec;

impl Codec {
    fn new() -> Codec { Codec }
}

impl Encoder for Codec {
    type Item = Msg;
    type Error = io::Error;

    fn encode(&mut self, item: Msg, dst: &mut BytesMut) -> Result<(), io::Error> {
        dst.put(ser::to_string(&item)?);
        Ok(())
    }
}

impl Decoder for Codec {
    type Item = Msg;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Msg>, io::Error> {
        let len = src.len();
        let b = &src.split_to(len)[..];
        let s = ::std::str::from_utf8(b).expect("valid utf8");
        let ping = de::from_str(s)?;
        Ok(Some(ping))
    }
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ping {
    pub source: u16, 
    pub hops: u32
} 

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Msg {
    Ping(Ping),
    Done
}

fn random_neighbor<R: Rng>(rng: &mut R, neighbors: &[SocketAddr]) -> SocketAddr {
    *seq::sample_iter(rng, neighbors, 1).unwrap()[0]
}

fn make_hop_server_task(port: u16, neighbor_ports: &[u16]) -> impl Future<Item=(), Error=()> {
    let mut rng = thread_rng();
    let neighbors: Vec<SocketAddr> = neighbor_ports.into_iter()
        .map(|port| ([0, 0, 0, 0, 0, 0, 0, 1], *port).into()).collect();
    let first_neighbor = random_neighbor(&mut rng, &neighbors);
    let addr = ([0, 0, 0, 0, 0, 0, 0, 1], port).into();

    let (udp_tx, udp_rx) = UdpFramed::new(
        UdpSocket::bind(&addr).expect("bind udp socket"),
        Codec::new(),
    ).split();

    let bouncer = move |udp_tx| { 
        udp_rx
            .and_then(move |(msg, _addr)| match msg {
                Msg::Ping(ping) => {
                    let mut rng = thread_rng();
                    let neighbor = random_neighbor(&mut rng, &neighbors);
                    Ok((Msg::Ping(Ping { source: ping.source, hops: ping.hops + 1 }), neighbor))
                }
                Msg::Done => Err(io::Error::new(io::ErrorKind::Other, "Server was Done")),
            })
            .filter(move |(msg, addr)| 
                if let Msg::Ping(ping) = msg {
                    if ping.source == port {
                        println!("Server {} Received ping from {} after {} hops", port, addr, ping.hops);
                        false
                    } else {
                        true
                    }
                } else {
                    false
                }
            )
            .forward(udp_tx)
            .and_then(|_| Ok(()))
    };

    let server_task = Delay::new(Instant::now() + Duration::from_secs(1))
        .then(
            move |_| udp_tx.send((Msg::Ping(Ping { source: port, hops: 0 }), first_neighbor))
        )
        .and_then(move |udp_tx| bouncer(udp_tx))
        .map_err(|err| println!("Err: {:?}", err));
    server_task
}

fn main() -> Result<(), Box<std::error::Error>> {
    let matches = App::new("test")
        .arg(Arg::with_name("port")
             .short("p")
             .long("port")
             .required(true)
             .takes_value(true))
        .arg(Arg::with_name("neighbors")
             .multiple(true))
        .get_matches();
    let port = matches.value_of("port").expect("port").parse::<u16>().expect("u16 port");
    let neighbors = matches.values_of("neighbors").expect("neighbors")
        .map(|x| x.parse::<u16>().expect("u16 neighbor port"))
        .collect::<Vec<_>>();
    println!("Server running on [::1]:{}", port);
    println!("Connecting to neighbors at ports: {:?}", &neighbors);
    let server_task = make_hop_server_task(port, &neighbors);
    
    tokio::run(server_task);
    Ok(())
}
