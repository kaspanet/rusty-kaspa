use kaspa_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use std::{fmt::Display, sync::Arc};

use crate::{AcceptingBlueScore, AcceptingBlueScoreDiff, AcceptingBlueScoreHashPair};

/// Some notes on the [`AcceptingBlueScoreKey`]:
/// 1) Big endian is important for the ordering of the keys. as it allows us to iterate ranges.
/// 2) Rocks-db does not play well with 0 byte values, so we increment by 1 before converting it to bytes,
/// and vice-versa, when converting from bytes to [`AcceptingBlueScore`], decrement by 1. This ensures we do not have 0 byte values.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct AcceptingBlueScoreKey([u8; std::mem::size_of::<AcceptingBlueScore>()]);

impl From<AcceptingBlueScore> for AcceptingBlueScoreKey {
    fn from(accepting_blue_score: AcceptingBlueScore) -> Self {
        Self((accepting_blue_score + 1).to_be_bytes()) // Big endian  is important for the ordering of the keys.
    }
}

impl From<AcceptingBlueScoreKey> for AcceptingBlueScore {
    fn from(accepting_blue_score_key: AcceptingBlueScoreKey) -> Self {
        AcceptingBlueScore::from_be_bytes(accepting_blue_score_key.0) - 1 // Big endian  is important for the ordering of the keys.
    }
}

impl From<&AcceptingBlueScore> for AcceptingBlueScoreKey {
    fn from(accepting_blue_score: &AcceptingBlueScore) -> Self {
        Self((accepting_blue_score + 1).to_be_bytes()) // Big endian  is important for the ordering of the keys.
    }
}

impl AsRef<[u8]> for AcceptingBlueScoreKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for AcceptingBlueScoreKey {
    type Error = StoreError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != std::mem::size_of::<AcceptingBlueScore>() {
            return Err(StoreError::InvalidValueLength(value.len(), std::mem::size_of::<AcceptingBlueScore>()));
        }
        let mut bytes = [0; std::mem::size_of::<AcceptingBlueScore>()];
        bytes.copy_from_slice(value);
        Ok(Self(bytes))
    }
}

impl Display for AcceptingBlueScoreKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AcceptingBlueScoreKey({0:?})", self.0)
    }
}

// Traits:

pub trait ScoreIndexAcceptingBlueScoreReader {
    fn get(&self, accepting_blue_score: AcceptingBlueScore) -> StoreResult<Hash>;
    fn get_sink(&self) -> StoreResult<AcceptingBlueScoreHashPair>;
    fn get_source(&self) -> StoreResult<AcceptingBlueScoreHashPair>;
    fn get_range(&self, from: AcceptingBlueScore, to: AcceptingBlueScore) -> StoreResult<Vec<AcceptingBlueScoreHashPair>>;
    fn has(&self, accepting_blue_score: AcceptingBlueScore) -> StoreResult<bool>;

    //For tests only:
    fn get_all(&self) -> StoreResult<Vec<AcceptingBlueScoreHashPair>>;
}

pub trait ScoreIndexAcceptingBlueScoreStore {
    fn write_diff(&mut self, batch: &mut WriteBatch, diff: AcceptingBlueScoreDiff) -> StoreResult<()>;
    fn remove_many(&mut self, batch: &mut WriteBatch, to_remove: Vec<AcceptingBlueScore>) -> StoreResult<()>;
    fn write_many(&mut self, batch: &mut WriteBatch, to_add: Vec<AcceptingBlueScoreHashPair>) -> StoreResult<()>;
    fn delete_all(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
}

// Implementations:

#[derive(Clone)]
pub struct DbScoreIndexAcceptingBlueScoreStore {
    access: CachedDbAccess<AcceptingBlueScoreKey, Hash>,
}

impl DbScoreIndexAcceptingBlueScoreStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::AcceptingBlueScore.into()) }
    }
}

impl ScoreIndexAcceptingBlueScoreReader for DbScoreIndexAcceptingBlueScoreStore {
    fn get(&self, accepting_blue_score: AcceptingBlueScore) -> StoreResult<Hash> {
        self.access.read(accepting_blue_score.into())
    }

    fn has(&self, accepting_blue_score: AcceptingBlueScore) -> StoreResult<bool> {
        self.access.has(accepting_blue_score.into())
    }

    fn get_range(&self, from: AcceptingBlueScore, to: AcceptingBlueScore) -> StoreResult<Vec<AcceptingBlueScoreHashPair>> {
        self.access
            .seek_iterator(
                None,
                Some(from.into()),
                Some((to + 1).into()), // The `+ 1` is to make the range inclusive.
                usize::MAX,
                false,
            )
            .map(|res| match res {
                Ok((k, v)) => Ok(AcceptingBlueScoreHashPair {
                    accepting_blue_score: AcceptingBlueScoreKey::try_from(k.as_ref())?.into(),
                    hash: v,
                }),
                Err(err) => Err(StoreError::Undefined(err.to_string())),
            })
            .collect::<StoreResult<Vec<AcceptingBlueScoreHashPair>>>()
    }

    fn get_sink(&self) -> StoreResult<AcceptingBlueScoreHashPair> {
        let ret = self.access.iterator(true).next();
        if let Some(res) = ret {
            let (k, v) = res.map_err(|err| StoreError::Undefined(err.to_string()))?;
            return Ok(AcceptingBlueScoreHashPair {
                accepting_blue_score: AcceptingBlueScoreKey::try_from(k.as_ref())?.into(),
                hash: v,
            });
        }
        Err(StoreError::DbEmptyError) // this is the only explanation for not finding the sink
    }

    fn get_source(&self) -> StoreResult<AcceptingBlueScoreHashPair> {
        let ret = self.access.iterator(false).next();
        if let Some(res) = ret {
            let (k, v) = res.map_err(|err| StoreError::Undefined(err.to_string()))?;
            return Ok(AcceptingBlueScoreHashPair {
                accepting_blue_score: AcceptingBlueScoreKey::try_from(k.as_ref())?.into(),
                hash: v,
            });
        }
        Err(StoreError::DbEmptyError) // this is the only explanation for not finding the source
    }

    fn get_all(&self) -> StoreResult<Vec<AcceptingBlueScoreHashPair>> {
        self.access
            .iterator(false)
            .map(|res| match res {
                Ok((k, v)) => Ok(AcceptingBlueScoreHashPair {
                    accepting_blue_score: AcceptingBlueScoreKey::try_from(k.as_ref())?.into(),
                    hash: v,
                }),
                Err(err) => Err(StoreError::Undefined(err.to_string())),
            })
            .collect::<StoreResult<Vec<AcceptingBlueScoreHashPair>>>()
    }
}

impl ScoreIndexAcceptingBlueScoreStore for DbScoreIndexAcceptingBlueScoreStore {
    fn write_diff(&mut self, batch: &mut WriteBatch, diff: AcceptingBlueScoreDiff) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_many(&mut writer, &mut diff.to_remove.iter().map(|k| k.into()))?;
        self.access.write_many(&mut writer, &mut diff.to_add.iter().map(|pair| (pair.accepting_blue_score.into(), pair.hash)))?;
        Ok(())
    }

    fn remove_many(&mut self, batch: &mut WriteBatch, to_remove: Vec<AcceptingBlueScore>) -> StoreResult<()> {
        let writer = BatchDbWriter::new(batch);
        self.access.delete_many(writer, &mut to_remove.iter().map(|k| k.into()))
    }

    fn write_many(&mut self, batch: &mut WriteBatch, to_add: Vec<AcceptingBlueScoreHashPair>) -> StoreResult<()> {
        let writer = BatchDbWriter::new(batch);
        self.access.write_many(writer, &mut to_add.iter().map(|pair| (pair.accepting_blue_score.into(), pair.hash)))
    }

    fn delete_all(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let writer = BatchDbWriter::new(batch);
        self.access.delete_all(writer)
    }
}
