use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash;
use rocksdb::{Options, DB};
use std::path::Path;

/// Semantic aliases.
pub type CovenantId = Hash;
pub type Pubkey = Hash;

/// Persistent storage for the rollup TUI backed by RocksDB.
///
/// Keys are raw byte prefixes (no string encoding for hot-path data).
///
/// ## Schema
///
/// | Prefix     | Key suffix                          | Value                        |
/// |------------|-------------------------------------|------------------------------|
/// | `cov/`     | covenant_id (32B)                   | `CovenantRecord`             |
/// | `meta/`    | covenant_id (32B)                   | `CovenantMeta`               |
/// | `bal/`     | covenant_id (32B) + pubkey (32B)    | u64 balance (8B LE)          |
/// | `acct/`    | covenant_id (32B) + pubkey (32B)    | privkey (32B raw)            |
/// | `utxo/`    | address string bytes                | `Vec<UtxoRecord>` (borsh)    |
/// | `proving/` | covenant_id (32B)                   | `ProvingState`               |
///
/// Balance keys inherit the first-byte index from the pubkey, so:
/// - Lookup by covenant + pubkey: exact 64-byte key
/// - Lookup by covenant + index:  prefix scan on covenant_id(32B) + index(1B)
pub struct RollupDb {
    db: DB,
}

// ── Record types ──

/// Covenant deployment info.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct CovenantRecord {
    /// Deployer private key bytes (POC only — not production-secure).
    pub deployer_privkey: Vec<u8>,
    /// Transaction ID of the deployment tx (None if not yet deployed).
    pub deployment_tx_id: Option<Hash>,
    /// Outpoint of the live covenant UTXO (tx_id, index).
    pub covenant_utxo: Option<(Hash, u32)>,
    /// Unix timestamp of creation.
    pub created_at: u64,
}

/// Aggregate covenant state (state root + seq commitment).
/// Individual balances are stored separately under `bal/`.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct CovenantMeta {
    pub state_root: Hash,
    pub seq_commitment: Hash,
}

/// Serialisable UTXO record.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct UtxoRecord {
    /// Outpoint: (tx_id, output_index).
    pub outpoint: (Hash, u32),
    /// Amount in sompi.
    pub amount: u64,
    /// Script public key bytes.
    pub spk: Vec<u8>,
}

/// Proving progress for a covenant.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct ProvingState {
    pub last_proved_block_hash: Hash,
    pub state_root: Hash,
    pub seq_commitment: Hash,
    pub proof_count: u64,
}

// ── Prefix constants ──

const PREFIX_COVENANT: &[u8] = b"cov/";
const PREFIX_META: &[u8] = b"meta/";
const PREFIX_BALANCE: &[u8] = b"bal/";
const PREFIX_ACCOUNT: &[u8] = b"acct/";
const PREFIX_UTXO: &[u8] = b"utxo/";
const PREFIX_PROVING: &[u8] = b"proving/";

impl RollupDb {
    /// Open (or create) the database at the given path.
    pub fn open(path: &Path) -> Result<Self, rocksdb::Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self { db })
    }

    // ── Covenant records ──

    pub fn put_covenant(&self, id: CovenantId, record: &CovenantRecord) -> Result<(), rocksdb::Error> {
        let key = prefix_key(PREFIX_COVENANT, &id.as_bytes());
        let val = borsh::to_vec(record).expect("borsh serialize");
        self.db.put(key, val)
    }

    pub fn get_covenant(&self, id: CovenantId) -> Result<Option<CovenantRecord>, rocksdb::Error> {
        let key = prefix_key(PREFIX_COVENANT, &id.as_bytes());
        Ok(self.db.get(key)?.map(|v| CovenantRecord::try_from_slice(&v).expect("borsh deserialize")))
    }

    pub fn delete_covenant(&self, id: CovenantId) -> Result<(), rocksdb::Error> {
        let key = prefix_key(PREFIX_COVENANT, &id.as_bytes());
        self.db.delete(key)
    }

    /// List all covenants. Returns (covenant_id, record) pairs.
    pub fn list_covenants(&self) -> Vec<(CovenantId, CovenantRecord)> {
        let iter = self.db.prefix_iterator(PREFIX_COVENANT);
        let mut results = Vec::new();
        for item in iter {
            let (k, v) = item.expect("db iterator");
            if !k.starts_with(PREFIX_COVENANT) {
                break;
            }
            let id = Hash::from_slice(&k[PREFIX_COVENANT.len()..]);
            let record = CovenantRecord::try_from_slice(&v).expect("borsh deserialize");
            results.push((id, record));
        }
        results
    }

    // ── Covenant metadata (state root + seq commitment) ──

    pub fn put_covenant_meta(&self, id: CovenantId, meta: &CovenantMeta) -> Result<(), rocksdb::Error> {
        let key = prefix_key(PREFIX_META, &id.as_bytes());
        let val = borsh::to_vec(meta).expect("borsh serialize");
        self.db.put(key, val)
    }

    pub fn get_covenant_meta(&self, id: CovenantId) -> Result<Option<CovenantMeta>, rocksdb::Error> {
        let key = prefix_key(PREFIX_META, &id.as_bytes());
        Ok(self.db.get(key)?.map(|v| CovenantMeta::try_from_slice(&v).expect("borsh deserialize")))
    }

    // ── Balances (individual SMT leaves) ──

    /// Store a single account balance. Key = covenant_id(32) + pubkey(32).
    pub fn put_balance(&self, id: CovenantId, pubkey: Pubkey, balance: u64) -> Result<(), rocksdb::Error> {
        let key = balance_key(id, pubkey);
        self.db.put(key, balance.to_le_bytes())
    }

    /// Get a single account balance. Returns 0 if not found.
    pub fn get_balance(&self, id: CovenantId, pubkey: Pubkey) -> Result<u64, rocksdb::Error> {
        let key = balance_key(id, pubkey);
        Ok(self.db.get(key)?.map(|v| u64::from_le_bytes(v[..8].try_into().expect("8 bytes"))).unwrap_or(0))
    }

    /// Find account by first-byte index. Prefix scan on covenant_id(32) + index(1).
    /// Returns the first match as (pubkey, balance).
    pub fn get_balance_by_index(&self, id: CovenantId, index: u8) -> Result<Option<(Pubkey, u64)>, rocksdb::Error> {
        let mut scan = Vec::with_capacity(PREFIX_BALANCE.len() + 33);
        scan.extend_from_slice(PREFIX_BALANCE);
        scan.extend_from_slice(&id.as_bytes());
        scan.push(index);

        let mut iter = self.db.prefix_iterator(&scan);
        if let Some(item) = iter.next() {
            let (k, v) = item.expect("db iterator");
            if k.starts_with(&scan) {
                let pubkey = Hash::from_slice(&k[PREFIX_BALANCE.len() + 32..]);
                let balance = u64::from_le_bytes(v.as_ref().try_into().expect("8 bytes"));
                return Ok(Some((pubkey, balance)));
            }
        }
        Ok(None)
    }

    /// List all balances for a covenant. Returns (pubkey, balance) pairs.
    pub fn list_balances(&self, id: CovenantId) -> Vec<(Pubkey, u64)> {
        let scan = prefix_key(PREFIX_BALANCE, &id.as_bytes());
        let iter = self.db.prefix_iterator(&scan);
        let mut results = Vec::new();
        for item in iter {
            let (k, v) = item.expect("db iterator");
            if !k.starts_with(&scan) {
                break;
            }
            let pubkey = Hash::from_slice(&k[scan.len()..]);
            let balance = u64::from_le_bytes(v.as_ref().try_into().expect("8 bytes"));
            results.push((pubkey, balance));
        }
        results
    }

    // ── Account private keys ──

    /// Store an account's private key. Key = covenant_id(32) + pubkey(32).
    /// Value = raw private key bytes (32B).
    pub fn put_account_key(&self, id: CovenantId, pubkey: Pubkey, privkey: &[u8; 32]) -> Result<(), rocksdb::Error> {
        let key = account_key(id, pubkey);
        self.db.put(key, privkey)
    }

    /// Get an account's private key by covenant + pubkey.
    pub fn get_account_key(&self, id: CovenantId, pubkey: Pubkey) -> Result<Option<[u8; 32]>, rocksdb::Error> {
        let key = account_key(id, pubkey);
        Ok(self.db.get(key)?.map(|v| <[u8; 32]>::try_from(&v[..32]).expect("32-byte privkey")))
    }

    /// Find account private key by first-byte index.
    /// Returns (pubkey, privkey) if found.
    #[allow(clippy::type_complexity)]
    pub fn get_account_key_by_index(&self, id: CovenantId, index: u8) -> Result<Option<(Pubkey, [u8; 32])>, rocksdb::Error> {
        let mut scan = Vec::with_capacity(PREFIX_ACCOUNT.len() + 33);
        scan.extend_from_slice(PREFIX_ACCOUNT);
        scan.extend_from_slice(&id.as_bytes());
        scan.push(index);

        let mut iter = self.db.prefix_iterator(&scan);
        if let Some(item) = iter.next() {
            let (k, v) = item.expect("db iterator");
            if k.starts_with(&scan) {
                let pubkey = Hash::from_slice(&k[PREFIX_ACCOUNT.len() + 32..]);
                let privkey: [u8; 32] = v.as_ref().try_into().expect("32-byte privkey");
                return Ok(Some((pubkey, privkey)));
            }
        }
        Ok(None)
    }

    /// List all accounts for a covenant. Returns (pubkey, privkey) pairs.
    pub fn list_accounts(&self, id: CovenantId) -> Vec<(Pubkey, [u8; 32])> {
        let scan = prefix_key(PREFIX_ACCOUNT, &id.as_bytes());
        let iter = self.db.prefix_iterator(&scan);
        let mut results = Vec::new();
        for item in iter {
            let (k, v) = item.expect("db iterator");
            if !k.starts_with(&scan) {
                break;
            }
            let pubkey = Hash::from_slice(&k[scan.len()..]);
            let privkey: [u8; 32] = v.as_ref().try_into().expect("32-byte privkey");
            results.push((pubkey, privkey));
        }
        results
    }

    // ── UTXO records ──

    pub fn put_utxos(&self, address: &str, utxos: &[UtxoRecord]) -> Result<(), rocksdb::Error> {
        let key = prefix_key(PREFIX_UTXO, address.as_bytes());
        let val = borsh::to_vec(utxos).expect("borsh serialize");
        self.db.put(key, val)
    }

    pub fn get_utxos(&self, address: &str) -> Result<Vec<UtxoRecord>, rocksdb::Error> {
        let key = prefix_key(PREFIX_UTXO, address.as_bytes());
        Ok(self.db.get(key)?.map(|v| Vec::<UtxoRecord>::try_from_slice(&v).expect("borsh deserialize")).unwrap_or_default())
    }

    // ── Proving state ──

    pub fn put_proving_state(&self, id: CovenantId, state: &ProvingState) -> Result<(), rocksdb::Error> {
        let key = prefix_key(PREFIX_PROVING, &id.as_bytes());
        let val = borsh::to_vec(state).expect("borsh serialize");
        self.db.put(key, val)
    }

    pub fn get_proving_state(&self, id: CovenantId) -> Result<Option<ProvingState>, rocksdb::Error> {
        let key = prefix_key(PREFIX_PROVING, &id.as_bytes());
        Ok(self.db.get(key)?.map(|v| ProvingState::try_from_slice(&v).expect("borsh deserialize")))
    }
}

fn prefix_key(prefix: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(prefix.len() + suffix.len());
    key.extend_from_slice(prefix);
    key.extend_from_slice(suffix);
    key
}

fn balance_key(id: CovenantId, pubkey: Pubkey) -> Vec<u8> {
    let mut key = Vec::with_capacity(PREFIX_BALANCE.len() + 64);
    key.extend_from_slice(PREFIX_BALANCE);
    key.extend_from_slice(&id.as_bytes());
    key.extend_from_slice(&pubkey.as_bytes());
    key
}

fn account_key(id: CovenantId, pubkey: Pubkey) -> Vec<u8> {
    let mut key = Vec::with_capacity(PREFIX_ACCOUNT.len() + 64);
    key.extend_from_slice(PREFIX_ACCOUNT);
    key.extend_from_slice(&id.as_bytes());
    key.extend_from_slice(&pubkey.as_bytes());
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_covenant_record() {
        let dir = tempfile::tempdir().unwrap();
        let db = RollupDb::open(dir.path()).unwrap();

        let id = Hash::from_bytes([0xAA; 32]);
        let record =
            CovenantRecord { deployer_privkey: vec![1, 2, 3], deployment_tx_id: None, covenant_utxo: None, created_at: 1234567890 };

        db.put_covenant(id, &record).unwrap();
        let loaded = db.get_covenant(id).unwrap().unwrap();
        assert_eq!(loaded.deployer_privkey, record.deployer_privkey);
        assert_eq!(loaded.created_at, 1234567890);
    }

    #[test]
    fn balance_put_get() {
        let dir = tempfile::tempdir().unwrap();
        let db = RollupDb::open(dir.path()).unwrap();

        let cov = Hash::from_bytes([0xFF; 32]);
        let mut pk_bytes = [0u8; 32];
        pk_bytes[0] = 42; // index byte = 42
        let pubkey = Hash::from_bytes(pk_bytes);

        db.put_balance(cov, pubkey, 1000).unwrap();
        assert_eq!(db.get_balance(cov, pubkey).unwrap(), 1000);

        // Missing key returns 0
        assert_eq!(db.get_balance(cov, Hash::from_bytes([0x99; 32])).unwrap(), 0);
    }

    #[test]
    fn balance_lookup_by_index() {
        let dir = tempfile::tempdir().unwrap();
        let db = RollupDb::open(dir.path()).unwrap();

        let cov = Hash::from_bytes([0xFF; 32]);

        let mut bytes_a = [0u8; 32];
        bytes_a[0] = 10; // index 10
        bytes_a[1] = 0xAA;
        let pk_a = Hash::from_bytes(bytes_a);

        let mut bytes_b = [0u8; 32];
        bytes_b[0] = 20; // index 20
        bytes_b[1] = 0xBB;
        let pk_b = Hash::from_bytes(bytes_b);

        db.put_balance(cov, pk_a, 500).unwrap();
        db.put_balance(cov, pk_b, 700).unwrap();

        // Find by index
        let (found_pk, found_bal) = db.get_balance_by_index(cov, 10).unwrap().unwrap();
        assert_eq!(found_pk, pk_a);
        assert_eq!(found_bal, 500);

        let (found_pk, found_bal) = db.get_balance_by_index(cov, 20).unwrap().unwrap();
        assert_eq!(found_pk, pk_b);
        assert_eq!(found_bal, 700);

        // Missing index
        assert!(db.get_balance_by_index(cov, 99).unwrap().is_none());
    }

    #[test]
    fn account_key_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = RollupDb::open(dir.path()).unwrap();

        let cov = Hash::from_bytes([0xFF; 32]);
        let mut pk_bytes = [0u8; 32];
        pk_bytes[0] = 42;
        pk_bytes[1] = 0xDE;
        let pubkey = Hash::from_bytes(pk_bytes);
        let privkey = [0xAB; 32];

        db.put_account_key(cov, pubkey, &privkey).unwrap();
        let loaded = db.get_account_key(cov, pubkey).unwrap().unwrap();
        assert_eq!(loaded, privkey);

        // By index
        let (found_pk, found_sk) = db.get_account_key_by_index(cov, 42).unwrap().unwrap();
        assert_eq!(found_pk, pubkey);
        assert_eq!(found_sk, privkey);
    }

    #[test]
    fn list_balances_and_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let db = RollupDb::open(dir.path()).unwrap();

        let cov = Hash::from_bytes([0xFF; 32]);

        let mut bytes1 = [0u8; 32];
        bytes1[0] = 10;
        let pk1 = Hash::from_bytes(bytes1);
        let mut bytes2 = [0u8; 32];
        bytes2[0] = 20;
        let pk2 = Hash::from_bytes(bytes2);

        db.put_balance(cov, pk1, 100).unwrap();
        db.put_balance(cov, pk2, 200).unwrap();
        db.put_account_key(cov, pk1, &[0x11; 32]).unwrap();
        db.put_account_key(cov, pk2, &[0x22; 32]).unwrap();

        let balances = db.list_balances(cov);
        assert_eq!(balances.len(), 2);

        let accounts = db.list_accounts(cov);
        assert_eq!(accounts.len(), 2);
    }

    #[test]
    fn roundtrip_utxos() {
        let dir = tempfile::tempdir().unwrap();
        let db = RollupDb::open(dir.path()).unwrap();

        let utxos = vec![
            UtxoRecord { outpoint: (Hash::from_bytes([0x11; 32]), 0), amount: 100_000, spk: vec![0u8; 34] },
            UtxoRecord { outpoint: (Hash::from_bytes([0x22; 32]), 1), amount: 200_000, spk: vec![0u8; 34] },
        ];

        db.put_utxos("kaspatest:qqtest", &utxos).unwrap();
        let loaded = db.get_utxos("kaspatest:qqtest").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].amount, 100_000);
        assert_eq!(loaded[1].amount, 200_000);
    }

    #[test]
    fn delete_covenant() {
        let dir = tempfile::tempdir().unwrap();
        let db = RollupDb::open(dir.path()).unwrap();

        let id = Hash::from_bytes([0xBB; 32]);
        let record =
            CovenantRecord { deployer_privkey: vec![1, 2, 3], deployment_tx_id: None, covenant_utxo: None, created_at: 1234567890 };

        db.put_covenant(id, &record).unwrap();
        assert!(db.get_covenant(id).unwrap().is_some());

        db.delete_covenant(id).unwrap();
        assert!(db.get_covenant(id).unwrap().is_none());
    }

    #[test]
    fn list_covenants_empty() {
        let dir = tempfile::tempdir().unwrap();
        let db = RollupDb::open(dir.path()).unwrap();
        assert!(db.list_covenants().is_empty());
    }
}
