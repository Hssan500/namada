//! Provides functionality for managing validator keys
use thiserror::Error;
use crate::types::key::{common, SchemeType};
use serde::{Deserialize, Serialize};
use crate::ledger::wallet;
use crate::ledger::wallet::{store, StoredKeypair};

/// Ways in which wallet store operations can fail
#[derive(Error, Debug)]
pub enum ReadError {
    /// Failed decoding the wallet store
    #[error("Failed decoding the wallet store: {0}")]
    Decode(toml::de::Error),
    /// Failed to read the wallet store
    #[error("Failed to read the wallet store from {0}: {1}")]
    ReadWallet(String, String),
    /// Failed to write the wallet store
    #[error("Failed to write the wallet store: {0}")]
    StoreNewWallet(String),
    /// Failed to decode a key
    #[error("Failed to decode a key: {0}")]
    Decryption(wallet::keys::DecryptionError),
}

/// Validator pre-genesis wallet includes all the required keys for genesis
/// setup and a cache of decrypted keys.
pub struct ValidatorWallet {
    /// The wallet store that can be written/read to/from TOML
    pub store: ValidatorStore,
    /// Cryptographic keypair for validator account key
    pub account_key: common::SecretKey,
    /// Cryptographic keypair for consensus key
    pub consensus_key: common::SecretKey,
    /// Cryptographic keypair for Tendermint node key
    pub tendermint_node_key: common::SecretKey,
}

/// Validator pre-genesis wallet store includes all the required keys for
/// genesis setup.
#[derive(Serialize, Deserialize, Debug)]
pub struct ValidatorStore {
    /// Cryptographic keypair for validator account key
    pub account_key: wallet::StoredKeypair<common::SecretKey>,
    /// Cryptographic keypair for consensus key
    pub consensus_key: wallet::StoredKeypair<common::SecretKey>,
    /// Cryptographic keypair for Tendermint node key
    pub tendermint_node_key: wallet::StoredKeypair<common::SecretKey>,
    /// Special validator keys
    pub validator_keys: wallet::ValidatorKeys,
}

impl ValidatorStore {
    /// Decode from TOML string bytes
    pub fn decode(data: Vec<u8>) -> Result<Self, toml::de::Error> {
        toml::from_slice(&data)
    }

    /// Encode in TOML string bytes
    pub fn encode(&self) -> Vec<u8> {
        toml::to_vec(self).expect(
            "Serializing of validator pre-genesis wallet shouldn't fail",
        )
    }
}

/// Generate a key and then encrypt it
pub fn gen_key_to_store(
    scheme: SchemeType,
    password: &Option<String>,
) -> (StoredKeypair<common::SecretKey>, common::SecretKey) {
    let sk = store::gen_sk(scheme);
    StoredKeypair::new(sk, password.clone())
}

impl From<wallet::keys::DecryptionError> for ReadError {
    fn from(err: wallet::keys::DecryptionError) -> Self {
        ReadError::Decryption(err)
    }
}

