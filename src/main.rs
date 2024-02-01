
#![doc = include_str!("../README.md")]

use std::error::Error;
// use std::num::NonZeroUsize;
// use std::ops::Add;
use std::time::{Duration};

use futures::{
    StreamExt, select
};
use async_std::stream;

use libp2p::{
    identity, identify, 
    kad,
    PeerId,
    swarm::{SwarmEvent},
};

use tracing_subscriber::EnvFilter;
use tracing::{debug, info, error, warn};
use clap::Parser;

use comms::p2p::{
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
    key: String,
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .try_init();

    // import the keypair
    let cli = Cli::parse();
    let bytes = std::fs::read(cli.key).unwrap();
    let local_key = identity::Keypair::from_protobuf_encoding(&bytes)?;
    info!("{:?}", PeerId::from_public_key(&local_key.public()));
    // Create a random key for ourselves
    // let local_key = identity::Keypair::generate_ed25519();
    // let bytes = local_key.to_protobuf_encoding().unwrap();
    // std::fs::write("keypair.secret", bytes);

    let mut swarm = setup_swarm_for_bootnode(&local_key).await?;
    swarm.behaviour_mut()
        .kademlia
        .set_mode(Some(kad::Mode::Server));

    swarm.listen_on("/ip4/0.0.0.0/udp/20201/quic-v1".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/20201".parse()?)?;
    swarm.listen_on("/ip6/::/tcp/20201".parse()?)?;
    swarm.listen_on("/ip6/::/udp/20201/quic-v1".parse()?)?;


    let mut timer_peer_discovery = stream::interval(Duration::from_secs(5 * 60)).fuse();
    
    info!("protocols in use: `{:?}`", 
        swarm.behaviour_mut().kademlia.protocol_names());
    loop {
        select! {
            // try to discover new peers
            () = timer_peer_discovery.select_next_some() => {
                let random_peer_id = PeerId::random();
                info!("Searching for the closest peers to `{}`", random_peer_id);
                swarm.behaviour_mut()
                    .kademlia.get_closest_peers(random_peer_id);
            },

            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("Local node is listening on {}", address);
                },

                SwarmEvent::Behaviour(BootNodeBehaviourEvent::Identify(identify::Event::Received {
                    peer_id,
                    info,
                    ..
                })) => {
                    info!("Inbound identify: `{:#?}`", info);
                    for addr in info.listen_addrs {
                        // if false == addr.iter().any(|item| item == &"127.0.0.1" || item == &"::1"){
                        swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr);
                        // }
                    }

                },
                
                _ => info!("{:#?}", event),
            }
        }
    }
}
