
#![doc = include_str!("../README.md")]

use std::{
    time::{
        Duration
    },
    env,
};

use futures::{
    StreamExt, select
};

use anyhow::Context;

use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

use libp2p::{
    core::ConnectedPoint,
    identity,
    identify, 
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

    let my_ip_addr = env::var("MY_IP_ADDR")
        .context("`MY_IP_ADDR` environment variable does not exist.")?;     
    let mut swarm = setup_swarm_for_bootnode(&local_key)?;    
    swarm.listen_on(
        format!(
            "/ip4/{}/udp/20201/quic-v1",
            my_ip_addr
        )
        .parse()?
    )?;
    swarm.listen_on(
        format!(
            "/ip4/{}/tcp/20201",
            my_ip_addr
        )
        .parse()?
    )?;

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
                    .kademlia
                    .get_closest_peers(random_peer_id);                
            },

            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}");
                },

                SwarmEvent::ConnectionEstablished {
                    peer_id,
                    endpoint,
                    ..
                } => {
                    println!(
                        "A connection has been established to {}@{:?}",
                        peer_id,
                        endpoint
                    );
                    let addr = match endpoint {
                        ConnectedPoint::Dialer { address, .. } => {
                            address
                        },

                        ConnectedPoint::Listener { send_back_addr, .. } => {
                            send_back_addr
                        }
                    };
                    swarm.behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr);
                },

                SwarmEvent::Behaviour(BootNodeBehaviourEvent::Identify(identify::Event::Received {
                    peer_id,
                    info
                })) => {
                    println!(
                        "Inbound identify event from {}: {:#?}",
                        peer_id,
                        info
                    );                                                       
                },

                SwarmEvent::Behaviour(BootNodeBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                    result: kad::QueryResult::GetClosestPeers(Ok(ok)),
                    ..
                })) => {
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
                
                _ => {
                    println!("{:#?}", event);
                },
            }
        }
    }
}
