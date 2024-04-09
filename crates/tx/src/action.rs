//! Tx actions are used to indicate from tx to VPs the type of actions that have
//! been applied by the tx to simplify validation (We can check that the
//! storage changes are valid based on the action, rather than trying to derive
//! the action from storage changes). When used, the kind is expected to written
//! to under temporary storage (discarded after tx execution and validation).

use namada_core::address::Address;
use namada_core::borsh::{BorshDeserialize, BorshSerialize};
use namada_core::storage::KeySeg;
use namada_core::{address, storage};

pub use crate::data::pos::{
    Bond, ClaimRewards, Redelegation, Unbond, Withdraw,
};

/// Actions applied from txs.
pub type Actions = Vec<Action>;

/// An action applied from a tx.
#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
pub enum Action {
    Pos(PosAction),
    Gov(GovAction),
    Pgf(PgfAction),
}

/// PoS tx actions.
#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
pub enum PosAction {
    BecomeValidator(Address),
    DeactivateValidator(Address),
    ReactivateValidator(Address),
    Unjail(Address),
    Bond(Bond),
    Unbond(Unbond),
    Withdraw(Withdraw),
    Redelegation(Redelegation),
    ClaimRewards(ClaimRewards),
    CommissionChange(Address),
    MetadataChange(Address),
    ConsensusKeyChange(Address),
}

/// Gov tx actions.
#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
pub enum GovAction {
    InitProposal { id: u64, author: Address },
    VoteProposal { id: u64, voter: Address },
}

/// PGF tx actions.
#[derive(Clone, Debug, BorshDeserialize, BorshSerialize)]
pub enum PgfAction {
    ResignSteward(Address),
    UpdateStewardCommission(Address),
}

/// Read actions from temporary storage
pub trait Read {
    /// Storage access errors
    type Err;

    fn read_temp<T: BorshDeserialize>(
        &self,
        key: &storage::Key,
    ) -> Result<Option<T>, Self::Err>;

    /// Read all the actions applied by a tx
    fn read_actions(&self) -> Result<Actions, Self::Err> {
        let key = storage_key();
        let actions = self.read_temp(&key)?;
        let actions: Actions = actions.unwrap_or_default();
        Ok(actions)
    }
}

/// Write actions to temporary storage
pub trait Write: Read {
    fn write_temp<T: BorshSerialize>(
        &mut self,
        key: &storage::Key,
        val: T,
    ) -> Result<(), Self::Err>;

    /// Push an action applied in a tx.
    fn push_action(&mut self, action: Action) -> Result<(), Self::Err> {
        let key = storage_key();
        let actions = self.read_temp(&key)?;
        let mut actions: Actions = actions.unwrap_or_default();
        actions.push(action);
        self.write_temp(&key, actions)?;
        Ok(())
    }
}

const TX_ACTIONS_KEY: &str = "tx_actions";

fn storage_key() -> storage::Key {
    storage::Key::from(address::TEMP_STORAGE.to_db_key())
        .push(&TX_ACTIONS_KEY.to_owned())
        .expect("Cannot obtain a storage key")
}
