use crate::runtime::params::{ADDRESS_SIZE, HASH_SIZE, STORAGE_VERSION};
use crate::runtime::storage::error::StorageError;
use borsh::{BorshDeserialize, BorshSerialize};
use paqus::block::Block;
use paqus::ledger::{Ledger, calculate_state_root};
use paqus::state::Account;
use paqus::transaction::SignedTransaction;
use paqus::types::{Address, BlockHash, BlockHeight, Hash, Height, TransactionHash};
use std::collections::BTreeMap;
use std::path::Path;

const BLOCKS_BY_HEIGHT: &str = "blocks_by_height";
const BLOCKS_BY_HASH: &str = "blocks_by_hash";
const ACCOUNTS: &str = "accounts";
const GENESIS_ACCOUNTS: &str = "genesis_accounts";
const STATE_SNAPSHOTS: &str = "state_snapshots";
const TX_INDEX: &str = "tx_index";
const ADDRESS_TX_INDEX: &str = "address_tx_index";
const META: &str = "meta";
const TIP_HEIGHT_KEY: &[u8] = b"tip_height";
const TIP_HASH_KEY: &[u8] = b"tip_hash";
const STORAGE_VERSION_KEY: &[u8] = b"storage_version";

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StateSnapshot {
    pub height: BlockHeight,
    pub block_hash: BlockHash,
    pub state_root: Hash,
    pub accounts: BTreeMap<Address, Account>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransactionLocation {
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub tx_index: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressTransactionLocation {
    pub tx_hash: TransactionHash,
    pub block_height: BlockHeight,
    pub block_hash: BlockHash,
    pub tx_index: u32,
    pub sent: bool,
}

impl StateSnapshot {
    pub fn verify_state_root(&self) -> bool {
        calculate_state_root(&self.accounts) == self.state_root
    }

    pub fn verify_against_block(&self, block: &Block) -> bool {
        block.height() == self.height
            && block.hash() == self.block_hash
            && block.state_root() == self.state_root
            && self.verify_state_root()
    }
}

#[derive(Clone, Debug)]
pub struct Storage {
    db: sled::Db,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let storage = Self {
            db: sled::open(path)?,
        };
        storage.ensure_storage_version()?;
        Ok(storage)
    }

    pub fn temporary() -> Result<Self, StorageError> {
        let storage = Self {
            db: sled::Config::new().temporary(true).open()?,
        };
        storage.ensure_storage_version()?;
        Ok(storage)
    }

    pub fn load_storage_version(&self) -> Result<Option<u8>, StorageError> {
        self.meta()?
            .get(STORAGE_VERSION_KEY)?
            .map(|bytes| decode(&bytes))
            .transpose()
    }

    fn save_storage_version(&self, version: u8) -> Result<(), StorageError> {
        self.meta()?
            .insert(STORAGE_VERSION_KEY, encode(&version)?.as_slice())?;
        Ok(())
    }

    fn ensure_storage_version(&self) -> Result<(), StorageError> {
        match self.load_storage_version()? {
            Some(STORAGE_VERSION) => Ok(()),
            Some(found) => Err(StorageError::UnsupportedStorageVersion {
                expected: STORAGE_VERSION,
                found,
            }),
            None if self.is_empty_database()? => {
                self.save_storage_version(STORAGE_VERSION)?;
                self.flush()
            }
            None => Err(StorageError::MissingStorageVersion),
        }
    }

    fn is_empty_database(&self) -> Result<bool, StorageError> {
        Ok(self.blocks_by_height()?.is_empty()
            && self.blocks_by_hash()?.is_empty()
            && self.accounts()?.is_empty()
            && self.genesis_accounts()?.is_empty()
            && self.state_snapshots()?.is_empty()
            && self.tx_index()?.is_empty()
            && self.address_tx_index()?.is_empty())
    }

    pub fn save_block(&self, block: &Block) -> Result<(), StorageError> {
        let bytes = encode(block)?;
        self.blocks_by_height()?
            .insert(height_key(block.height()), bytes.as_slice())?;
        self.blocks_by_hash()?
            .insert(block.hash().0.as_slice(), bytes.as_slice())?;
        self.index_block_transactions(block)?;
        Ok(())
    }

    fn index_block_transactions(&self, block: &Block) -> Result<(), StorageError> {
        let tx_index = self.tx_index()?;
        let address_tx_index = self.address_tx_index()?;
        let block_hash = block.hash();

        for (index, transaction) in block.transactions.iter().enumerate() {
            let tx_index_u32 = u32::try_from(index)
                .map_err(|_| StorageError::Integrity("transaction index exceeds u32"))?;
            let tx_hash = transaction.hash();
            let location = TransactionLocation {
                block_height: block.height(),
                block_hash,
                tx_index: tx_index_u32,
            };
            tx_index.insert(tx_hash.0.as_slice(), encode(&location)?.as_slice())?;

            let sent_location = AddressTransactionLocation {
                tx_hash,
                block_height: block.height(),
                block_hash,
                tx_index: tx_index_u32,
                sent: true,
            };
            address_tx_index.insert(
                address_tx_key(
                    &transaction.payload.from,
                    block.height(),
                    tx_index_u32,
                    true,
                ),
                encode(&sent_location)?.as_slice(),
            )?;

            if transaction.payload.to != transaction.payload.from {
                let received_location = AddressTransactionLocation {
                    sent: false,
                    ..sent_location
                };
                address_tx_index.insert(
                    address_tx_key(&transaction.payload.to, block.height(), tx_index_u32, false),
                    encode(&received_location)?.as_slice(),
                )?;
            }
        }

        Ok(())
    }

    pub fn load_block_by_height(&self, height: BlockHeight) -> Result<Option<Block>, StorageError> {
        self.blocks_by_height()?
            .get(height_key(height))?
            .map(|bytes| {
                let block: Block = decode(&bytes)?;
                if block.height() != height {
                    return Err(StorageError::Integrity(
                        "stored block height does not match height key",
                    ));
                }
                Ok(block)
            })
            .transpose()
    }

    pub fn load_block_by_hash(&self, hash: &BlockHash) -> Result<Option<Block>, StorageError> {
        self.blocks_by_hash()?
            .get(hash.0.as_slice())?
            .map(|bytes| {
                let block: Block = decode(&bytes)?;
                if block.hash() != *hash {
                    return Err(StorageError::Integrity(
                        "stored block hash does not match hash key",
                    ));
                }
                Ok(block)
            })
            .transpose()
    }

    pub fn load_transaction_location(
        &self,
        hash: &TransactionHash,
    ) -> Result<Option<TransactionLocation>, StorageError> {
        self.tx_index()?
            .get(hash.0.as_slice())?
            .map(|bytes| decode(&bytes))
            .transpose()
    }

    pub fn load_transaction(
        &self,
        hash: &TransactionHash,
    ) -> Result<Option<(TransactionLocation, SignedTransaction)>, StorageError> {
        let Some(location) = self.load_transaction_location(hash)? else {
            return Ok(None);
        };
        let Some(block) = self.load_block_by_height(location.block_height)? else {
            return Err(StorageError::Integrity(
                "indexed transaction block is missing",
            ));
        };
        if block.hash() != location.block_hash {
            return Err(StorageError::Integrity(
                "indexed transaction block hash mismatch",
            ));
        }
        let transaction = block
            .transactions
            .get(location.tx_index as usize)
            .ok_or(StorageError::Integrity(
                "indexed transaction position is missing",
            ))?
            .clone();
        if transaction.hash() != *hash {
            return Err(StorageError::Integrity(
                "indexed transaction hash does not match transaction",
            ));
        }
        Ok(Some((location, transaction)))
    }

    pub fn load_address_transaction_locations(
        &self,
        address: &Address,
    ) -> Result<Vec<AddressTransactionLocation>, StorageError> {
        let prefix = address.0;
        let mut locations = Vec::new();
        for entry in self.address_tx_index()?.scan_prefix(prefix) {
            let (_key, bytes) = entry?;
            locations.push(decode(&bytes)?);
        }
        locations.sort_by_key(|location: &AddressTransactionLocation| {
            (location.block_height, location.tx_index, location.sent)
        });
        Ok(locations)
    }

    pub fn save_account(&self, account: &Account) -> Result<(), StorageError> {
        self.accounts()?
            .insert(account.address.0.as_slice(), encode(account)?.as_slice())?;
        Ok(())
    }

    pub fn load_account(&self, address: &Address) -> Result<Option<Account>, StorageError> {
        self.accounts()?
            .get(address.0.as_slice())?
            .map(|bytes| decode(&bytes))
            .transpose()
    }

    pub fn save_genesis_accounts(
        &self,
        accounts: &BTreeMap<Address, Account>,
    ) -> Result<(), StorageError> {
        self.genesis_accounts()?
            .insert(b"accounts", encode(accounts)?.as_slice())?;
        Ok(())
    }

    pub fn load_genesis_accounts(
        &self,
    ) -> Result<Option<BTreeMap<Address, Account>>, StorageError> {
        self.genesis_accounts()?
            .get(b"accounts")?
            .map(|bytes| decode(&bytes))
            .transpose()
    }

    pub fn save_state_snapshot(&self, ledger: &Ledger) -> Result<(), StorageError> {
        let (Some(height), Some(block_hash)) = (ledger.tip_height(), ledger.tip_hash()) else {
            return Ok(());
        };
        let snapshot = StateSnapshot {
            height,
            block_hash,
            state_root: ledger.state_root().into(),
            accounts: ledger.accounts.clone(),
        };
        self.state_snapshots()?
            .insert(height_key(height), encode(&snapshot)?.as_slice())?;
        Ok(())
    }

    pub fn load_state_snapshot(
        &self,
        height: BlockHeight,
    ) -> Result<Option<StateSnapshot>, StorageError> {
        self.state_snapshots()?
            .get(height_key(height))?
            .map(|bytes| {
                let snapshot: StateSnapshot = decode(&bytes)?;
                if snapshot.height != height {
                    return Err(StorageError::Integrity(
                        "stored state snapshot height does not match height key",
                    ));
                }
                if !snapshot.verify_state_root() {
                    return Err(StorageError::Integrity(
                        "stored state snapshot root does not match accounts",
                    ));
                }
                Ok(snapshot)
            })
            .transpose()
    }

    pub fn save_tip(&self, height: BlockHeight, hash: &BlockHash) -> Result<(), StorageError> {
        let meta = self.meta()?;
        meta.insert(TIP_HEIGHT_KEY, encode(&height)?.as_slice())?;
        meta.insert(TIP_HASH_KEY, hash.0.as_slice())?;
        Ok(())
    }

    pub fn load_tip(&self) -> Result<Option<(BlockHeight, BlockHash)>, StorageError> {
        let meta = self.meta()?;
        let Some(height_bytes) = meta.get(TIP_HEIGHT_KEY)? else {
            return Ok(None);
        };
        let Some(hash_bytes) = meta.get(TIP_HASH_KEY)? else {
            return Ok(None);
        };

        let height = decode(&height_bytes)?;
        let hash = Hash(
            hash_bytes
                .as_ref()
                .try_into()
                .map_err(|_| invalid_data("stored tip hash has invalid length"))?,
        );
        Ok(Some((height, hash.into())))
    }

    pub fn save_ledger(&self, ledger: &Ledger) -> Result<(), StorageError> {
        for account in ledger.accounts.values() {
            self.save_account(account)?;
        }

        for block in ledger.chain.blocks.values() {
            self.save_block(block)?;
        }

        if let (Some(height), Some(hash)) = (ledger.tip_height(), ledger.tip_hash()) {
            self.save_tip(height, &hash)?;
            self.save_state_snapshot(ledger)?;
            if height.0 == 0 {
                self.save_genesis_accounts(&ledger.accounts)?;
            }
        }

        self.flush()?;
        Ok(())
    }

    pub fn load_ledger(&self) -> Result<Ledger, StorageError> {
        self.ensure_storage_version()?;
        self.validate_chain_integrity()?;

        let mut ledger = Ledger::new();
        for account in self.accounts()?.iter() {
            let (_key, bytes) = account?;
            let account: Account = decode(&bytes)?;
            if account.address.0.as_slice() != _key.as_ref() {
                return Err(StorageError::Integrity(
                    "stored account address does not match account key",
                ));
            }
            ledger.accounts.insert(account.address, account);
        }

        if let Some((tip_height, _tip_hash)) = self.load_tip()? {
            for height in 0..=tip_height.0 {
                let block = self
                    .load_block_by_height(Height(height))?
                    .ok_or(StorageError::Integrity("stored chain block is missing"))?;
                ledger
                    .chain
                    .insert_block(block)
                    .map_err(|_| StorageError::Integrity("stored chain block is invalid"))?;
            }
        }

        Ok(ledger)
    }

    pub fn difficulty_window(
        &self,
        tip_height: BlockHeight,
        window: u64,
    ) -> Result<Option<(u64, u64, u64, u32)>, StorageError> {
        if window == 0 || tip_height.0 < window {
            return Ok(None);
        }

        let Some(tip) = self.load_block_by_height(tip_height)? else {
            return Ok(None);
        };
        let first_height = Height(tip_height.0 - window);
        let Some(first) = self.load_block_by_height(first_height)? else {
            return Ok(None);
        };
        let block_count = tip_height.0.saturating_sub(first_height.0);

        Ok(Some((
            first.timestamp(),
            tip.timestamp(),
            block_count,
            tip.difficulty(),
        )))
    }

    pub fn validate_chain_integrity(&self) -> Result<(), StorageError> {
        let Some((tip_height, tip_hash)) = self.load_tip()? else {
            return Ok(());
        };

        let tip_block = self
            .load_block_by_height(tip_height)?
            .ok_or(StorageError::Integrity(
                "stored tip height block is missing",
            ))?;
        if tip_block.hash() != tip_hash {
            return Err(StorageError::Integrity(
                "stored tip hash does not match tip height block",
            ));
        }

        let mut expected_hash = tip_hash;
        for height in (0..=tip_height.0).rev() {
            let block_height = Height(height);
            let block = self
                .load_block_by_height(block_height)?
                .ok_or(StorageError::Integrity("stored chain block is missing"))?;

            if block.hash() != expected_hash {
                return Err(StorageError::Integrity(
                    "stored chain block hash does not match expected hash",
                ));
            }

            if height == 0 {
                if block.previous_hash() != Hash([0; HASH_SIZE]) {
                    return Err(StorageError::Integrity(
                        "stored genesis block previous hash is not zero",
                    ));
                }
            } else {
                let previous = self.load_block_by_height(Height(height - 1))?.ok_or(
                    StorageError::Integrity("stored previous chain block is missing"),
                )?;
                if block.previous_hash() != previous.hash() {
                    return Err(StorageError::Integrity(
                        "stored chain block previous hash is broken",
                    ));
                }
                expected_hash = previous.hash();
            }
        }

        Ok(())
    }

    pub fn flush(&self) -> Result<(), StorageError> {
        self.db.flush()?;
        Ok(())
    }

    fn blocks_by_height(&self) -> Result<sled::Tree, StorageError> {
        Ok(self.db.open_tree(BLOCKS_BY_HEIGHT)?)
    }

    fn blocks_by_hash(&self) -> Result<sled::Tree, StorageError> {
        Ok(self.db.open_tree(BLOCKS_BY_HASH)?)
    }

    #[cfg(test)]
    pub(crate) fn test_blocks_by_height(&self) -> Result<sled::Tree, StorageError> {
        self.blocks_by_height()
    }

    #[cfg(test)]
    pub(crate) fn test_blocks_by_hash(&self) -> Result<sled::Tree, StorageError> {
        self.blocks_by_hash()
    }

    fn accounts(&self) -> Result<sled::Tree, StorageError> {
        Ok(self.db.open_tree(ACCOUNTS)?)
    }

    fn genesis_accounts(&self) -> Result<sled::Tree, StorageError> {
        Ok(self.db.open_tree(GENESIS_ACCOUNTS)?)
    }

    fn state_snapshots(&self) -> Result<sled::Tree, StorageError> {
        Ok(self.db.open_tree(STATE_SNAPSHOTS)?)
    }

    #[cfg(test)]
    pub(crate) fn test_state_snapshots(&self) -> Result<sled::Tree, StorageError> {
        self.state_snapshots()
    }

    fn meta(&self) -> Result<sled::Tree, StorageError> {
        Ok(self.db.open_tree(META)?)
    }

    fn tx_index(&self) -> Result<sled::Tree, StorageError> {
        Ok(self.db.open_tree(TX_INDEX)?)
    }

    fn address_tx_index(&self) -> Result<sled::Tree, StorageError> {
        Ok(self.db.open_tree(ADDRESS_TX_INDEX)?)
    }

    #[cfg(test)]
    pub(crate) fn test_meta(&self) -> Result<sled::Tree, StorageError> {
        self.meta()
    }
}

fn height_key(height: BlockHeight) -> [u8; 8] {
    height.0.to_be_bytes()
}

fn address_tx_key(address: &Address, height: BlockHeight, tx_index: u32, sent: bool) -> Vec<u8> {
    let mut key = Vec::with_capacity(ADDRESS_SIZE + 8 + 4 + 1);
    key.extend_from_slice(&address.0);
    key.extend_from_slice(&height.0.to_be_bytes());
    key.extend_from_slice(&tx_index.to_be_bytes());
    key.push(u8::from(sent));
    key
}

fn encode<T: BorshSerialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    Ok(borsh::to_vec(value)?)
}

fn decode<T: BorshDeserialize>(bytes: &[u8]) -> Result<T, StorageError> {
    Ok(T::try_from_slice(bytes)?)
}

fn invalid_data(message: &'static str) -> StorageError {
    StorageError::Serialization(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}
