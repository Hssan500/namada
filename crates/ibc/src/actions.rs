//! Implementation of `IbcActions` with the protocol storage

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use namada_core::address::{Address, InternalAddress};
use namada_core::borsh::BorshSerializeExt;
use namada_core::ibc::apps::transfer::types::msgs::transfer::MsgTransfer as IbcMsgTransfer;
use namada_core::ibc::apps::transfer::types::packet::PacketData;
use namada_core::ibc::apps::transfer::types::PrefixedCoin;
use namada_core::ibc::core::channel::types::timeout::TimeoutHeight;
use namada_core::ibc::MsgTransfer;
use namada_core::tendermint::Time as TmTime;
use namada_core::token::Amount;
use namada_events::EventTypeBuilder;
use namada_governance::storage::proposal::PGFIbcTarget;
use namada_parameters::read_epoch_duration_parameter;
use namada_state::{
    DBIter, Epochs, ResultExt, State, StateRead, StorageError, StorageHasher,
    StorageRead, StorageResult, StorageWrite, TxHostEnvState, WlState, DB,
};
use namada_token as token;
use token::DenominatedAmount;

use crate::event::IbcEvent;
use crate::{IbcActions, IbcCommonContext, IbcStorageContext};

/// IBC protocol context
#[derive(Debug)]
pub struct IbcProtocolContext<'a, S>
where
    S: State,
{
    state: &'a mut S,
}

impl<S> StorageRead for IbcProtocolContext<'_, S>
where
    S: State,
{
    type PrefixIter<'iter> = <S as StorageRead>::PrefixIter<'iter> where Self: 'iter;

    fn read_bytes(
        &self,
        key: &namada_storage::Key,
    ) -> StorageResult<Option<Vec<u8>>> {
        self.state.read_bytes(key)
    }

    fn has_key(&self, key: &namada_storage::Key) -> StorageResult<bool> {
        self.state.has_key(key)
    }

    fn iter_prefix<'iter>(
        &'iter self,
        prefix: &namada_storage::Key,
    ) -> StorageResult<Self::PrefixIter<'iter>> {
        self.state.iter_prefix(prefix)
    }

    fn iter_next<'iter>(
        &'iter self,
        iter: &mut Self::PrefixIter<'iter>,
    ) -> StorageResult<Option<(String, Vec<u8>)>> {
        self.state.iter_next(iter)
    }

    fn get_chain_id(&self) -> StorageResult<String> {
        self.state.get_chain_id()
    }

    fn get_block_height(&self) -> StorageResult<namada_storage::BlockHeight> {
        self.state.get_block_height()
    }

    fn get_block_header(
        &self,
        height: namada_storage::BlockHeight,
    ) -> StorageResult<Option<namada_storage::Header>> {
        StorageRead::get_block_header(self.state, height)
    }

    fn get_block_epoch(&self) -> StorageResult<namada_storage::Epoch> {
        self.state.get_block_epoch()
    }

    fn get_pred_epochs(&self) -> StorageResult<Epochs> {
        self.state.get_pred_epochs()
    }

    fn get_tx_index(&self) -> StorageResult<namada_storage::TxIndex> {
        self.state.get_tx_index()
    }

    fn get_native_token(&self) -> StorageResult<Address> {
        self.state.get_native_token()
    }
}

impl<S> StorageWrite for IbcProtocolContext<'_, S>
where
    S: State,
{
    fn write_bytes(
        &mut self,
        key: &namada_storage::Key,
        val: impl AsRef<[u8]>,
    ) -> StorageResult<()> {
        self.state.write_bytes(key, val)
    }

    fn delete(&mut self, key: &namada_storage::Key) -> StorageResult<()> {
        self.state.delete(key)
    }
}

impl<D, H> IbcStorageContext for TxHostEnvState<'_, D, H>
where
    D: 'static + DB + for<'iter> DBIter<'iter>,
    H: 'static + StorageHasher,
{
    fn emit_ibc_event(&mut self, event: IbcEvent) -> Result<(), StorageError> {
        let gas = self.write_log_mut().emit_event(event);
        self.charge_gas(gas).into_storage_result()?;
        Ok(())
    }

    fn get_ibc_events(
        &self,
        event_type: impl AsRef<str>,
    ) -> Result<Vec<IbcEvent>, StorageError> {
        let event_type = EventTypeBuilder::new_of::<IbcEvent>()
            .with_segment(event_type)
            .build();

        Ok(self
            .write_log()
            .lookup_events_with_prefix(&event_type)
            .filter_map(|event| IbcEvent::try_from(event).ok())
            .collect())
    }

    fn transfer_token(
        &mut self,
        src: &Address,
        dest: &Address,
        token: &Address,
        amount: Amount,
    ) -> Result<(), StorageError> {
        token::transfer(self, token, src, dest, amount)
    }

    fn handle_masp_tx(
        &mut self,
        shielded: &masp_primitives::transaction::Transaction,
    ) -> Result<(), StorageError> {
        namada_token::utils::handle_masp_tx(self, shielded)?;
        namada_token::utils::update_note_commitment_tree(self, shielded)
    }

    fn mint_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: Amount,
    ) -> Result<(), StorageError> {
        token::credit_tokens(self, token, target, amount)?;
        let minter_key = token::storage_key::minter_key(token);
        self.write(&minter_key, Address::Internal(InternalAddress::Ibc))
    }

    fn burn_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: Amount,
    ) -> Result<(), StorageError> {
        token::burn_tokens(self, token, target, amount)
    }

    fn log_string(&self, message: String) {
        tracing::trace!(message);
    }
}

impl<D, H> IbcCommonContext for TxHostEnvState<'_, D, H>
where
    D: 'static + DB + for<'iter> DBIter<'iter>,
    H: 'static + StorageHasher,
{
}

impl<S> IbcStorageContext for IbcProtocolContext<'_, S>
where
    S: State,
{
    fn emit_ibc_event(&mut self, event: IbcEvent) -> Result<(), StorageError> {
        self.state.write_log_mut().emit_event(event);
        Ok(())
    }

    /// Get IBC events
    fn get_ibc_events(
        &self,
        event_type: impl AsRef<str>,
    ) -> Result<Vec<IbcEvent>, StorageError> {
        let event_type = EventTypeBuilder::new_of::<IbcEvent>()
            .with_segment(event_type)
            .build();

        Ok(self
            .state
            .write_log()
            .lookup_events_with_prefix(&event_type)
            .filter_map(|event| IbcEvent::try_from(event).ok())
            .collect())
    }

    /// Transfer token
    fn transfer_token(
        &mut self,
        src: &Address,
        dest: &Address,
        token: &Address,
        amount: Amount,
    ) -> Result<(), StorageError> {
        token::transfer(self.state, token, src, dest, amount)
    }

    /// Handle masp tx
    fn handle_masp_tx(
        &mut self,
        _shielded: &masp_primitives::transaction::Transaction,
    ) -> Result<(), StorageError> {
        unimplemented!("No MASP transfer in an IBC protocol transaction")
    }

    /// Mint token
    fn mint_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: Amount,
    ) -> Result<(), StorageError> {
        token::credit_tokens(self.state, token, target, amount)?;
        let minter_key = token::storage_key::minter_key(token);
        self.state
            .write(&minter_key, Address::Internal(InternalAddress::Ibc))
    }

    /// Burn token
    fn burn_token(
        &mut self,
        target: &Address,
        token: &Address,
        amount: Amount,
    ) -> Result<(), StorageError> {
        token::burn_tokens(self.state, token, target, amount)
    }

    fn log_string(&self, message: String) {
        tracing::trace!(message);
    }
}

impl<S> IbcCommonContext for IbcProtocolContext<'_, S> where S: State {}

/// Transfer tokens over IBC
pub fn transfer_over_ibc<D, H>(
    state: &mut WlState<D, H>,
    token: &Address,
    source: &Address,
    target: &PGFIbcTarget,
) -> StorageResult<()>
where
    D: DB + for<'iter> DBIter<'iter> + 'static,
    H: StorageHasher + 'static,
{
    let denom = token::read_denom(state, token)?.ok_or_else(|| {
        StorageError::new_alloc(format!("No denomination for {token}"))
    })?;
    let amount = DenominatedAmount::new(target.amount, denom).canonical();
    if amount.denom().0 != 0 {
        return Err(StorageError::new_alloc(format!(
            "The amount for the IBC transfer should be an integer: {amount}"
        )));
    }
    let token = PrefixedCoin {
        denom: token.to_string().parse().expect("invalid token"),
        amount: amount.amount().into(),
    };
    let packet_data = PacketData {
        token,
        sender: source.to_string().into(),
        receiver: target.target.clone().into(),
        memo: String::default().into(),
    };
    let timeout_timestamp = state
        .in_mem()
        .header
        .as_ref()
        .expect("The header should exist")
        .time
        + read_epoch_duration_parameter(state)?.min_duration;
    let timeout_timestamp =
        TmTime::try_from(timeout_timestamp).into_storage_result()?;
    let message = IbcMsgTransfer {
        port_id_on_a: target.port_id.clone(),
        chan_id_on_a: target.channel_id.clone(),
        packet_data,
        timeout_height_on_b: TimeoutHeight::Never,
        timeout_timestamp_on_b: timeout_timestamp.into(),
    };
    let data = MsgTransfer {
        message,
        transfer: None,
    }
    .serialize_to_vec();

    let ctx = IbcProtocolContext { state };

    // Use an empty verifiers set placeholder for validation, this is only
    // needed in txs and not protocol
    let verifiers = Rc::new(RefCell::new(BTreeSet::<Address>::new()));
    let mut actions = IbcActions::new(Rc::new(RefCell::new(ctx)), verifiers);
    actions.execute(&data).into_storage_result()?;

    Ok(())
}
