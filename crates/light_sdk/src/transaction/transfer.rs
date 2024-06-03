use namada_sdk::address::Address;
use namada_sdk::hash::Hash;
use namada_sdk::key::common;
use namada_sdk::token::DenominatedAmount;
use namada_sdk::tx::data::GasLimit;
use namada_sdk::tx::{
    Authorization, Tx, TxError, TX_SHIELDED_TRANSFER_WASM,
    TX_SHIELDING_TRANSFER_WASM, TX_TRANSPARENT_TRANSFER_WASM,
    TX_UNSHIELDING_TRANSFER_WASM,
};

use super::{attach_fee, attach_fee_signature, GlobalArgs};
use crate::transaction;

/// A transfer transaction
#[derive(Debug, Clone)]
pub struct Transfer(Tx);

impl Transfer {
    /// Build a transparent transfer transaction from the given parameters
    pub fn transparent(
        source: Address,
        target: Address,
        token: Address,
        amount: DenominatedAmount,
        args: GlobalArgs,
    ) -> Self {
        let data = namada_sdk::token::TransparentTransfer {
            source,
            target,
            token,
            amount,
        };

        Self(transaction::build_tx(
            args,
            data,
            TX_TRANSPARENT_TRANSFER_WASM.to_string(),
        ))
    }

    /// Build a shielded transfer transaction from the given parameters
    pub fn shielded(shielded_section_hash: Hash, args: GlobalArgs) -> Self {
        let data = namada_sdk::token::ShieldedTransfer {
            section_hash: shielded_section_hash,
        };

        Self(transaction::build_tx(
            args,
            data,
            TX_SHIELDED_TRANSFER_WASM.to_string(),
        ))
    }

    /// Build a shielding transfer transaction from the given parameters
    pub fn shielding(
        source: Address,
        token: Address,
        amount: DenominatedAmount,
        shielded_section_hash: Hash,
        args: GlobalArgs,
    ) -> Self {
        let data = namada_sdk::token::ShieldingTransfer {
            source,
            token,
            amount,
            shielded_section_hash,
        };

        Self(transaction::build_tx(
            args,
            data,
            TX_SHIELDING_TRANSFER_WASM.to_string(),
        ))
    }

    /// Build an unshielding transfer transaction from the given parameters
    pub fn unshielding(
        target: Address,
        token: Address,
        amount: DenominatedAmount,
        shielded_section_hash: Hash,
        args: GlobalArgs,
    ) -> Self {
        let data = namada_sdk::token::UnshieldingTransfer {
            target,
            token,
            amount,
            shielded_section_hash,
        };

        Self(transaction::build_tx(
            args,
            data,
            TX_UNSHIELDING_TRANSFER_WASM.to_string(),
        ))
    }

    /// Get the bytes to sign for the given transaction
    pub fn get_sign_bytes(&self) -> Vec<Hash> {
        transaction::get_sign_bytes(&self.0)
    }

    /// Attach the provided signatures to the tx
    pub fn attach_signatures(
        self,
        signer: common::PublicKey,
        signature: common::Signature,
    ) -> Self {
        Self(transaction::attach_raw_signatures(
            self.0, signer, signature,
        ))
    }

    /// Attach the fee data to the tx
    pub fn attach_fee(
        self,
        fee: DenominatedAmount,
        token: Address,
        fee_payer: common::PublicKey,
        gas_limit: GasLimit,
    ) -> Self {
        Self(attach_fee(self.0, fee, token, fee_payer, gas_limit))
    }

    /// Get the bytes of the fee data to sign
    pub fn get_fee_sig_bytes(&self) -> Hash {
        transaction::get_wrapper_sign_bytes(&self.0)
    }

    /// Attach a signature of the fee to the tx
    pub fn attach_fee_signature(
        self,
        signer: common::PublicKey,
        signature: common::Signature,
    ) -> Self {
        Self(attach_fee_signature(self.0, signer, signature))
    }

    /// Generates the protobuf encoding of this transaction
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_bytes()
    }

    /// Gets the inner transaction without the domain wrapper
    pub fn payload(self) -> Tx {
        self.0
    }

    /// Validate this wrapper transaction
    pub fn validate_tx(&self) -> Result<Option<&Authorization>, TxError> {
        self.0.validate_tx()
    }
}
