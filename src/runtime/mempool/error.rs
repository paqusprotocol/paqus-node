use paqus::ledger::LedgerError;
use paqus::transaction::TransactionError;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MempoolError {
    DuplicateTransaction,
    FeeTooLow,
    MempoolFull,
    ReplacementFeeTooLow,
    InvalidTransaction(TransactionError),
    InvalidLedgerState(LedgerError),
}

impl fmt::Display for MempoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MempoolError::DuplicateTransaction => {
                f.write_str("transaction already exists in mempool")
            }
            MempoolError::FeeTooLow => f.write_str("transaction fee is below node policy"),
            MempoolError::MempoolFull => f.write_str("mempool transaction limit reached"),
            MempoolError::ReplacementFeeTooLow => {
                f.write_str("replacement transaction fee must be higher")
            }
            MempoolError::InvalidTransaction(error) => write!(f, "invalid transaction: {error}"),
            MempoolError::InvalidLedgerState(error) => {
                write!(f, "transaction does not fit ledger state: {error}")
            }
        }
    }
}

impl Error for MempoolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            MempoolError::DuplicateTransaction => None,
            MempoolError::FeeTooLow => None,
            MempoolError::MempoolFull => None,
            MempoolError::ReplacementFeeTooLow => None,
            MempoolError::InvalidTransaction(error) => Some(error),
            MempoolError::InvalidLedgerState(error) => Some(error),
        }
    }
}

impl From<TransactionError> for MempoolError {
    fn from(error: TransactionError) -> Self {
        MempoolError::InvalidTransaction(error)
    }
}

impl From<LedgerError> for MempoolError {
    fn from(error: LedgerError) -> Self {
        MempoolError::InvalidLedgerState(error)
    }
}
