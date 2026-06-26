use crate::network::send_message;
use crate::p2p::PeerState;
use crate::paquscore::NetworkMessage;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

pub fn broadcast_to_peers(
    peers: &Arc<Mutex<HashMap<SocketAddr, PeerState>>>,
    message: NetworkMessage,
) {
    let peers = match peers.lock() {
        Ok(peers) => peers.keys().copied().collect::<Vec<_>>(),
        Err(_) => {
            eprintln!("peer state lock poisoned");
            return;
        }
    };
    for peer in peers {
        if let Err(error) = send_message(peer, message.clone()) {
            eprintln!("broadcast to {peer} failed: {error}");
        }
    }
}
