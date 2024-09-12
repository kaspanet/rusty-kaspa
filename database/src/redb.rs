use crate::access::DbAccess;
use crate::errors::StoreError;
use crate::key::DbKey;
use crate::prelude::DbWriter;
use itertools::Either;
use redb::{ReadableTable, TableDefinition};
use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};

const TABLE: TableDefinition<&[u8], Vec<u8>> = TableDefinition::new("0");

pub struct Redb {
    db: redb::Database,
    write_queue_count: AtomicU64,
}

impl DbAccess for Redb {
    fn has(&self, db_key: DbKey) -> Result<bool, StoreError> {
        Ok(|| -> Result<_, redb::Error> { Ok(self.db.begin_read()?.open_table(TABLE)?.get(db_key.as_ref())?.is_some()) }()?)
    }

    fn read(&self, db_key: &DbKey) -> Result<Option<impl AsRef<[u8]>>, StoreError> {
        Ok(|| -> Result<_, redb::Error> {
            Ok(self.db.begin_read()?.open_table(TABLE)?.get(db_key.as_ref())?.map(|guard| guard.value()))
        }()?)
    }

    fn iterator(
        &self,
        prefix: impl Into<Vec<u8>>,
        seek_from: Option<DbKey>,
    ) -> impl Iterator<Item = Result<(impl AsRef<[u8]>, impl AsRef<[u8]>), Box<dyn Error>>> + '_ {
        let prefix = prefix.into();
        seek_from.as_ref().inspect(|seek_from| debug_assert!(seek_from.as_ref().starts_with(prefix.as_ref())));

        let table = self.db.begin_read().unwrap().open_table(TABLE).unwrap(); // todo change interface to support errors
        Iterator::map(
            {
                if let Some(seek_from) = seek_from {
                    Either::Left(table.range(seek_from.as_ref()..).unwrap()) // todo change interface to support errors
                } else {
                    Either::Right(table.range(prefix.as_slice()..).unwrap()) // todo change interface to support errors
                }
            }
            .take_while(move |r| r.as_ref().is_ok_and(|(k, _)| k.value().starts_with(&prefix))),
            |r| r.map(|(k, v)| (k.value().to_vec(), v.value())).map_err(Into::into),
        )
    }

    fn write(&self, _writer: &mut impl DbWriter, db_key: DbKey, data: Vec<u8>) -> Result<(), StoreError> {
        let process = || -> Result<_, redb::Error> {
            let write_tx = self.db.begin_write()?;
            let mut table = write_tx.open_table(TABLE)?;
            table.insert(db_key.as_ref(), data)?;
            Ok(())
        };
        self.write_queue_count.fetch_add(1, Ordering::Relaxed);
        let res = process();
        self.write_queue_count.fetch_sub(1, Ordering::Relaxed);
        Ok(res?)
    }

    fn delete(&self, _writer: &mut impl DbWriter, db_key: DbKey) -> Result<(), StoreError> {
        let process = || -> Result<_, redb::Error> {
            let write_tx = self.db.begin_write()?;
            let mut table = write_tx.open_table(TABLE)?;
            table.remove(db_key.as_ref())?;
            Ok(())
        };
        self.write_queue_count.fetch_add(1, Ordering::Relaxed);
        let res = process();
        self.write_queue_count.fetch_sub(1, Ordering::Relaxed);
        Ok(res?)
    }

    fn delete_range_by_prefix(&self, writer: &mut impl DbWriter, prefix: &[u8]) -> Result<(), StoreError> {
        todo!()
    }
}
