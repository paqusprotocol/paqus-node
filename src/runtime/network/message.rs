use crate::runtime::network::error::NetworkError;
use crate::runtime::params::{
    CHAIN_ID, CHAIN_NAME, MAX_NETWORK_MESSAGE_SIZE, NETWORK_MAGIC, PROTOCOL_STAGE, PROTOCOL_VERSION,
};
use borsh::{BorshDeserialize, BorshSerialize};
use paqus::block::Block;
use paqus::transaction::SignedTransaction;
use paqus::types::{BlockHash, BlockHeight};

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct PeerInfo {
    pub address: String,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TipInfo {
    pub height: BlockHeight,
    pub hash: BlockHash,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct VersionInfo {
    pub protocol_version: u8,
    pub chain_id: u8,
    pub chain_name: String,
    pub protocol_stage: String,
    pub network_magic: [u8; 4],
    pub tip: Option<TipInfo>,
}

impl VersionInfo {
    pub fn local(tip: Option<TipInfo>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            chain_id: CHAIN_ID,
            chain_name: CHAIN_NAME.to_string(),
            protocol_stage: PROTOCOL_STAGE.to_string(),
            network_magic: NETWORK_MAGIC,
            tip,
        }
    }

    pub fn validate_compatibility(&self) -> Result<(), RejectReason> {
        if self.network_magic != NETWORK_MAGIC {
            return Err(RejectReason::NetworkMismatch);
        }
        if self.chain_id != CHAIN_ID
            || self.chain_name != CHAIN_NAME
            || self.protocol_stage != PROTOCOL_STAGE
        {
            return Err(RejectReason::ChainMismatch);
        }
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(RejectReason::ProtocolVersionMismatch);
        }
        Ok(())
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum RejectReason {
    ProtocolVersionMismatch,
    ChainMismatch,
    NetworkMismatch,
    InvalidMessage,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum NetworkMessage {
    Version(VersionInfo),
    VerAck(VersionInfo),
    Reject {
        reason: RejectReason,
        message: String,
    },
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    GetTip,
    Tip(TipInfo),
    GetBlockByHeight {
        height: BlockHeight,
    },
    GetBlockByHash {
        hash: BlockHash,
    },
    Block(Block),
    Transaction(SignedTransaction),
    GetPeers,
    Peers(Vec<PeerInfo>),
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NetworkEnvelope {
    pub magic: [u8; 4],
    pub message: NetworkMessage,
}

impl NetworkEnvelope {
    pub fn new(message: NetworkMessage) -> Self {
        Self {
            magic: NETWORK_MAGIC,
            message,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, NetworkError> {
        let bytes = borsh::to_vec(self)?;
        if bytes.len() > MAX_NETWORK_MESSAGE_SIZE {
            return Err(NetworkError::MessageTooLarge);
        }
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NetworkError> {
        if bytes.len() > MAX_NETWORK_MESSAGE_SIZE {
            return Err(NetworkError::MessageTooLarge);
        }

        let envelope = Self::try_from_slice(bytes)?;
        if envelope.magic != NETWORK_MAGIC {
            return Err(NetworkError::Serialization(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "network magic mismatch",
            )));
        }
        Ok(envelope)
    }
}

impl NetworkMessage {
    pub fn to_envelope(self) -> NetworkEnvelope {
        NetworkEnvelope::new(self)
    }
}
