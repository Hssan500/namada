//! Namada transaction.

#![doc(html_favicon_url = "https://dev.namada.net/master/favicon.png")]
#![doc(html_logo_url = "https://dev.namada.net/master/rustdoc-logo.png")]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![warn(
    missing_docs,
    rust_2018_idioms,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::arithmetic_side_effects,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr
)]

pub mod action;
pub mod data;
pub mod event;
pub mod proto;
mod types;

use data::TxType;
pub use either;
pub use event::new_tx_event;
pub use namada_core::key::SignableEthMessage;
pub use namada_core::sign::SignatureIndex;
pub use types::{
    standalone_signature, verify_standalone_sig, Authorization, BatchedTx,
    BatchedTxRef, Code, Commitment, CompressedAuthorization, Data, DecodeError,
    Header, IndexedTx, MaspBuilder, Memo, Section, Signed, Signer, Tx,
    TxCommitments, TxError, VerifySigError,
};

const SALT_LENGTH: usize = 8;

mod hex_serde {
    use data_encoding::HEXUPPER;
    use serde::{de, Deserializer, Serializer};

    use super::*;

    pub fn serialize<S>(
        salt: &[u8; SALT_LENGTH],
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert the byte array to a hex string
        let hex_string = HEXUPPER.encode(salt);
        serializer.serialize_str(&hex_string)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<[u8; SALT_LENGTH], D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize a hex string
        let hex_string =
            <String as serde::Deserialize>::deserialize(deserializer)?;
        // Convert the hex string back to a byte array
        let bytes = HEXUPPER
            .decode(hex_string.as_bytes())
            .map_err(de::Error::custom)?;

        if bytes.len() != SALT_LENGTH {
            return Err(de::Error::custom(format!(
                "Invalid length: expected {} bytes, got {}",
                SALT_LENGTH,
                bytes.len()
            )));
        }

        let mut array = [0u8; SALT_LENGTH];
        array.copy_from_slice(&bytes);

        Ok(array)
    }
}

#[cfg(test)]
mod tests {
    use data_encoding::HEXLOWER;
    use prost::Message;

    use super::*;

    #[test]
    fn encoding_round_trip() {
        use proto::Tx;

        let tx = Tx {
            data: "arbitrary data".as_bytes().to_owned(),
        };
        let mut tx_bytes = vec![];
        tx.encode(&mut tx_bytes).unwrap();
        let tx_hex = HEXLOWER.encode(&tx_bytes);
        let tx_from_hex = HEXLOWER.decode(tx_hex.as_ref()).unwrap();
        let tx_from_bytes = Tx::decode(&tx_from_hex[..]).unwrap();
        assert_eq!(tx, tx_from_bytes);
    }
}
