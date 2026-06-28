#![allow(dead_code, unused_imports)]

pub mod cache;
pub mod mempool;
pub mod miner;
pub mod network;
pub mod node;
pub mod storage;
pub mod wallet;

pub mod params {
    pub use paqus::params::*;

    const MINUTE: u64 = 60;
    const DAY: u64 = 24 * 60 * MINUTE;

    pub const STORAGE_VERSION: u8 = 1;
    pub const MAX_RELAY_TRANSACTION_AGE_SECS: u64 = DAY;
    pub const MAX_RELAY_TRANSACTION_FUTURE_SECS: u64 = paqus::params::BLOCK_TIME as u64;
    pub const LOW_FEE_EXPIRY_SECS: u64 = 30 * MINUTE;
    pub const MEMPOOL_EXPIRY_SECS: u64 = DAY;
    pub const MAX_MEMPOOL_TXS: usize = 1_000;
    pub const MAX_MEMPOOL_BYTES: usize = 10 * 1024 * 1024;
    pub const MAX_NETWORK_MESSAGE_SIZE: usize = 8 * 1024 * 1024;
    pub const BASE_FEE: u32 = 2;
    pub const DEFAULT_TRANSACTION_FEE: u32 = BASE_FEE;
    pub const MIN_RELAY_FEE_FLOOR: u32 = 1;
    pub const DEFAULT_MIN_RELAY_FEE: u32 = MIN_RELAY_FEE_FLOOR;
    pub const DEFAULT_MARKET_FEE: u32 = DEFAULT_TRANSACTION_FEE;
}
