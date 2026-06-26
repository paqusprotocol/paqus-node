use crate::runtime::cache::CoreCache;
use crate::runtime::mempool::Mempool;
use crate::runtime::miner::{MiningConfig, MiningResult, mine_candidate_block};
use crate::runtime::node::error::NodeError;
use crate::runtime::params::DIFFICULTY_ADJUSTMENT_INTERVAL;
use crate::runtime::storage::Storage;
use paqus::block::Block;
use paqus::consensus::Consensus;
use paqus::genesis::{GENESIS_HASH, genesis_block};
use paqus::ledger::fork_choice::ForkChoice;
use paqus::ledger::{Chain, Ledger};
use paqus::transaction::SignedTransaction;
use paqus::types::{
    AccountNonce, Address, Amount, Balance, BlockHash, BlockHeight, TransactionHash,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingBalance {
    pub incoming: Amount,
    pub outgoing: Amount,
}

impl Default for PendingBalance {
    fn default() -> Self {
        Self {
            incoming: Amount(0),
            outgoing: Amount(0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BalanceSummary {
    pub confirmed: Amount,
    pub available: Amount,
    pub pending: PendingBalance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountView {
    pub balance: Amount,
    pub unspendable: Amount,
    pub nonce: AccountNonce,
}

impl Default for BalanceSummary {
    fn default() -> Self {
        Self {
            confirmed: Amount(0),
            available: Amount(0),
            pending: PendingBalance::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub ledger: Ledger,
    pub mempool: Mempool,
    pub storage: Storage,
    pub consensus: Consensus,
    pub cache: CoreCache,
    pub fork_choice: ForkChoice,
    genesis_accounts: BTreeMap<Address, paqus::state::Account>,
}

impl Node {
    pub fn new(ledger: Ledger, storage: Storage, consensus: Consensus) -> Self {
        let genesis_accounts = if ledger.tip_height() == Some(paqus::types::Height(0)) {
            ledger.accounts.clone()
        } else {
            BTreeMap::new()
        };
        Self::with_genesis_accounts(ledger, storage, consensus, genesis_accounts)
    }

    pub fn with_genesis_accounts(
        ledger: Ledger,
        storage: Storage,
        consensus: Consensus,
        genesis_accounts: BTreeMap<Address, paqus::state::Account>,
    ) -> Self {
        let cache = CoreCache::from_ledger(&ledger);
        let mut fork_choice = ForkChoice::new();
        for block in ledger.chain.blocks.values() {
            fork_choice
                .insert_block(block.clone())
                .expect("ledger chain should build a valid fork choice index");
        }
        Self {
            ledger,
            mempool: Mempool::new(),
            storage,
            consensus,
            cache,
            fork_choice,
            genesis_accounts,
        }
    }

    pub fn temporary(ledger: Ledger, consensus: Consensus) -> Result<Self, NodeError> {
        Ok(Self::new(ledger, Storage::temporary()?, consensus))
    }

    pub fn init_or_load(path: impl AsRef<Path>, consensus: Consensus) -> Result<Self, NodeError> {
        let storage = Storage::open(path)?;
        let ledger = if storage.load_tip()?.is_some() {
            storage.load_ledger()?
        } else {
            let genesis = genesis_block();
            assert_eq!(genesis.hash().0, GENESIS_HASH);
            let mut ledger = Ledger::new();
            ledger.apply_block(genesis)?;
            storage.save_ledger(&ledger)?;
            storage.save_genesis_accounts(&ledger.accounts)?;
            ledger
        };

        let genesis_accounts = storage.load_genesis_accounts()?.unwrap_or_else(|| {
            if ledger.tip_height() == Some(paqus::types::Height(0)) {
                ledger.accounts.clone()
            } else {
                BTreeMap::new()
            }
        });
        Ok(Self::with_genesis_accounts(
            ledger,
            storage,
            consensus,
            genesis_accounts,
        ))
    }

    pub fn submit_transaction(
        &mut self,
        transaction: SignedTransaction,
    ) -> Result<TransactionHash, NodeError> {
        Ok(self
            .mempool
            .insert_validated_at(&self.ledger, transaction, current_unix_timestamp())?)
    }

    pub fn apply_block(&mut self, block: Block) -> Result<(), NodeError> {
        self.validate_block_for_known_parent(&block)?;
        let block_hash = self.fork_choice.insert_block(block.clone())?;
        let best_tip_hash = self.fork_choice.best_tip().map(|node| node.hash);

        if best_tip_hash != Some(block_hash) {
            self.storage.save_block(&block)?;
            return Ok(());
        }

        let extends_active_tip = match self.ledger.tip_hash() {
            Some(tip_hash) => block.previous_hash() == tip_hash,
            None => block.height().0 == 0,
        };
        if !extends_active_tip {
            return self.reorg_to_best_tip();
        }

        self.ledger.apply_block(block.clone())?;
        self.mempool.remove_confirmed(&block);
        self.cache.insert_block(block.clone());
        for transaction in &block.transactions {
            if let Some(sender) = self.ledger.account(&transaction.payload.from) {
                self.cache.insert_account(sender.clone());
            }
            if let Some(receiver) = self.ledger.account(&transaction.payload.to) {
                self.cache.insert_account(receiver.clone());
            }
        }
        if let Some(miner) = self.ledger.account(&block.miner_address()) {
            self.cache.insert_account(miner.clone());
        }
        self.storage.save_ledger(&self.ledger)?;
        Ok(())
    }

    fn reorg_to_best_tip(&mut self) -> Result<(), NodeError> {
        if self.genesis_accounts.is_empty() {
            return Err(NodeError::MissingGenesisState);
        }

        let old_blocks: Vec<_> = self.ledger.chain.blocks.values().cloned().collect();
        let old_tip_hash = self.ledger.tip_hash();
        let best_tip = self
            .fork_choice
            .best_tip()
            .ok_or(NodeError::ReorgRequired)?
            .hash;
        let ancestor = self
            .common_ancestor(old_tip_hash, best_tip)
            .ok_or(NodeError::ReorgRequired)?;
        let winning_branch = self
            .fork_choice
            .branch_from_ancestor(ancestor, best_tip)
            .ok_or(NodeError::ReorgRequired)?;

        let mut rebuilt = Ledger {
            accounts: self.genesis_accounts.clone(),
            chain: Chain::new(),
        };
        let genesis = self
            .fork_choice
            .get(
                &self
                    .fork_choice
                    .ancestor_hashes(best_tip)
                    .last()
                    .copied()
                    .unwrap_or(best_tip),
            )
            .ok_or(NodeError::ReorgRequired)?
            .block
            .clone();
        rebuilt.chain.insert_block(genesis)?;

        let full_branch = self
            .fork_choice
            .branch_from_ancestor(
                rebuilt.tip_hash().ok_or(NodeError::ReorgRequired)?,
                best_tip,
            )
            .ok_or(NodeError::ReorgRequired)?;
        for block in full_branch {
            rebuilt.apply_block(block)?;
        }

        self.ledger = rebuilt;
        self.cache = CoreCache::from_ledger(&self.ledger);
        self.storage.save_ledger(&self.ledger)?;

        let winning_hashes: std::collections::BTreeSet<_> = self
            .fork_choice
            .ancestor_hashes(best_tip)
            .into_iter()
            .collect();
        for old_block in old_blocks {
            if winning_hashes.contains(&old_block.hash()) {
                continue;
            }
            for transaction in old_block.transactions {
                let _ = self.mempool.insert_validated(&self.ledger, transaction);
            }
        }

        // Keep the variable intentionally used for the common ancestor search, even when the
        // winning branch starts at genesis.
        let _ = winning_branch;
        Ok(())
    }

    fn common_ancestor(&self, old_tip: Option<BlockHash>, new_tip: BlockHash) -> Option<BlockHash> {
        let old_tip = old_tip?;
        let old_ancestors: std::collections::BTreeSet<_> = self
            .fork_choice
            .ancestor_hashes(old_tip)
            .into_iter()
            .collect();

        self.fork_choice
            .ancestor_hashes(new_tip)
            .into_iter()
            .find(|hash| old_ancestors.contains(hash))
    }

    fn validate_block_for_known_parent(&self, block: &Block) -> Result<(), NodeError> {
        let now = current_unix_timestamp();
        if block.height().0 == 0 {
            self.consensus.validate_genesis_block_at(block, now)?;
            return Ok(());
        }

        let parent = self
            .fork_choice
            .get(&BlockHash::from(block.previous_hash().as_hash()))
            .ok_or(paqus::ledger::fork_choice::ForkChoiceError::MissingParent)?;
        if self.consensus.config.difficulty != 0 {
            let expected_difficulty = self.next_difficulty_after_tip(parent.block.height())?;
            if block.difficulty() != expected_difficulty {
                return Err(paqus::consensus::ConsensusError::UnexpectedDifficulty.into());
            }
        }
        self.consensus
            .validate_next_block_with_tip_at(block, &parent.block, now)?;
        Ok(())
    }

    pub fn mine_block(
        &mut self,
        miner_address: Address,
        timestamp: u64,
        max_attempts: u64,
        transaction_limit: usize,
    ) -> Result<MiningResult, NodeError> {
        self.mempool.prune_expired(timestamp);
        let difficulty = self.next_difficulty()?;
        let result = mine_candidate_block(
            &self.mempool,
            &self.ledger,
            &self.consensus,
            miner_address,
            timestamp,
            MiningConfig {
                difficulty,
                max_attempts,
                transaction_limit,
            },
        )?
        .ok_or(NodeError::MiningExhausted)?;

        self.apply_block(result.block.clone())?;
        Ok(result)
    }

    pub fn next_difficulty(&self) -> Result<u32, NodeError> {
        let Some(tip_height) = self.ledger.tip_height() else {
            return Ok(self.consensus.config.difficulty);
        };

        self.next_difficulty_after_tip(tip_height)
    }

    fn next_difficulty_after_tip(&self, tip_height: BlockHeight) -> Result<u32, NodeError> {
        let Some((first_timestamp, last_timestamp, block_count, current_difficulty)) = self
            .storage
            .difficulty_window(tip_height, DIFFICULTY_ADJUSTMENT_INTERVAL)?
        else {
            return Ok(self.consensus.config.difficulty);
        };

        Ok(self.consensus.retarget_difficulty(
            current_difficulty,
            first_timestamp,
            last_timestamp,
            block_count,
        )?)
    }

    pub fn flush_to_storage(&self) -> Result<(), NodeError> {
        self.storage.save_ledger(&self.ledger)?;
        Ok(())
    }

    pub fn tip_height(&self) -> Option<BlockHeight> {
        self.ledger.tip_height()
    }

    pub fn tip_hash(&self) -> Option<BlockHash> {
        self.ledger.tip_hash()
    }

    pub fn balance(&self, address: &Address) -> Option<Balance> {
        self.ledger.balance(address)
    }

    pub fn confirmed_balance(&self, address: &Address) -> Option<Balance> {
        self.ledger.confirmed_balance(address)
    }

    pub fn available_balance(&self, address: &Address) -> Option<Balance> {
        self.available_balance_with_depth(address, crate::runtime::params::FINALITY_DEPTH as u64)
    }

    pub fn available_balance_with_depth(
        &self,
        address: &Address,
        _finality_depth: u64,
    ) -> Option<Balance> {
        let tip_height = self.ledger.tip_height()?;
        self.ledger
            .account(address)
            .map(|account| account.available_balance_at(tip_height))
    }

    pub fn pending_balance(&self, address: &Address) -> PendingBalance {
        let mut pending = PendingBalance::default();
        for transaction in self.mempool.transactions() {
            if transaction.payload.to == *address {
                pending.incoming.0 = pending
                    .incoming
                    .0
                    .saturating_add(transaction.payload.amount.0);
            }
            if transaction.payload.from == *address {
                let total = transaction
                    .payload
                    .amount
                    .0
                    .saturating_add(transaction.payload.fee.0);
                pending.outgoing.0 = pending.outgoing.0.saturating_add(total);
            }
        }
        pending
    }

    pub fn balance_summary(&self, address: &Address) -> Option<BalanceSummary> {
        Some(BalanceSummary {
            confirmed: self.confirmed_balance(address)?,
            available: self.available_balance(address)?,
            pending: self.pending_balance(address),
        })
    }

    pub fn account_view(&self, address: &Address) -> Option<AccountView> {
        let account = self.ledger.account(address)?;
        let tip_height = self.ledger.tip_height()?;
        Some(AccountView {
            balance: account.available_balance_at(tip_height),
            unspendable: account.unspendable_balance_at(tip_height),
            nonce: account.nonce,
        })
    }
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
