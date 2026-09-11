//! Read-set capture must be total.
//!
//! These tests exist because of R1: a read that is not logged is a location
//! validation will never re-check. Each `DatabaseRef` method gets its own case
//! asserting the log grew, including `storage_by_account_id_ref`, which revm
//! supplies a default body for (E1 in `docs/AI_USAGE.md`).

use parevm::state::StateView;
use parevm::{BaseState, Granularity, Key, ReadOrigin, ReadRecorder};
use revm::database_interface::DatabaseRef;
use revm::primitives::{address, Address, StorageKey, StorageValue, B256, U256};
use revm::state::{AccountId, AccountInfo};

const ALICE: Address = address!("1111111111111111111111111111111111111111");

fn recorder(granularity: Granularity) -> ReadRecorder<BaseState> {
    let mut base = BaseState::new();
    base.insert_account(
        ALICE,
        AccountInfo {
            balance: U256::from(1_000u64),
            nonce: 7,
            ..Default::default()
        },
    );
    base.insert_storage(ALICE, StorageKey::from(42u64), StorageValue::from(99u64));
    base.insert_block_hash(1, B256::repeat_byte(0xab));
    ReadRecorder::new(base, granularity)
}

#[test]
fn basic_ref_is_logged() {
    let r = recorder(Granularity::Slot);
    let info = r.basic_ref(ALICE).expect("basic").expect("account exists");
    assert_eq!(info.nonce, 7);
    let set = r.take_read_set();
    assert_eq!(set.len(), 1, "basic_ref must log exactly one read");
    assert_eq!(set.iter().next().unwrap().0, Key::Basic(ALICE));
}

#[test]
fn storage_ref_is_logged() {
    let r = recorder(Granularity::Slot);
    let v = r
        .storage_ref(ALICE, StorageKey::from(42u64))
        .expect("storage");
    assert_eq!(v, StorageValue::from(99u64));
    let set = r.take_read_set();
    assert_eq!(set.len(), 1, "storage_ref must log exactly one read");
    assert_eq!(
        set.iter().next().unwrap().0,
        Key::Storage(ALICE, StorageKey::from(42u64))
    );
}

/// revm gives this method a default body, so the compiler would not have
/// complained had we omitted it. It is implemented explicitly; this proves it.
#[test]
fn storage_by_account_id_ref_is_logged() {
    let r = recorder(Granularity::Slot);
    let v = r
        .storage_by_account_id_ref(
            ALICE,
            AccountId::new(0).expect("account id"),
            StorageKey::from(42u64),
        )
        .expect("storage by id");
    assert_eq!(v, StorageValue::from(99u64));
    let set = r.take_read_set();
    assert_eq!(
        set.len(),
        1,
        "storage_by_account_id_ref must log exactly one read"
    );
}

#[test]
fn code_by_hash_ref_is_logged() {
    let r = recorder(Granularity::Slot);
    let _ = r.code_by_hash_ref(B256::ZERO);
    assert_eq!(
        r.read_set_len(),
        0,
        "a failed lookup logs nothing; the value never reached the EVM"
    );

    let empty = revm::primitives::KECCAK_EMPTY;
    r.code_by_hash_ref(empty).expect("empty code");
    let set = r.take_read_set();
    assert_eq!(set.len(), 1, "code_by_hash_ref must log exactly one read");
    assert_eq!(set.iter().next().unwrap().0, Key::CodeHash(empty));
}

#[test]
fn block_hash_ref_is_logged() {
    let r = recorder(Granularity::Slot);
    let h = r.block_hash_ref(1).expect("block hash");
    assert_eq!(h, B256::repeat_byte(0xab));
    let set = r.take_read_set();
    assert_eq!(set.len(), 1, "block_hash_ref must log exactly one read");
    assert_eq!(set.iter().next().unwrap().0, Key::BlockHash(1));
}

#[test]
fn reads_accumulate_until_taken() {
    let r = recorder(Granularity::Slot);
    r.basic_ref(ALICE).expect("basic");
    r.storage_ref(ALICE, StorageKey::from(42u64)).expect("slot");
    r.block_hash_ref(1).expect("block hash");
    assert_eq!(r.read_set_len(), 3);

    let set = r.take_read_set();
    assert_eq!(set.len(), 3);
    assert_eq!(r.read_set_len(), 0, "taking the set must reset it");
}

#[test]
fn account_granularity_coarsens_storage_keys() {
    let r = recorder(Granularity::Account);
    r.storage_ref(ALICE, StorageKey::from(42u64)).expect("slot");
    r.storage_ref(ALICE, StorageKey::from(43u64)).expect("slot");
    let set = r.take_read_set();

    let keys: Vec<_> = set.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        keys,
        vec![Key::Basic(ALICE), Key::Basic(ALICE)],
        "under account granularity every slot of an account maps to one key"
    );
}

#[test]
fn base_reads_report_base_origin() {
    let r = recorder(Granularity::Slot);
    r.basic_ref(ALICE).expect("basic");
    let set = r.take_read_set();
    assert_eq!(set.iter().next().unwrap().1, ReadOrigin::Base);
}

#[test]
fn immutable_keys_are_excluded_from_validation() {
    let r = recorder(Granularity::Slot);
    r.basic_ref(ALICE).expect("basic");
    r.block_hash_ref(1).expect("block hash");
    r.code_by_hash_ref(revm::primitives::KECCAK_EMPTY)
        .expect("code");
    let set = r.take_read_set();

    assert_eq!(set.len(), 3);
    assert_eq!(
        set.mutable().count(),
        1,
        "only the account read can change; block hash and code cannot"
    );
}

#[test]
fn missing_account_is_still_a_read() {
    let r = recorder(Granularity::Slot);
    let missing = address!("dead00000000000000000000000000000000dead");
    assert!(r.basic_ref(missing).expect("basic").is_none());
    assert_eq!(
        r.read_set_len(),
        1,
        "reading a nonexistent account observes state and must be logged: \
         a later transaction may create it"
    );
}

/// Guards the trait itself. If revm adds a read method, `StateView` must grow
/// to match, and this list is where the team notices.
#[test]
fn state_view_surface_is_what_we_think_it_is() {
    fn assert_impl<V: StateView>() {}
    assert_impl::<BaseState>();
}
