extern crate bytes;
extern crate clap;
extern crate futures;
extern crate rand;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate tokio;
extern crate tokio_io;

use std::sync::Arc;
use bytes::{BufMut, BytesMut};
use clap::{Arg, App};
use rand::random;
use serde_json::{de, ser};
use tokio::io;
use tokio::net::{UdpSocket, UdpFramed};
use tokio::prelude::*;
use tokio_io::codec::{Encoder, Decoder};

pub struct Codec;

impl Codec {
    fn new() -> Codec { Codec }
}

impl Encoder for Codec {
    type Item = Ping;
    type Error = io::Error;

    fn encode(&mut self, item: Ping, dst: &mut BytesMut) -> Result<(), io::Error> {
        dst.put(ser::to_string(&item)?);
        Ok(())
    }
}

impl Decoder for Codec {
    type Item = Ping;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Ping>, io::Error> {
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

// #[derive(Clone, Debug, Serialize, Deserialize)]
// enum Msg {
//     Ping(Ping),
//     Done
// }

fn main() {
    let matches = App::new("test")
        .arg(Arg::with_name("port")
             .short("p")
             .long("port")
             .takes_value(true))
        .arg(Arg::with_name("neighbors")
             .multiple(true))
        .get_matches();
    let port = matches.value_of("port").expect("port").parse::<u16>().expect("u16 port");

    let neighbors = matches.values_of("neighbors").expect("neighbors")
        .map(|x| x.parse::<u16>().expect("u16 neighbor port"))
        .collect::<Vec<_>>();
    println!("Connecting to neighbors at ports: {:?}", &neighbors);
    let addr = ([0, 0, 0, 0, 0, 0, 0, 1], port).into();

    let (udp_tx, udp_rx) = UdpFramed::new(
        UdpSocket::bind(&addr).expect("bind udp socket"),
        Codec::new(),
    ).split();

    //let neighbor = neighbors[random::<usize>() % neighbors.len()];
    let server = udp_rx
        //.send((Ping { source: port, hops: 0 }, &neighbor))
        //.and_then(|_| udp_rx)
        .map_err(|err| {
            println!("accept error = {:?}", err);
        })
        .for_each(move |(ping, addr)| {
            let addr = &addr.clone();
            let udpsock = UdpSocket::bind(&addr).expect("bind inner socket");
            if ping.source == port {
                println!("Received ping from {} after {} hops", addr, ping.hops);
            } else {
                let dest = random::<usize>() % &neighbors.len();
                let neighbor = ([0, 0, 0, 0, 0, 0, 0, 1], *&neighbors[dest]).into();
                let dgram = udpsock
                    .send_dgram(ser::to_string(&Ping { source: ping.source, hops: ping.hops + 1 }).expect("serialize ping"), &neighbor)
                    .map(|_|())
                    .map_err(|err| println!("an error occurred: {:?}", err));
                tokio::spawn(dgram);
            };
            Ok(())
        });
    println!("Server running on {:?}", addr);
    tokio::run(server);
}
