use paqus::block::Block;
use paqus::ledger::Ledger;
use paqus::state::Account;
use paqus::types::{Address, BlockHash, BlockHeight};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoreCache {
    accounts: BTreeMap<Address, Account>,
    blocks_by_height: BTreeMap<BlockHeight, Block>,
    blocks_by_hash: BTreeMap<BlockHash, Block>,
    tip_height: Option<BlockHeight>,
    tip_hash: Option<BlockHash>,
}

impl CoreCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_ledger(ledger: &Ledger) -> Self {
        let mut cache = Self::new();

        for account in ledger.accounts.values() {
            cache.insert_account(account.clone());
        }

        for block in ledger.chain.blocks.values() {
            cache.insert_block(block.clone());
        }

        cache.tip_height = ledger.tip_height();
        cache.tip_hash = ledger.tip_hash();
        cache
    }

    pub fn insert_account(&mut self, account: Account) {
        self.accounts.insert(account.address, account);
    }

    pub fn account(&self, address: &Address) -> Option<&Account> {
        self.accounts.get(address)
    }

    pub fn insert_block(&mut self, block: Block) {
        let height = block.height();
        let hash = block.hash();

        self.blocks_by_height.insert(height, block.clone());
        self.blocks_by_hash.insert(hash, block);
        self.tip_height = Some(height);
        self.tip_hash = Some(hash);
    }

    pub fn block_by_height(&self, height: &BlockHeight) -> Option<&Block> {
        self.blocks_by_height.get(height)
    }

    pub fn block_by_hash(&self, hash: &BlockHash) -> Option<&Block> {
        self.blocks_by_hash.get(hash)
    }

    pub fn tip_height(&self) -> Option<BlockHeight> {
        self.tip_height
    }

    pub fn tip_hash(&self) -> Option<BlockHash> {
        self.tip_hash
    }

    pub fn clear(&mut self) {
        self.accounts.clear();
        self.blocks_by_height.clear();
        self.blocks_by_hash.clear();
        self.tip_height = None;
        self.tip_hash = None;
    }
}

#[cfg(test)]
mod test {
    use super::CoreCache;
    use paqus::block::Block;
    use paqus::ledger::Ledger;
    use paqus::state::Account;
    use paqus::types::{Address, Amount, Hash, Height, Nonce};

    fn address(byte: u8) -> Address {
        Address([byte; 20])
    }

    #[test]
    fn caches_accounts_and_blocks() {
        let mut cache = CoreCache::new();
        let account = Account::new(address(1), Amount(100));
        let block = Block::new(
            Height(0),
            Hash([0; 64]),
            address(9),
            1_700_000_000,
            Nonce(0),
            vec![],
        );
        let block_hash = block.hash();

        cache.insert_account(account.clone());
        cache.insert_block(block.clone());

        assert_eq!(cache.account(&address(1)), Some(&account));
        assert_eq!(cache.block_by_height(&Height(0)), Some(&block));
        assert_eq!(cache.block_by_hash(&block_hash), Some(&block));
        assert_eq!(cache.tip_height(), Some(Height(0)));
        assert_eq!(cache.tip_hash(), Some(block_hash));
    }

    #[test]
    fn builds_from_ledger_snapshot() {
        let mut ledger = Ledger::new();
        let block = Block::new(
            Height(0),
            Hash([0; 64]),
            address(9),
            1_700_000_000,
            Nonce(0),
            vec![],
        );
        let block_hash = block.hash();

        ledger.create_account(address(1), Amount(100)).unwrap();
        ledger.chain.insert_block(block).unwrap();

        let cache = CoreCache::from_ledger(&ledger);

        assert_eq!(cache.account(&address(1)).unwrap().balance, Amount(100));
        assert_eq!(cache.tip_height(), Some(Height(0)));
        assert_eq!(cache.tip_hash(), Some(block_hash));
    }
}
