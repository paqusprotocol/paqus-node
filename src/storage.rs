// src/storage.rs

use borsh::{BorshDeserialize, BorshSerialize};
use lmdb::{Database, DatabaseFlags, Environment, EnvironmentFlags, Transaction, WriteFlags};

use crate::{
    Account, Address, Block, BlockHash, Hash, Height, SignedTransaction, TransactionHash,
};

pub struct ChainStorage {
    env: Environment,
    blocks_by_height: Database,
    blocks_by_hash: Database,
    txs: Database,
    tx_index: Database,
    accounts: Database,
    meta: Database,
}

impl ChainStorage {
    pub fn open(path: &str) -> Result<Self, String> {
        let env = Environment::new()
            .set_max_dbs(10)
            .set_map_size(1024 * 1024 * 128)
            .set_flags(EnvironmentFlags::NO_SUB_DIR)
            .open(std::path::Path::new(path))
            .map_err(|e| format!("failed to open lmdb environment: {e}"))?;

        let blocks_by_height = env
            .create_db(Some("blocks_by_height"), DatabaseFlags::empty())
            .map_err(|e| format!("failed to open blocks_by_height db: {e}"))?;
        let blocks_by_hash = env
            .create_db(Some("blocks_by_hash"), DatabaseFlags::empty())
            .map_err(|e| format!("failed to open blocks_by_hash db: {e}"))?;
        let txs = env
            .create_db(Some("txs"), DatabaseFlags::empty())
            .map_err(|e| format!("failed to open txs db: {e}"))?;
        let tx_index = env
            .create_db(Some("tx_index"), DatabaseFlags::empty())
            .map_err(|e| format!("failed to open tx_index db: {e}"))?;
        let accounts = env
            .create_db(Some("accounts"), DatabaseFlags::empty())
            .map_err(|e| format!("failed to open accounts db: {e}"))?;
        let meta = env
            .create_db(Some("meta"), DatabaseFlags::empty())
            .map_err(|e| format!("failed to open meta db: {e}"))?;

        Ok(Self {
            env,
            blocks_by_height,
            blocks_by_hash,
            txs,
            tx_index,
            accounts,
            meta,
        })
    }

    pub fn save_block(&self, block: &Block) -> Result<(), String> {
        let height = block.height();
        let hash = block.hash();
        let block_bytes =
            borsh::to_vec(block).map_err(|e| format!("failed to serialize block: {e}"))?;
        let mut txn = self
            .env
            .begin_rw_txn()
            .map_err(|e| format!("failed to begin write transaction: {e}"))?;

        txn.put(
            self.blocks_by_height,
            &height_key(height),
            &block_bytes,
            WriteFlags::empty(),
        )
        .map_err(|e| format!("failed to save block by height: {e}"))?;
        txn.put(self.blocks_by_hash, &hash.0, &block_bytes, WriteFlags::empty())
            .map_err(|e| format!("failed to save block by hash: {e}"))?;
        txn.put(
            self.meta,
            b"tip_height",
            &height.0.to_be_bytes(),
            WriteFlags::empty(),
        )
        .map_err(|e| format!("failed to save tip height: {e}"))?;
        txn.put(self.meta, b"tip_hash", &hash.0, WriteFlags::empty())
            .map_err(|e| format!("failed to save tip hash: {e}"))?;

        for tx in &block.transactions {
            self.save_transaction_in_txn(&mut txn, tx, Some(height), Some(hash))?;
        }

        txn.commit()
            .map_err(|e| format!("failed to commit block: {e}"))?;

        Ok(())
    }

    pub fn load_block_by_height(&self, height: Height) -> Result<Option<Block>, String> {
        let txn = self
            .env
            .begin_ro_txn()
            .map_err(|e| format!("failed to begin read transaction: {e}"))?;
        let Some(bytes) = txn
            .get(self.blocks_by_height, &height_key(height))
            .ok()
        else {
            return Ok(None);
        };

        Block::try_from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("invalid block bytes: {e}"))
    }

    pub fn load_block_by_hash(&self, hash: &BlockHash) -> Result<Option<Block>, String> {
        let txn = self
            .env
            .begin_ro_txn()
            .map_err(|e| format!("failed to begin read transaction: {e}"))?;
        let Some(bytes) = txn.get(self.blocks_by_hash, &hash.0).ok() else {
            return Ok(None);
        };

        Block::try_from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("invalid block bytes: {e}"))
    }

    pub fn save_transaction(
        &self,
        tx: &SignedTransaction,
        height: Option<Height>,
        block_hash: Option<BlockHash>,
    ) -> Result<(), String> {
        let mut txn = self
            .env
            .begin_rw_txn()
            .map_err(|e| format!("failed to begin write transaction: {e}"))?;
        self.save_transaction_in_txn(&mut txn, tx, height, block_hash)?;
        txn.commit()
            .map_err(|e| format!("failed to commit transaction: {e}"))?;
        Ok(())
    }

    fn save_transaction_in_txn(
        &self,
        txn: &mut lmdb::RwTransaction,
        tx: &SignedTransaction,
        height: Option<Height>,
        block_hash: Option<BlockHash>,
    ) -> Result<(), String> {
        let tx_hash = tx.hash();

        let tx_bytes =
            borsh::to_vec(tx).map_err(|e| format!("failed to serialize tx: {e}"))?;
        txn.put(self.txs, &tx_hash.0, &tx_bytes, WriteFlags::empty())
            .map_err(|e| format!("failed to save tx: {e}"))?;

        if let (Some(height), Some(block_hash)) = (height, block_hash) {
            let index = TxLocation {
                height,
                block_hash,
            };

            let index_bytes =
                borsh::to_vec(&index).map_err(|e| format!("failed to serialize tx index: {e}"))?;
            txn.put(self.tx_index, &tx_hash.0, &index_bytes, WriteFlags::empty())
                .map_err(|e| format!("failed to save tx index: {e}"))?;
        }

        Ok(())
    }

    pub fn load_transaction(
        &self,
        hash: &TransactionHash,
    ) -> Result<Option<SignedTransaction>, String> {
        let txn = self
            .env
            .begin_ro_txn()
            .map_err(|e| format!("failed to begin read transaction: {e}"))?;
        let Some(bytes) = txn.get(self.txs, &hash.0).ok() else {
            return Ok(None);
        };

        SignedTransaction::try_from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("invalid tx bytes: {e}"))
    }

    pub fn save_account(&self, account: &Account) -> Result<(), String> {
        let mut txn = self
            .env
            .begin_rw_txn()
            .map_err(|e| format!("failed to begin write transaction: {e}"))?;
        let bytes =
            borsh::to_vec(account).map_err(|e| format!("failed to serialize account: {e}"))?;
        txn.put(self.accounts, &account.address.0, &bytes, WriteFlags::empty())
            .map_err(|e| format!("failed to save account: {e}"))?;
        txn.commit()
            .map_err(|e| format!("failed to commit account: {e}"))?;

        Ok(())
    }

    pub fn load_account(&self, address: &Address) -> Result<Option<Account>, String> {
        let txn = self
            .env
            .begin_ro_txn()
            .map_err(|e| format!("failed to begin read transaction: {e}"))?;
        let Some(bytes) = txn.get(self.accounts, &address.0).ok() else {
            return Ok(None);
        };

        Account::try_from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("invalid account bytes: {e}"))
    }

    pub fn flush(&self) -> Result<(), String> {
        self.env
            .sync(false)
            .map_err(|e| format!("failed to sync lmdb env: {e}"))?;
        Ok(())
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct TxLocation {
    pub height: Height,
    pub block_hash: BlockHash,
}

fn height_key(height: Height) -> [u8; 8] {
    height.0.to_be_bytes()
}
