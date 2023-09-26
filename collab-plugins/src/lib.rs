#[cfg(any(feature = "rocksdb_plugin"))]
pub use collab_persistence::*;

#[cfg(feature = "sync_plugin")]
pub mod sync_plugin;

pub mod local_storage;

#[cfg(feature = "postgres_storage_plugin")]
pub mod cloud_storage;

#[cfg(feature = "snapshot_plugin")]
pub mod snapshot;
