use std::borrow::Borrow;
use std::collections::BTreeSet;

use namada::core::address::Address;
use namada::core::hash::Hash;
use namada::core::storage::{Key, TxIndex};
use namada::core::time::DurationSecs;
use namada::ledger::gas::TxGasMeter;
use namada::ledger::parameters::{self, EpochDuration};
use namada::ledger::storage::mockdb::MockDB;
use namada::ledger::storage::testing::TestStorage;
use namada::ledger::storage::write_log::WriteLog;
use namada::ledger::storage::{Sha256Hasher, WlStorage};
pub use namada::tx::data::TxType;
use namada::tx::Tx;
use namada::vm::prefix_iter::PrefixIterators;
use namada::vm::wasm::run::Error;
use namada::vm::wasm::{self, TxCache, VpCache};
use namada::vm::{self, WasmCacheRwAccess};
use namada::{account, token};
use namada_tx_prelude::transaction::TxSentinel;
use namada_tx_prelude::{BorshSerializeExt, Ctx};
use namada_vp_prelude::key::common;
use tempfile::TempDir;

use crate::vp::TestVpEnv;

/// Tx execution context provides access to host env functions
static mut CTX: Ctx = unsafe { Ctx::new() };

/// Tx execution context provides access to host env functions
pub fn ctx() -> &'static mut Ctx {
    unsafe { &mut CTX }
}

/// This module combines the native host function implementations from
/// `native_tx_host_env` with the functions exposed to the tx wasm
/// that will call to the native functions, instead of interfacing via a
/// wasm runtime. It can be used for host environment integration tests.
pub mod tx_host_env {
    pub use namada_tx_prelude::*;

    pub use super::ctx;
    pub use super::native_tx_host_env::*;
}

/// Host environment structures required for transactions.
#[derive(Debug)]
pub struct TestTxEnv {
    pub wl_storage: WlStorage<MockDB, Sha256Hasher>,
    pub iterators: PrefixIterators<'static, MockDB>,
    pub verifiers: BTreeSet<Address>,
    pub gas_meter: TxGasMeter,
    pub sentinel: TxSentinel,
    pub tx_index: TxIndex,
    pub result_buffer: Option<Vec<u8>>,
    pub vp_wasm_cache: VpCache<WasmCacheRwAccess>,
    pub vp_cache_dir: TempDir,
    pub tx_wasm_cache: TxCache<WasmCacheRwAccess>,
    pub tx_cache_dir: TempDir,
    pub tx: Tx,
}
impl Default for TestTxEnv {
    fn default() -> Self {
        let (vp_wasm_cache, vp_cache_dir) =
            wasm::compilation_cache::common::testing::cache();
        let (tx_wasm_cache, tx_cache_dir) =
            wasm::compilation_cache::common::testing::cache();
        let wl_storage = WlStorage {
            storage: TestStorage::default(),
            write_log: WriteLog::default(),
        };
        let mut tx = Tx::from_type(TxType::Raw);
        tx.header.chain_id = wl_storage.storage.chain_id.clone();
        Self {
            wl_storage,
            iterators: PrefixIterators::default(),
            gas_meter: TxGasMeter::new_from_sub_limit(100_000_000.into()),
            sentinel: TxSentinel::default(),
            tx_index: TxIndex::default(),
            verifiers: BTreeSet::default(),
            result_buffer: None,
            vp_wasm_cache,
            vp_cache_dir,
            tx_wasm_cache,
            tx_cache_dir,
            tx,
        }
    }
}

impl TestTxEnv {
    pub fn all_touched_storage_keys(&self) -> BTreeSet<Key> {
        self.wl_storage.write_log.get_keys()
    }

    pub fn get_verifiers(&self) -> BTreeSet<Address> {
        self.wl_storage
            .write_log
            .verifiers_and_changed_keys(&self.verifiers)
            .0
    }

    pub fn init_parameters(
        &mut self,
        epoch_duration: Option<EpochDuration>,
        vp_allowlist: Option<Vec<String>>,
        tx_allowlist: Option<Vec<String>>,
        max_signatures_per_transaction: Option<u8>,
    ) {
        parameters::update_epoch_parameter(
            &mut self.wl_storage,
            &epoch_duration.unwrap_or(EpochDuration {
                min_num_of_blocks: 1,
                min_duration: DurationSecs(5),
            }),
        )
        .unwrap();
        parameters::update_tx_allowlist_parameter(
            &mut self.wl_storage,
            tx_allowlist.unwrap_or_default(),
        )
        .unwrap();
        parameters::update_vp_allowlist_parameter(
            &mut self.wl_storage,
            vp_allowlist.unwrap_or_default(),
        )
        .unwrap();
        parameters::update_max_signature_per_tx(
            &mut self.wl_storage,
            max_signatures_per_transaction.unwrap_or(15),
        )
        .unwrap();
    }

    pub fn store_wasm_code(&mut self, code: Vec<u8>) {
        let hash = Hash::sha256(&code);
        let key = Key::wasm_code(&hash);
        self.wl_storage.storage.write(&key, code).unwrap();
    }

    /// Fake accounts' existence by initializing their VP storage.
    /// This is needed for accounts that are being modified by a tx test to
    /// pass account existence check in `tx_write` function. Only established
    /// addresses ([`Address::Established`]) have their VP storage initialized,
    /// as other types of accounts should not have wasm VPs in storage in any
    /// case.
    pub fn spawn_accounts(
        &mut self,
        addresses: impl IntoIterator<Item = impl Borrow<Address>>,
    ) {
        for address in addresses {
            if matches!(
                address.borrow(),
                Address::Internal(_) | Address::Implicit(_)
            ) {
                continue;
            }
            let key = Key::validity_predicate(address.borrow());
            let vp_code = vec![];
            self.wl_storage
                .storage
                .write(&key, vp_code)
                .expect("Unable to write VP");
        }
    }

    pub fn init_account_storage(
        &mut self,
        owner: &Address,
        public_keys: Vec<common::PublicKey>,
        threshold: u8,
    ) {
        account::init_account_storage(
            &mut self.wl_storage,
            owner,
            &public_keys,
            threshold,
        )
        .expect("Unable to write Account substorage.");
    }

    /// Set public key for the address.
    pub fn write_account_threshold(
        &mut self,
        address: &Address,
        threshold: u8,
    ) {
        let storage_key = account::threshold_key(address);
        self.wl_storage
            .storage
            .write(&storage_key, threshold.serialize_to_vec())
            .unwrap();
    }

    /// Commit the genesis state. Typically, you'll want to call this after
    /// setting up the initial state, before running a transaction.
    pub fn commit_genesis(&mut self) {
        self.wl_storage.commit_block().unwrap();
    }

    pub fn commit_tx_and_block(&mut self) {
        self.wl_storage.commit_tx();
        self.wl_storage
            .commit_block()
            .map_err(|err| println!("{:?}", err))
            .ok();
        self.iterators = PrefixIterators::default();
        self.verifiers = BTreeSet::default();
    }

    /// Credit tokens to the target account.
    pub fn credit_tokens(
        &mut self,
        target: &Address,
        token: &Address,
        amount: token::Amount,
    ) {
        let storage_key = token::storage_key::balance_key(token, target);
        self.wl_storage
            .storage
            .write(&storage_key, amount.serialize_to_vec())
            .unwrap();
    }

    /// Apply the tx changes to the write log.
    pub fn execute_tx(&mut self) -> Result<(), Error> {
        wasm::run::tx(
            &self.wl_storage.storage,
            &mut self.wl_storage.write_log,
            &mut self.gas_meter,
            &self.tx_index,
            &self.tx,
            &mut self.vp_wasm_cache,
            &mut self.tx_wasm_cache,
        )
        .and(Ok(()))
    }
}

/// This module allows to test code with tx host environment functions.
/// It keeps a thread-local global `TxEnv`, which is passed to any of
/// invoked host environment functions and so it must be initialized
/// before the test.
mod native_tx_host_env {

    use std::cell::RefCell;
    use std::pin::Pin;

    // TODO replace with `std::concat_idents` once stabilized (https://github.com/rust-lang/rust/issues/29599)
    use concat_idents::concat_idents;
    use namada::vm::host_env::*;

    use super::*;

    thread_local! {
        /// A [`TestTxEnv`] that can be used for tx host env functions calls
        /// that implements the WASM host environment in native environment.
        pub static ENV: RefCell<Option<Pin<Box<TestTxEnv>>>> =
            RefCell::new(None);
    }

    /// Initialize the tx host environment in [`ENV`]. This will be used in the
    /// host env function calls via macro `native_host_fn!`.
    pub fn init() {
        ENV.with(|env| {
            let test_env = TestTxEnv::default();
            *env.borrow_mut() = Some(Box::pin(test_env));
        });
    }

    /// Set the tx host environment in [`ENV`] from the given [`TestTxEnv`].
    /// This will be used in the host env function calls via
    /// macro `native_host_fn!`.
    pub fn set(test_env: TestTxEnv) {
        ENV.with(|env| {
            *env.borrow_mut() = Some(Box::pin(test_env));
        });
    }

    /// Mutably borrow the [`TestTxEnv`] from [`ENV`]. The [`ENV`] must be
    /// initialized.
    pub fn with<T>(f: impl Fn(&mut TestTxEnv) -> T) -> T {
        ENV.with(|env| {
            let mut env = env.borrow_mut();
            let mut env = env
                .as_mut()
                .expect(
                    "Did you forget to initialize the ENV? (e.g. call to \
                     `tx_host_env::init()`)",
                )
                .as_mut();
            f(&mut env)
        })
    }

    /// Take the [`TestTxEnv`] out of [`ENV`]. The [`ENV`] must be initialized.
    pub fn take() -> TestTxEnv {
        ENV.with(|env| {
            let mut env = env.borrow_mut();
            let env = env.take().expect(
                "Did you forget to initialize the ENV? (e.g. call to \
                 `tx_host_env::init()`)",
            );
            let env = Pin::into_inner(env);
            *env
        })
    }

    pub fn commit_tx_and_block() {
        with(|env| env.commit_tx_and_block())
    }

    /// Set the [`TestTxEnv`] back from a [`TestVpEnv`]. This is useful when
    /// testing validation with multiple transactions that accumulate some state
    /// changes.
    pub fn set_from_vp_env(vp_env: TestVpEnv) {
        let TestVpEnv {
            wl_storage,
            tx,
            vp_wasm_cache,
            vp_cache_dir,
            ..
        } = vp_env;
        let tx_env = TestTxEnv {
            wl_storage,
            vp_wasm_cache,
            vp_cache_dir,
            tx,
            ..Default::default()
        };
        set(tx_env);
    }

    /// A helper macro to create implementations of the host environment
    /// functions exported to wasm, which uses the environment from the
    /// `ENV` variable.
    macro_rules! native_host_fn {
            // unit return type
            ( $fn:ident ( $($arg:ident : $type:ty),* $(,)?) ) => {
                concat_idents!(extern_fn_name = namada, _, $fn {
                    #[no_mangle]
                    extern "C" fn extern_fn_name( $($arg: $type),* ) {
                        with(|TestTxEnv {
                                wl_storage,
                                iterators,
                                verifiers,
                                gas_meter,
                                sentinel,
                                result_buffer,
                                tx_index,
                                vp_wasm_cache,
                                vp_cache_dir: _,
                                tx_wasm_cache,
                                tx_cache_dir: _,
                                tx,
                            }: &mut TestTxEnv| {

                            let tx_env = vm::host_env::testing::tx_env(
                                &wl_storage.storage,
                                &mut wl_storage.write_log,
                                iterators,
                                verifiers,
                                gas_meter,
                                sentinel,
                                tx,
                                tx_index,
                                result_buffer,
                                vp_wasm_cache,
                                tx_wasm_cache,
                            );

                            // Call the `host_env` function and unwrap any
                            // runtime errors
                            $fn( &tx_env, $($arg),* ).unwrap()
                        })
                    }
                });
            };

            // non-unit return type
            ( $fn:ident ( $($arg:ident : $type:ty),* $(,)?) -> $ret:ty ) => {
                concat_idents!(extern_fn_name = namada, _, $fn {
                    #[no_mangle]
                    extern "C" fn extern_fn_name( $($arg: $type),* ) -> $ret {
                        with(|TestTxEnv {
                            tx_index,
                                wl_storage,
                                iterators,
                                verifiers,
                                gas_meter,
                                sentinel,
                                result_buffer,
                                vp_wasm_cache,
                                vp_cache_dir: _,
                                tx_wasm_cache,
                                tx_cache_dir: _,
                                tx,
                            }: &mut TestTxEnv| {

                            let tx_env = vm::host_env::testing::tx_env(
                                &wl_storage.storage,
                                &mut wl_storage.write_log,
                                iterators,
                                verifiers,
                                gas_meter,
                                sentinel,
                                tx,
                                tx_index,
                                result_buffer,
                                vp_wasm_cache,
                                tx_wasm_cache,
                            );

                            // Call the `host_env` function and unwrap any
                            // runtime errors
                            $fn( &tx_env, $($arg),* ).unwrap()
                        })
                    }
                });
            };

            // unit, non-result, return type
            ( "non-result", $fn:ident ( $($arg:ident : $type:ty),* $(,)?) ) => {
                concat_idents!(extern_fn_name = namada, _, $fn {
                    #[no_mangle]
                    extern "C" fn extern_fn_name( $($arg: $type),* ) {
                        with(|TestTxEnv {
                                wl_storage,
                                iterators,
                                verifiers,
                                gas_meter,
                                sentinel,
                                result_buffer,
                                tx_index,
                                vp_wasm_cache,
                                vp_cache_dir: _,
                                tx_wasm_cache,
                                tx_cache_dir: _,
                                tx,
                            }: &mut TestTxEnv| {

                            let tx_env = vm::host_env::testing::tx_env(
                                &wl_storage.storage,
                                &mut wl_storage.write_log,
                                iterators,
                                verifiers,
                                gas_meter,
                                sentinel,
                                tx,
                                tx_index,
                                result_buffer,
                                vp_wasm_cache,
                                tx_wasm_cache,
                            );

                            // Call the `host_env` function
                            $fn( &tx_env, $($arg),* )
                        })
                    }
                });
            };
        }

    // Implement all the exported functions from
    // [`namada_vm_env::imports::tx`] `extern "C"` section.
    native_host_fn!(tx_read(key_ptr: u64, key_len: u64) -> i64);
    native_host_fn!(tx_result_buffer(result_ptr: u64));
    native_host_fn!(tx_has_key(key_ptr: u64, key_len: u64) -> i64);
    native_host_fn!(tx_write(
        key_ptr: u64,
        key_len: u64,
        val_ptr: u64,
        val_len: u64
    ));
    native_host_fn!(tx_write_temp(
        key_ptr: u64,
        key_len: u64,
        val_ptr: u64,
        val_len: u64
    ));
    native_host_fn!(tx_delete(key_ptr: u64, key_len: u64));
    native_host_fn!(tx_iter_prefix(prefix_ptr: u64, prefix_len: u64) -> u64);
    native_host_fn!(tx_iter_next(iter_id: u64) -> i64);
    native_host_fn!(tx_insert_verifier(addr_ptr: u64, addr_len: u64));
    native_host_fn!(tx_update_validity_predicate(
        addr_ptr: u64,
        addr_len: u64,
        code_hash_ptr: u64,
        code_hash_len: u64,
        code_tag_ptr: u64,
        code_tag_len: u64,
    ));
    native_host_fn!(tx_init_account(
        code_hash_ptr: u64,
        code_hash_len: u64,
        code_tag_ptr: u64,
        code_tag_len: u64,
        result_ptr: u64
    ));
    native_host_fn!(tx_emit_ibc_event(event_ptr: u64, event_len: u64));
    native_host_fn!(tx_get_ibc_events(event_type_ptr: u64, event_type_len: u64) -> i64);
    native_host_fn!(tx_get_chain_id(result_ptr: u64));
    native_host_fn!(tx_get_block_height() -> u64);
    native_host_fn!(tx_get_tx_index() -> u32);
    native_host_fn!(tx_get_block_header(height: u64) -> i64);
    native_host_fn!(tx_get_block_hash(result_ptr: u64));
    native_host_fn!(tx_get_block_epoch() -> u64);
    native_host_fn!(tx_get_pred_epochs() -> i64);
    native_host_fn!(tx_get_native_token(result_ptr: u64));
    native_host_fn!(tx_log_string(str_ptr: u64, str_len: u64));
    native_host_fn!(tx_charge_gas(used_gas: u64));
    native_host_fn!("non-result", tx_set_commitment_sentinel());
    native_host_fn!(tx_verify_tx_section_signature(
        hash_list_ptr: u64,
        hash_list_len: u64,
        public_keys_map_ptr: u64,
        public_keys_map_len: u64,
        threshold: u8,
        max_signatures_ptr: u64,
        max_signatures_len: u64,
    ) -> i64);
}

#[cfg(test)]
mod tests {
    use namada::core::storage;
    use namada::ledger::storage::mockdb::MockDB;
    use namada::vm::host_env::{self, TxVmEnv};
    use namada::vm::memory::VmMemory;
    use proptest::prelude::*;
    use test_log::test;

    use super::*;

    #[derive(Debug)]
    struct TestSetup {
        write_to_memory: bool,
        write_to_wl: bool,
        write_to_storage: bool,
        key: storage::Key,
        key_memory_ptr: u64,
        read_buffer_memory_ptr: u64,
        val: Vec<u8>,
        val_memory_ptr: u64,
    }

    impl TestSetup {
        fn key_bytes(&self) -> Vec<u8> {
            self.key.to_string().as_bytes().to_vec()
        }

        fn key_len(&self) -> u64 {
            self.key_bytes().len() as _
        }

        fn val_len(&self) -> u64 {
            self.val.len() as _
        }
    }

    proptest! {
        #[test]
        fn test_tx_read_cannot_panic(
            setup in arb_test_setup()
        ) {
            test_tx_read_cannot_panic_aux(setup)
        }
    }

    fn test_tx_read_cannot_panic_aux(setup: TestSetup) {
        // dbg!(&setup);

        let mut test_env = TestTxEnv::default();
        let tx_env = setup_host_env(&setup, &mut test_env);

        // Can fail, but must not panic
        let _res =
            host_env::tx_read(&tx_env, setup.key_memory_ptr, setup.key_len());
        let _res =
            host_env::tx_result_buffer(&tx_env, setup.read_buffer_memory_ptr);
    }

    proptest! {
        #[test]
        fn test_tx_charge_gas_cannot_panic(
            setup in arb_test_setup(),
            gas in arb_u64(),
        ) {
            test_tx_charge_gas_cannot_panic_aux(setup, gas)
        }
    }

    fn test_tx_charge_gas_cannot_panic_aux(setup: TestSetup, gas: u64) {
        let mut test_env = TestTxEnv::default();
        let tx_env = setup_host_env(&setup, &mut test_env);

        // Can fail, but must not panic
        let _res = host_env::tx_charge_gas(&tx_env, gas);
    }

    proptest! {
        #[test]
        fn test_tx_has_key_cannot_panic(
            setup in arb_test_setup(),
        ) {
            test_tx_has_key_cannot_panic_aux(setup)
        }
    }

    fn test_tx_has_key_cannot_panic_aux(setup: TestSetup) {
        // dbg!(&setup);

        let mut test_env = TestTxEnv::default();
        let tx_env = setup_host_env(&setup, &mut test_env);

        // Can fail, but must not panic
        let _res = host_env::tx_has_key(
            &tx_env,
            setup.key_memory_ptr,
            setup.key_len(),
        );
    }

    proptest! {
        #[test]
        fn test_tx_write_cannot_panic(
            setup in arb_test_setup(),
        ) {
            test_tx_write_cannot_panic_aux(setup)
        }
    }

    fn test_tx_write_cannot_panic_aux(setup: TestSetup) {
        // dbg!(&setup);

        let mut test_env = TestTxEnv::default();
        let tx_env = setup_host_env(&setup, &mut test_env);

        // Can fail, but must not panic
        let _res = host_env::tx_write(
            &tx_env,
            setup.key_memory_ptr,
            setup.key_len(),
            setup.val_memory_ptr,
            setup.val_len(),
        );
        let _res = host_env::tx_write_temp(
            &tx_env,
            setup.key_memory_ptr,
            setup.key_len(),
            setup.val_memory_ptr,
            setup.val_len(),
        );
    }

    proptest! {
        #[test]
        fn test_tx_delete_cannot_panic(
            setup in arb_test_setup(),
        ) {
            test_tx_delete_cannot_panic_aux(setup)
        }
    }

    fn test_tx_delete_cannot_panic_aux(setup: TestSetup) {
        // dbg!(&setup);

        let mut test_env = TestTxEnv::default();
        let tx_env = setup_host_env(&setup, &mut test_env);

        // Can fail, but must not panic
        let _res =
            host_env::tx_delete(&tx_env, setup.key_memory_ptr, setup.key_len());
    }

    proptest! {
        #[test]
        fn test_tx_iter_prefix_cannot_panic(
            setup in arb_test_setup(),
        ) {
            test_tx_iter_prefix_cannot_panic_aux(setup)
        }
    }

    fn test_tx_iter_prefix_cannot_panic_aux(setup: TestSetup) {
        // dbg!(&setup);

        let mut test_env = TestTxEnv::default();
        let tx_env = setup_host_env(&setup, &mut test_env);

        // Can fail, but must not panic
        let _res = host_env::tx_iter_prefix(
            &tx_env,
            setup.key_memory_ptr,
            setup.key_len(),
        );
        let _res = host_env::tx_iter_next(
            &tx_env,
            // This field is not used for anything else in this test
            setup.val_memory_ptr,
        );
    }

    fn setup_host_env(
        setup: &TestSetup,
        test_env: &mut TestTxEnv,
    ) -> TxVmEnv<
        'static,
        wasm::memory::WasmMemory,
        MockDB,
        Sha256Hasher,
        WasmCacheRwAccess,
    > {
        if setup.write_to_storage {
            // Write the key-val to storage which may affect `tx_read` execution
            // path
            let _res =
                test_env.wl_storage.storage.write(&setup.key, &setup.val);
        }
        if setup.write_to_wl {
            // Write the key-val to write log which may affect `tx_read`
            // execution path
            let _res = test_env
                .wl_storage
                .write_log
                .write(&setup.key, setup.val.clone());
        }

        let TestTxEnv {
            wl_storage,
            iterators,
            verifiers,
            gas_meter,
            sentinel,
            result_buffer,
            tx_index,
            vp_wasm_cache,
            vp_cache_dir: _,
            tx_wasm_cache,
            tx_cache_dir: _,
            tx,
        } = test_env;

        let tx_env = vm::host_env::testing::tx_env_with_wasm_memory(
            &wl_storage.storage,
            &mut wl_storage.write_log,
            iterators,
            verifiers,
            gas_meter,
            sentinel,
            tx,
            tx_index,
            result_buffer,
            vp_wasm_cache,
            tx_wasm_cache,
        );

        if setup.write_to_memory {
            let key_bytes = setup.key_bytes();
            // Write the key-val to memory which may affect `tx_read` execution
            // path. Can fail, but must not panic
            let _res =
                tx_env.memory.write_bytes(setup.key_memory_ptr, key_bytes);
        }

        tx_env
    }

    fn arb_test_setup() -> impl Strategy<Value = TestSetup> {
        (
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            namada::core::storage::testing::arb_key(),
            arb_u64(),
            arb_u64(),
            any::<Vec<u8>>(),
            arb_u64(),
        )
            .prop_map(
                |(
                    write_to_memory,
                    write_to_wl,
                    write_to_storage,
                    key,
                    key_memory_ptr,
                    read_buffer_memory_ptr,
                    val,
                    val_memory_ptr,
                )| TestSetup {
                    write_to_memory,
                    write_to_wl,
                    write_to_storage,
                    key,
                    key_memory_ptr,
                    read_buffer_memory_ptr,
                    val,
                    val_memory_ptr,
                },
            )
    }

    fn arb_u64() -> impl Strategy<Value = u64> {
        prop_oneof![
            5 => Just(u64::MIN),
            5 => Just(u64::MIN + 1),
            5 => u64::MIN + 2..=u32::MAX as u64,
            1 => Just(u64::MAX),
            1 => Just(u64::MAX - 1),
            1 => u32::MAX as u64 + 1..u64::MAX - 1,
        ]
    }
}
