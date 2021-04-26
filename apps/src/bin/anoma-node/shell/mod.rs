pub mod gas;
pub mod storage;
mod tendermint;

use core::fmt;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc;
use std::vec;

use anoma::protobuf::types::Tx;
use anoma_shared::bytes::ByteBuf;
use prost::Message;
use rayon::prelude::*;
use thiserror::Error;

use self::gas::{BlockGasMeter, VpGasMeter};
use self::storage::{Address, BlockHash, BlockHeight, Key, Storage};
use self::tendermint::{AbciMsg, AbciReceiver};
use crate::vm::host_env::write_log::WriteLog;
use crate::vm::{self, TxRunner, VpRunner};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Error removing the DB data: {0}")]
    RemoveDB(std::io::Error),
    #[error("Storage error: {0}")]
    StorageError(storage::Error),
    #[error("Shell ABCI channel receiver error: {0}")]
    AbciChannelRecvError(mpsc::RecvError),
    #[error("Shell ABCI channel sender error: {0}")]
    AbciChannelSendError(String),
    #[error("Error decoding a transaction from bytes: {0}")]
    TxDecodingError(prost::DecodeError),
    #[error("Transaction runner error: {0}")]
    TxRunnerError(vm::Error),
    #[error("Gas error: {0}")]
    GasError(gas::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn run(config: anoma::config::Ledger) -> Result<()> {
    // open a channel between ABCI (the sender) and the shell (the receiver)
    let (sender, receiver) = mpsc::channel();
    let shell = Shell::new(receiver, &config.db);
    // Run Tendermint ABCI server in another thread
    std::thread::spawn(move || tendermint::run(sender, config));
    shell.run()
}

pub fn reset(config: anoma::config::Ledger) -> Result<()> {
    // simply nuke the DB files
    let db_path = &config.db;
    match std::fs::remove_dir_all(&db_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        res => res.map_err(Error::RemoveDB)?,
    };
    // reset Tendermint state
    tendermint::reset(config);
    Ok(())
}

#[derive(Debug)]
pub struct Shell {
    abci: AbciReceiver,
    storage: storage::Storage,
    // The gas meter is sync with mutex to allow VPs sharing it
    // TODO it should be possible to impl a lock-free gas metering for VPs
    gas_meter: BlockGasMeter,
    write_log: WriteLog,
}

#[derive(Clone, Debug)]
pub enum MempoolTxType {
    /// A transaction that has not been validated by this node before
    NewTransaction,
    /// A transaction that has been validated at some previous level that may
    /// need to be validated again
    RecheckTransaction,
}

pub struct MerkleRoot(pub Vec<u8>);

impl Shell {
    pub fn new(abci: AbciReceiver, db_path: impl AsRef<Path>) -> Self {
        let mut storage = Storage::new(db_path);
        // TODO load initial accounts from genesis
        let key11 = Key::parse("@ada/balance/eth".to_owned())
            .expect("Unable to convert string into a key");
        storage
            .write(
                &key11,
                vec![0x10_u8, 0x27_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8],
            )
            .expect("Unable to set the initial balance for validator account");
        let key12 = Key::parse("@ada/balance/xtz".to_owned())
            .expect("Unable to convert string into a key");
        storage
            .write(
                &key12,
                vec![0x10_u8, 0x27_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8],
            )
            .expect("Unable to set the initial balance for validator account");
        let key21 = Key::parse("@alan/balance/eth".to_owned())
            .expect("Unable to convert string into a key");
        storage
            .write(
                &key21,
                vec![0x64_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8],
            )
            .expect("Unable to set the initial balance for basic account");
        let key22 = Key::parse("@alan/balance/xtz".to_owned())
            .expect("Unable to convert string into a key");
        storage
            .write(
                &key22,
                vec![0x64_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8, 0_u8],
            )
            .expect("Unable to set the initial balance for basic account");
        Self {
            abci,
            storage,
            gas_meter: BlockGasMeter::default(),
            write_log: WriteLog::new(),
        }
    }

    /// Run the shell in the current thread (blocking).
    pub fn run(mut self) -> Result<()> {
        loop {
            let msg = self.abci.recv().map_err(Error::AbciChannelRecvError)?;
            match msg {
                AbciMsg::GetInfo { reply } => {
                    let result = self.last_state();
                    reply.send(result).map_err(|e| {
                        Error::AbciChannelSendError(format!("GetInfo {}", e))
                    })?
                }
                AbciMsg::InitChain { reply, chain_id } => {
                    self.init_chain(chain_id)?;
                    reply.send(()).map_err(|e| {
                        Error::AbciChannelSendError(format!("InitChain {}", e))
                    })?
                }
                AbciMsg::MempoolValidate { reply, tx, r#type } => {
                    let result = self
                        .mempool_validate(&tx, r#type)
                        .map_err(|e| format!("{}", e));
                    reply.send(result).map_err(|e| {
                        Error::AbciChannelSendError(format!(
                            "MempoolValidate {}",
                            e
                        ))
                    })?
                }
                AbciMsg::BeginBlock {
                    reply,
                    hash,
                    height,
                } => {
                    self.begin_block(hash, height);
                    reply.send(()).map_err(|e| {
                        Error::AbciChannelSendError(format!("BeginBlock {}", e))
                    })?
                }
                AbciMsg::ApplyTx { reply, tx } => {
                    let result =
                        self.apply_tx(&tx).map_err(|e| format!("{}", e));
                    reply.send(result).map_err(|e| {
                        Error::AbciChannelSendError(format!("ApplyTx {}", e))
                    })?
                }
                AbciMsg::EndBlock { reply, height } => {
                    self.end_block(height);
                    reply.send(()).map_err(|e| {
                        Error::AbciChannelSendError(format!("EndBlock {}", e))
                    })?
                }
                AbciMsg::CommitBlock { reply } => {
                    let result = self.commit();
                    reply.send(result).map_err(|e| {
                        Error::AbciChannelSendError(format!(
                            "CommitBlock {}",
                            e
                        ))
                    })?
                }
                AbciMsg::AbciQuery {
                    reply,
                    path,
                    data,
                    height: _,
                    prove: _,
                } => {
                    if path == "dry_run_tx" {
                        let result = self
                            .dry_run_tx(&data)
                            .map_err(|e| format!("{}", e));

                        reply.send(result).map_err(|e| {
                            Error::AbciChannelSendError(format!(
                                "ApplyTx {}",
                                e
                            ))
                        })?
                    }
                }
            }
        }
    }
}
#[derive(Clone)]
struct VpsResult {
    pub accepted_vps: HashSet<Address>,
    pub rejected_vps: HashSet<Address>,
    pub changed_keys: Vec<String>,
    pub gas_used: Vec<u64>,
    pub have_error: bool,
}

impl VpsResult {
    pub fn new(
        accepted_vps: HashSet<Address>,
        rejected_vps: HashSet<Address>,
        changed_keys: Vec<String>,
        gas_used: Vec<u64>,
        have_error: bool,
    ) -> Self {
        Self {
            accepted_vps,
            rejected_vps,
            changed_keys,
            gas_used,
            have_error,
        }
    }
}

impl fmt::Display for VpsResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Vps -> accepted: {:?}. rejected: {:?}, keys: {:?}, gas_used: \
             {:?}, error: {:}",
            self.accepted_vps,
            self.rejected_vps,
            self.changed_keys,
            self.gas_used,
            self.have_error
        )
    }
}

impl Default for VpsResult {
    fn default() -> Self {
        Self {
            accepted_vps: HashSet::default(),
            rejected_vps: HashSet::default(),
            changed_keys: Vec::default(),
            gas_used: Vec::default(),
            have_error: false,
        }
    }
}

struct TxResult {
    // a value of 0 indicates that the transaction overflowed with gas
    gas_used: u64,
    vps: VpsResult,
    valid: bool,
}

impl TxResult {
    pub fn new(gas: Result<u64>, vps: VpsResult) -> Self {
        let mut tx_result = TxResult {
            gas_used: gas.unwrap_or(0),
            vps,
            valid: false,
        };
        tx_result.valid = tx_result.is_tx_correct();
        tx_result
    }

    pub fn is_tx_correct(&self) -> bool {
        self.vps.rejected_vps.is_empty()
    }
}

impl fmt::Display for TxResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Transaction is valid: {}. Gas used: {}, vps: {}",
            self.valid,
            self.gas_used,
            self.vps.to_string(),
        )
    }
}

impl Shell {
    pub fn init_chain(&mut self, chain_id: String) -> Result<()> {
        self.storage
            .set_chain_id(&chain_id)
            .map_err(Error::StorageError)
    }

    /// Validate a transaction request. On success, the transaction will
    /// included in the mempool and propagated to peers, otherwise it will be
    /// rejected.
    pub fn mempool_validate(
        &self,
        tx_bytes: &[u8],
        r#_type: MempoolTxType,
    ) -> Result<()> {
        let _tx = Tx::decode(tx_bytes).map_err(Error::TxDecodingError)?;
        Ok(())
    }

    /// Validate and apply a transaction.
    pub fn dry_run_tx(&mut self, tx_bytes: &[u8]) -> Result<String> {
        let mut gas_meter = BlockGasMeter::default();
        let mut write_log = self.write_log.clone();
        let result =
            run_tx(tx_bytes, &mut gas_meter, &mut write_log, &self.storage)?;
        Ok(result.to_string())
    }

    /// Validate and apply a transaction.
    pub fn apply_tx(&mut self, tx_bytes: &[u8]) -> Result<u64> {
        let result = run_tx(
            tx_bytes,
            &mut self.gas_meter.clone(),
            &mut self.write_log,
            &self.storage,
        )?;
        // Apply the transaction if accepted by all the VPs
        if result.vps.rejected_vps.is_empty() {
            log::debug!(
                "all accepted apply_tx storage modification {:#?}",
                self.storage
            );
            self.write_log.commit_tx();
        } else {
            self.write_log.drop_tx();
        }
        Ok(result.gas_used)
    }

    /// Begin a new block.
    pub fn begin_block(&mut self, hash: BlockHash, height: BlockHeight) {
        self.gas_meter.reset();
        self.storage.begin_block(hash, height).unwrap();
    }

    /// End a block.
    pub fn end_block(&mut self, _height: BlockHeight) {}

    /// Commit a block. Persist the application state and return the Merkle root
    /// hash.
    pub fn commit(&mut self) -> MerkleRoot {
        // commit changes from the write-log to storage
        self.write_log
            .commit_block(&mut self.storage)
            .expect("Expected committing block write log success");
        log::debug!("storage to commit {:#?}", self.storage);
        // store the block's data in DB
        // TODO commit async?
        self.storage.commit().unwrap_or_else(|e| {
            log::error!(
                "Encountered a storage error while committing a block {:?}",
                e
            )
        });
        let root = self.storage.merkle_root();
        MerkleRoot(root.as_slice().to_vec())
    }

    /// Load the Merkle root hash and the height of the last committed block, if
    /// any.
    pub fn last_state(&mut self) -> Option<(MerkleRoot, u64)> {
        let result = self.storage.load_last_state().unwrap_or_else(|e| {
            log::error!(
                "Encountered an error while reading last state from
        storage {}",
                e
            );
            None
        });
        match &result {
            Some((root, height)) => {
                log::info!(
                    "Last state root hash: {}, height: {}",
                    ByteBuf(&root.0),
                    height
                )
            }
            None => {
                log::info!("No state could be found")
            }
        }
        result
    }
}

fn get_verifiers(
    write_log: &WriteLog,
    verifiers: &HashSet<Address>,
) -> HashMap<Address, Vec<String>> {
    let mut verifiers =
        verifiers.iter().fold(HashMap::new(), |mut acc, addr| {
            acc.insert(addr.clone(), vec![]);
            acc
        });
    // get changed keys grouped by the address
    for key in &write_log.get_changed_keys() {
        for addr in &key.find_addresses() {
            match verifiers.get_mut(&addr) {
                Some(keys) => keys.push(key.to_string()),
                None => {
                    verifiers.insert(addr.clone(), vec![key.to_string()]);
                }
            }
        }
    }
    verifiers
}

fn run_tx(
    tx_bytes: &[u8],
    block_gas_meter: &mut BlockGasMeter,
    write_log: &mut WriteLog,
    storage: &Storage,
) -> Result<TxResult> {
    block_gas_meter
        .add_base_transaction_fee(tx_bytes.len())
        .map_err(Error::GasError)?;

    let tx = Tx::decode(tx_bytes).map_err(Error::TxDecodingError)?;

    // Execute the transaction code
    let verifiers = execute_tx(&tx, storage, block_gas_meter, write_log)?;

    let vps_result =
        check_vps(&tx, storage, block_gas_meter, write_log, &verifiers, true)?;

    let gas = block_gas_meter
        .finalize_transaction()
        .map_err(Error::GasError);

    Ok(TxResult::new(gas, vps_result))
}

fn check_vps(
    tx: &Tx,
    storage: &Storage,
    gas_meter: &mut BlockGasMeter,
    write_log: &mut WriteLog,
    verifiers: &HashSet<Address>,
    dry_run: bool,
) -> Result<VpsResult> {
    let verifiers = get_verifiers(write_log, verifiers);

    let tx_data = tx.data.clone().unwrap_or_default();

    let verifiers_vps: Vec<(Address, Vec<String>, Vec<u8>)> = verifiers
        .iter()
        .map(|(addr, keys)| {
            let vp = storage
                .validity_predicate(&addr)
                .map_err(Error::StorageError)?;

            gas_meter
                .add_compiling_fee(vp.len())
                .map_err(Error::GasError)?;

            Ok((addr.clone(), keys.clone(), vp))
        })
        .collect::<std::result::Result<_, _>>()?;

    let initial_gas = gas_meter.get_current_transaction_gas();

    let mut vps_result;

    if dry_run {
        vps_result = run_vps_dry(
            verifiers_vps,
            tx_data,
            storage,
            write_log,
            initial_gas,
        );
    } else {
        vps_result =
            run_vps(verifiers_vps, tx_data, storage, write_log, initial_gas)?;
    }

    // sort decreasing order
    vps_result.gas_used.sort_by(|a, b| b.cmp(a));

    // I'm assuming that at least 1 VP will always be there
    if let Some((max_gas_used, rest)) = vps_result.gas_used.split_first() {
        gas_meter.add(*max_gas_used).map_err(Error::GasError)?;
        gas_meter
            .add_parallel_fee(&mut rest.to_vec())
            .map_err(Error::GasError)?;
    }

    Ok(vps_result)
}

fn execute_tx(
    tx: &Tx,
    storage: &Storage,
    gas_meter: &mut BlockGasMeter,
    write_log: &mut WriteLog,
) -> Result<HashSet<Address>> {
    let tx_code = tx.code.clone();
    gas_meter
        .add_compiling_fee(tx_code.len())
        .map_err(Error::GasError)?;
    let tx_data = tx.data.clone().unwrap_or_default();
    let mut verifiers = HashSet::new();

    let tx_runner = TxRunner::new();

    tx_runner
        .run(
            storage,
            write_log,
            &mut verifiers,
            gas_meter,
            tx_code,
            tx_data,
        )
        .map_err(Error::TxRunnerError)?;

    Ok(verifiers)
}

fn run_vps_dry(
    verifiers: Vec<(Address, Vec<String>, Vec<u8>)>,
    tx_data: Vec<u8>,
    storage: &Storage,
    write_log: &mut WriteLog,
    initial_gas: u64,
) -> VpsResult {
    let addresses = verifiers
        .iter()
        .map(|(addr, _, _)| addr)
        .collect::<HashSet<_>>();

    verifiers
        .par_iter()
        // in dry-run, we don't short-circuit on failure, instead keep running
        // all VPs until they finish
        .fold_with(VpsResult::default(), |result, (addr, keys, vp)| {
            run_vp(
                result,
                initial_gas,
                tx_data.clone(),
                storage,
                write_log,
                addresses.clone(),
                (addr, keys, vp),
            )
        })
        .collect()
}

fn run_vps(
    verifiers: Vec<(Address, Vec<String>, Vec<u8>)>,
    tx_data: Vec<u8>,
    storage: &Storage,
    write_log: &mut WriteLog,
    initial_gas: u64,
) -> Result<VpsResult> {
    let addresses = verifiers
        .iter()
        .map(|(addr, _, _)| addr)
        .collect::<HashSet<_>>();

    verifiers
        .par_iter()
        .try_fold(VpsResult::default, |result, (addr, keys, vp)| {
            Ok(run_vp(
                result,
                initial_gas,
                tx_data.clone(),
                storage,
                write_log,
                addresses.clone(),
                (addr, keys, vp),
            ))
        })
        .try_reduce(VpsResult::default, |a, b| Ok(merge_vp_results(a, b)))
}

impl FromParallelIterator<VpsResult> for VpsResult {
    fn from_par_iter<I>(par_iter: I) -> Self
    where
        I: IntoParallelIterator<Item = VpsResult>,
    {
        par_iter
            .into_par_iter()
            .fold(VpsResult::default, merge_vp_results)
            .collect()
    }
}

fn merge_vp_results(a: VpsResult, mut b: VpsResult) -> VpsResult {
    let accepted_vps = a.accepted_vps.union(&b.accepted_vps).collect();
    let rejected_vps = a.rejected_vps.union(&b.rejected_vps).collect();
    let mut changed_keys = a.changed_keys;
    changed_keys.append(&mut b.changed_keys);
    let mut gas_used = a.gas_used;
    gas_used.append(&mut b.gas_used);
    let have_error = a.have_error || b.have_error;
    VpsResult::new(
        accepted_vps,
        rejected_vps,
        changed_keys,
        gas_used,
        have_error,
    )
}

fn run_vp(
    mut result: VpsResult,
    initial_gas: u64,
    tx_data: Vec<u8>,
    storage: &Storage,
    write_log: &WriteLog,
    addresses: HashSet<Address>,
    (addr, keys, vp): (&Address, &[String], &[u8]),
) -> VpsResult {
    let mut vp_gas_meter = VpGasMeter::new(initial_gas);
    let vp_runner = VpRunner::new();

    let accept = vp_runner.run(
        vp,
        tx_data,
        addr,
        storage,
        write_log,
        &mut vp_gas_meter,
        keys.to_vec(),
        addresses,
    );
    result.gas_used.push(vp_gas_meter.vp_gas);
    result.changed_keys.extend_from_slice(&keys);

    match accept {
        Ok(accepted) => {
            if !accepted {
                result.rejected_vps.insert(addr.clone());
            } else {
                result.accepted_vps.insert(addr.clone());
            }
        }
        Err(_) => {
            result.rejected_vps.insert(addr.clone());
            result.have_error = true;
        }
    }
    result
}
