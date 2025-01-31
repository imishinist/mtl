use std::cmp::Ordering;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{fs, thread};

use byteorder::ByteOrder;
use redb::{Database, ReadableTable, RedbKey, RedbValue, TableDefinition, TypeName};

use crate::hash::Hash;
use crate::{filesystem, ObjectID};

#[derive(Debug)]
pub struct CacheKey {
    path: PathBuf,
}

impl CacheKey {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl RedbValue for CacheKey {
    type SelfType<'a> = CacheKey;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        let path = filesystem::path_from_bytes(data);
        Self { path }
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let path = value.path.as_os_str();
        filesystem::osstr_to_bytes(path).to_vec()
    }

    fn type_name() -> TypeName {
        TypeName::new("cache-key")
    }
}

impl RedbKey for CacheKey {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        data1.cmp(data2)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CacheValue {
    pub mtime: u128,
    pub size: u64,
    pub object_id: ObjectID,
}

impl RedbValue for CacheValue {
    type SelfType<'a> = CacheValue;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        Some(16 + 8 + Hash::fixed_width())
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        let mtime = byteorder::LittleEndian::read_u128(&data[0..16]);
        let size = byteorder::LittleEndian::read_u64(&data[16..24]);
        let hash = Hash::new(byteorder::LittleEndian::read_u64(&data[24..32]));
        let object_id = ObjectID::new(hash);
        Self {
            mtime,
            size,
            object_id,
        }
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let mut data = vec![0; 32];
        byteorder::LittleEndian::write_u128(&mut data, value.mtime);
        byteorder::LittleEndian::write_u64(&mut data[16..], value.size);
        byteorder::LittleEndian::write_u64(&mut data[24..], value.object_id.as_u64());
        data
    }

    fn type_name() -> TypeName {
        TypeName::new("cache-value")
    }
}

enum WriteRequest {
    Insert(CacheKey, CacheValue),
    Shutdown,
}

pub const CACHE_TABLE: TableDefinition<CacheKey, CacheValue> = TableDefinition::new("cache-table");

#[derive(Debug)]
pub struct Cache {
    db: Arc<Database>,

    tx: mpsc::Sender<WriteRequest>,
    writer_handle: Option<JoinHandle<()>>,
}

impl Cache {
    pub fn open<P: AsRef<Path>>(cache_path: P, interval: Duration) -> anyhow::Result<Self> {
        let cache_path = cache_path.as_ref();
        let cache_dir = cache_path.parent().unwrap();
        fs::create_dir_all(cache_dir)?;

        let db = if cache_path.exists() {
            Database::open(cache_path)
        } else {
            Database::create(cache_path)
        }?;
        let db = Arc::new(db);

        let (tx, rx) = mpsc::channel::<WriteRequest>();
        let writer_db = db.clone();
        let writer_handle = thread::spawn(move || {
            if let Err(e) = writer_thread_loop(writer_db, rx, interval) {
                log::error!("writer thread error: {:?}", e);
            }
        });
        Ok(Self {
            db,
            tx,
            writer_handle: Some(writer_handle),
        })
    }

    pub fn insert<P: AsRef<Path>>(&self, key: P, value: CacheValue) -> anyhow::Result<()> {
        let key = CacheKey::new(key);
        self.tx.send(WriteRequest::Insert(key, value))?;
        Ok(())
    }

    pub fn get<P: AsRef<Path>>(&self, key: P) -> anyhow::Result<Option<CacheValue>> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(CACHE_TABLE)?;
        let key = CacheKey::new(key);

        let x = match table.get(&key)? {
            Some(value) => Ok(Some(value.value())),
            None => Ok(None),
        };
        x
    }

    fn shutdown_and_join(&mut self) {
        log::info!("shutting down cache thread");
        if self.writer_handle.is_none() {
            return;
        }

        log::info!("sending shutdown request to writer thread");
        let _ = self.tx.send(WriteRequest::Shutdown);

        log::info!("joining writer thread");
        if let Some(handle) = self.writer_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        self.shutdown_and_join();
    }
}

fn writer_thread_loop(
    db: Arc<Database>,
    rx: mpsc::Receiver<WriteRequest>,
    interval: Duration,
) -> anyhow::Result<()> {
    let mut buffer = Vec::new();
    let mut last_flush = Instant::now();

    loop {
        let remaining = interval
            .checked_sub(last_flush.elapsed())
            .unwrap_or(Duration::from_secs(0));

        match rx.recv_timeout(remaining) {
            Ok(req) => match req {
                WriteRequest::Insert(k, v) => {
                    buffer.push((k, v));
                }
                WriteRequest::Shutdown => {
                    flush(&db, &mut buffer)?;
                    break;
                }
            },
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                flush(&db, &mut buffer)?;
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        if last_flush.elapsed() >= interval {
            flush(&db, &mut buffer)?;
            last_flush = Instant::now();
        }
    }
    Ok(())
}

fn flush(db: &Database, buffer: &mut Vec<(CacheKey, CacheValue)>) -> anyhow::Result<()> {
    if buffer.is_empty() {
        return Ok(());
    }

    log::info!("flushing cache buffer with {} entries", buffer.len());
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(CACHE_TABLE)?;
        for (key, value) in buffer.drain(..) {
            table.insert(key, value)?;
        }
    }
    write_txn.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cache.db");
        let cache = super::Cache::open(path, Duration::from_millis(10)).unwrap();

        let expected = super::CacheValue {
            mtime: 1,
            size: 2,
            object_id: super::ObjectID::new(super::Hash::new(3)),
        };
        cache.insert("foo", expected).unwrap();
        thread::sleep(Duration::from_millis(20));

        let actual = cache.get("foo").unwrap().unwrap();
        assert_eq!(expected, actual);

        let expected = super::CacheValue {
            mtime: 2,
            size: 10,
            object_id: super::ObjectID::new(super::Hash::new(6)),
        };
        cache.insert("bar", expected).unwrap();
        cache.insert("foo", expected).unwrap();
        thread::sleep(Duration::from_millis(20));

        let actual = cache.get("bar").unwrap().unwrap();
        assert_eq!(expected, actual);

        let actual = cache.get("foo").unwrap().unwrap();
        assert_eq!(expected, actual);
    }
}
