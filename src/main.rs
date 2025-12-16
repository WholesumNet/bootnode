
#![doc = include_str!("../README.md")]

// use std::num::NonZeroUsize;
// use std::ops::Add;
use std::time::{
    Duration
};

use futures::{
    StreamExt, select
};

use tokio::{
    time::{
    self, interval
    },
};
use tokio_stream::wrappers::IntervalStream;

use libp2p::{
    identity, identify, 
    kad,
    PeerId,
    swarm::{
        SwarmEvent
    },
};

use tracing_subscriber::EnvFilter;
use clap::Parser;

use peyk::p2p::{
    BootNodeBehaviourEvent,
    setup_swarm_for_bootnode
};

// const BOOTNODES: [&str; 4] = [
//     "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
//     "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
//     "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
//     "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
// ];

// CLI
#[derive(Parser, Debug)]
#[command(name = "Bootnode CLI for Wholesum: p2p verifiable computing marketplace.")]
#[command(author = "Wholesum team")]
#[command(version = "0.1")]
#[command(about = "Yet another verifiable compute marketplace.", long_about = None)]
struct Cli {
    #[arg(short, long)]
    key_file: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let cli = Cli::parse();

    // key 
    let local_key = {
        if let Some(key_file) = cli.key_file {
            let bytes = std::fs::read(key_file).unwrap();
            identity::Keypair::from_protobuf_encoding(&bytes)?
        } else {
            // Create a random key for ourselves
            let new_key = identity::Keypair::generate_ed25519();
            let bytes = new_key.to_protobuf_encoding().unwrap();
            let _bw = std::fs::write("./key.secret", bytes);
            println!("No keys were supplied, so one is generated for you and saved to `./key.secret` file.");
            new_key
        }
    };   
    println!("my peer id: `{:?}`", PeerId::from_public_key(&local_key.public()));    
 

    // import the keypair
    // let bytes = std::fs::read(cli.key).unwrap();
    // let local_key = identity::Keypair::from_protobuf_encoding(&bytes)?;
    // println!("{:?}", PeerId::from_public_key(&local_key.public()));

    // Create a random key for ourself
    // let local_key = identity::Keypair::generate_ed25519();
    // let bytes = local_key.to_protobuf_encoding().unwrap();
    // std::fs::write("keypair.secret", bytes);

    // Add the bootnodes to the local routing table. `libp2p-dns` built
    // into the `transport` resolves the `dnsaddr` when Kademlia tries
    // to dial these nodes.
    // for peer in &BOOTNODES {
    //     swarm
    //         .behaviour_mut()
    //         .add_address(&peer.parse()?, "/dnsaddr/bootstrap.libp2p.io".parse()?);
    // }
    let mut swarm = setup_swarm_for_bootnode(&local_key)?;
    // let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key.clone())
    //     .with_async_std()
    //     .with_tcp(
    //         tcp::Config::default(),
    //         noise::Config::new,
    //         yamux::Config::default,
    //     )?
    //     .with_dns()?
    //     .with_behaviour(|key| {
    //         // Create a Kademlia behaviour.
    //         let mut cfg = kad::Config::default();
    //         cfg.set_query_timeout(Duration::from_secs(5 * 60));
    //         let store = kad::store::MemoryStore::new(key.public().to_peer_id());
    //         kad::Behaviour::with_config(key.public().to_peer_id(), store, cfg)
    //     })?
    //     .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(5)))
    //     .build();
    swarm.listen_on("/ip4/127.0.0.1/udp/20201/quic-v1".parse()?)?;
    swarm.listen_on("/ip4/127.0.0.1/tcp/20201".parse()?)?;
    swarm.listen_on("/ip6/::1/tcp/20201".parse()?)?;
    swarm.listen_on("/ip6/::1/udp/20201/quic-v1".parse()?)?;

    let mut timer_peer_discovery = IntervalStream::new(
        interval(Duration::from_secs(60))
    )
    .fuse();
    
    println!("protocol names: {:#?}", swarm.behaviour_mut().kademlia.protocol_names());
    loop {
        select! {
            // try to discover new peers
            _i = timer_peer_discovery.select_next_some() => {
                let random_peer_id = PeerId::random();
                println!("Searching for the closest peers to `{random_peer_id}`");
                swarm.behaviour_mut()
                    .kademlia.get_closest_peers(random_peer_id);
            },

            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}");
                },

                SwarmEvent::Behaviour(BootNodeBehaviourEvent::Identify(identify::Event::Received {
                    peer_id: remote_peer_id,
                    info
                })) => {
                    println!("Inbound identify event from `{}`: `{:?}`", remote_peer_id, info);
                },

                SwarmEvent::Behaviour(BootNodeBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                    result: kad::QueryResult::GetClosestPeers(Ok(ok)),
                    ..
                })) => {
                    // The example is considered failed as there
                    // should always be at least 1 reachable peer.
                    if ok.peers.is_empty() {
                        eprintln!("Query finished with no closest peers.");
                    }

                    println!("Query finished with closest peers: {:#?}", ok.peers);
                },

                SwarmEvent::Behaviour(BootNodeBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                    result:
                        kad::QueryResult::GetClosestPeers(Err(kad::GetClosestPeersError::Timeout {
                            ..
                        })),
                    ..
                })) => {
                    eprintln!("Query for closest peers timed out");
                },
                
                _ => {},
            }
        }
    }
}
