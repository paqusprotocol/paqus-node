use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletError {
    SenderAddressMismatch,
}

impl fmt::Display for WalletError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WalletError::SenderAddressMismatch => {
                f.write_str("transaction sender does not match wallet address")
            }
        }
    }
}

impl Error for WalletError {}
