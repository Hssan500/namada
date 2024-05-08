//! Transaction events.

use namada_core::borsh::{BorshDeserialize, BorshSerialize};
use namada_core::event::extend::{
    ComposeEvent, EventAttributeEntry, Height, Log, TxHash,
};
use namada_core::event::{Event, EventToEmit};
use namada_macros::BorshDeserializer;
#[cfg(feature = "migrations")]
use namada_migrations::*;

use super::Tx;
use crate::data::{ResultCode, TxResult};
use crate::TxType;

/// Transaction event.
#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    BorshSerialize,
    BorshDeserialize,
    BorshDeserializer,
)]
pub struct TxEvent(pub Event);

impl From<TxEvent> for Event {
    #[inline]
    fn from(TxEvent(event): TxEvent) -> Self {
        event
    }
}

impl EventToEmit for TxEvent {
    const DOMAIN: &'static str = "tx";
}

pub mod types {
    //! Transaction event types.

    use namada_core::event::EventType;

    use super::TxEvent;

    /// Applied transaction.
    pub const APPLIED: EventType = namada_core::event_type!(TxEvent, "applied");
}

/// Creates a new event with the hash and height of the transaction
/// already filled in.
pub fn new_tx_event(tx: &Tx, height: u64) -> Event {
    let base_event = match tx.header().tx_type {
        TxType::Wrapper(_) | TxType::Protocol(_) => {
            Event::applied_tx().with(TxHash(tx.header_hash()))
        }
        _ => unreachable!(),
    };
    base_event
        .with(Height(height.into()))
        .with(Log(String::new()))
        .into()
}

/// Extend an [`Event`] with result code data.
pub struct Code(pub ResultCode);

impl EventAttributeEntry<'static> for Code {
    type Value = ResultCode;
    type ValueOwned = Self::Value;

    const KEY: &'static str = "code";

    fn into_value(self) -> Self::Value {
        self.0
    }
}

/// Extend an [`Event`] with inner tx data.
pub struct InnerTx<'result>(pub &'result TxResult);

impl<'result> EventAttributeEntry<'result> for InnerTx<'result> {
    type Value = &'result TxResult;
    type ValueOwned = TxResult;

    const KEY: &'static str = "inner_tx";

    fn into_value(self) -> Self::Value {
        self.0
    }
}
