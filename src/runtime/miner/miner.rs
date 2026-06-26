use crate::runtime::mempool::Mempool;
use paqus::block::Block;
use paqus::consensus::{Consensus, ConsensusError};
use paqus::ledger::Ledger;
use paqus::types::{Address, Nonce};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningConfig {
    pub difficulty: u32,
    pub max_attempts: u64,
    pub transaction_limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiningResult {
    pub block: Block,
    pub attempts: u64,
}

pub fn mine_candidate_block(
    mempool: &Mempool,
    ledger: &Ledger,
    consensus: &Consensus,
    miner_address: Address,
    timestamp: u64,
    config: MiningConfig,
) -> Result<Option<MiningResult>, ConsensusError> {
    let mut block = mempool
        .create_candidate_block(
            ledger,
            miner_address,
            timestamp,
            Nonce(0),
            config.transaction_limit,
        )
        .map_err(|_| ConsensusError::InvalidBlock(paqus::block::BlockError::InvalidStateRoot))?;
    block.header.difficulty = config.difficulty;

    for attempt in 0..config.max_attempts {
        block.header.nonce = Nonce(attempt);
        if config.difficulty == 0 {
            return Ok(Some(MiningResult {
                block,
                attempts: attempt.saturating_add(1),
            }));
        }

        let hash = consensus.proof_of_work_hash(&block)?;
        if consensus
            .validate_proof_of_work_hash_with_difficulty(&hash, config.difficulty)
            .is_ok()
        {
            return Ok(Some(MiningResult {
                block,
                attempts: attempt.saturating_add(1),
            }));
        }
    }

    Ok(None)
}
