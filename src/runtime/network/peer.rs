use crate::runtime::network::error::NetworkError;
use crate::runtime::network::message::{NetworkEnvelope, NetworkMessage, PeerInfo};
use crate::runtime::network::transport::{read_message, write_message};
use std::io::{Read, Write};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer<S> {
    stream: S,
    info: PeerInfo,
}

impl<S> Peer<S> {
    pub fn new(stream: S, info: PeerInfo) -> Self {
        Self { stream, info }
    }

    pub fn info(&self) -> &PeerInfo {
        &self.info
    }

    pub fn stream_mut(&mut self) -> &mut S {
        &mut self.stream
    }

    pub fn into_inner(self) -> S {
        self.stream
    }
}

impl<S: Read + Write> Peer<S> {
    pub fn send(&mut self, message: NetworkMessage) -> Result<(), NetworkError> {
        write_message(&mut self.stream, &message.to_envelope())
    }

    pub fn send_envelope(&mut self, envelope: &NetworkEnvelope) -> Result<(), NetworkError> {
        write_message(&mut self.stream, envelope)
    }

    pub fn recv(&mut self) -> Result<NetworkEnvelope, NetworkError> {
        read_message(&mut self.stream)
    }
}
