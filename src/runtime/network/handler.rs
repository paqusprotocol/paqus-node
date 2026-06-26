use crate::runtime::network::error::NetworkError;
use crate::runtime::network::message::{NetworkMessage, TipInfo, VersionInfo};
use crate::runtime::node::Node;

pub fn handle_message(
    node: &mut Node,
    message: NetworkMessage,
) -> Result<Option<NetworkMessage>, NetworkError> {
    match message {
        NetworkMessage::Version(version) => match version.validate_compatibility() {
            Ok(()) => Ok(Some(NetworkMessage::VerAck(local_version(node)))),
            Err(reason) => Ok(Some(NetworkMessage::Reject {
                reason,
                message: "incompatible peer version".to_string(),
            })),
        },
        NetworkMessage::VerAck(_) => Ok(None),
        NetworkMessage::Reject { .. } => Ok(None),
        NetworkMessage::Ping { nonce } => Ok(Some(NetworkMessage::Pong { nonce })),
        NetworkMessage::Pong { .. } => Ok(None),
        NetworkMessage::GetTip => Ok(node
            .tip_height()
            .zip(node.tip_hash())
            .map(|(height, hash)| NetworkMessage::Tip(TipInfo { height, hash }))),
        NetworkMessage::Tip(_) => Ok(None),
        NetworkMessage::GetBlockByHeight { height } => Ok(node
            .ledger
            .block(&height)
            .cloned()
            .map(NetworkMessage::Block)),
        NetworkMessage::GetBlockByHash { hash } => Ok(node
            .cache
            .block_by_hash(&hash)
            .cloned()
            .map(NetworkMessage::Block)),
        NetworkMessage::Block(block) => {
            node.apply_block(block)?;
            Ok(None)
        }
        NetworkMessage::Transaction(transaction) => {
            node.submit_transaction(transaction)?;
            Ok(None)
        }
        NetworkMessage::GetPeers => Ok(Some(NetworkMessage::Peers(vec![]))),
        NetworkMessage::Peers(_) => Ok(None),
    }
}

fn local_version(node: &Node) -> VersionInfo {
    VersionInfo::local(
        node.tip_height()
            .zip(node.tip_hash())
            .map(|(height, hash)| TipInfo { height, hash }),
    )
}
