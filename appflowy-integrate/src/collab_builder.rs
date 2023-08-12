use std::fmt::Debug;
use std::sync::{Arc, Weak};

use anyhow::Error;
use collab::core::collab::{CollabRawData, MutexCollab};
use collab::preclude::CollabBuilder;
use collab_persistence::kv::rocks_kv::RocksCollabDB;
use collab_plugins::cloud_storage::network_state::{CollabNetworkReachability, CollabNetworkState};
use collab_plugins::cloud_storage::postgres::SupabaseDBPlugin;
use collab_plugins::cloud_storage::{CollabObject, CollabType, RemoteCollabStorage};
use collab_plugins::local_storage::rocksdb::RocksdbDiskPlugin;
use collab_plugins::local_storage::CollabPersistenceConfig;
use collab_plugins::snapshot::{CollabSnapshotPlugin, SnapshotPersistence};
use parking_lot::{Mutex, RwLock};

#[derive(Clone, Debug)]
pub enum CollabStorageType {
  Local,
  AWS,
  Supabase,
}

pub trait CollabStorageProvider: Send + Sync + 'static {
  fn storage_type(&self) -> CollabStorageType;
  fn get_storage(
    &self,
    collab_object: &CollabObject,
    storage_type: &CollabStorageType,
  ) -> Option<Arc<dyn RemoteCollabStorage>>;
  fn is_sync_enabled(&self) -> bool;
}

impl<T> CollabStorageProvider for Arc<T>
where
  T: CollabStorageProvider,
{
  fn storage_type(&self) -> CollabStorageType {
    (**self).storage_type()
  }

  fn get_storage(
    &self,
    collab_object: &CollabObject,
    storage_type: &CollabStorageType,
  ) -> Option<Arc<dyn RemoteCollabStorage>> {
    (**self).get_storage(collab_object, storage_type)
  }

  fn is_sync_enabled(&self) -> bool {
    (**self).is_sync_enabled()
  }
}

pub struct AppFlowyCollabBuilder {
  network_reachability: CollabNetworkReachability,
  workspace_id: RwLock<Option<String>>,
  cloud_storage: RwLock<Arc<dyn CollabStorageProvider>>,
  snapshot_persistence: Option<Arc<dyn SnapshotPersistence>>,
  device_id: Mutex<String>,
}

impl AppFlowyCollabBuilder {
  pub fn new<T: CollabStorageProvider>(
    cloud_storage: T,
    snapshot_persistence: Option<Arc<dyn SnapshotPersistence>>,
  ) -> Self {
    Self {
      network_reachability: CollabNetworkReachability::new(),
      workspace_id: Default::default(),
      cloud_storage: RwLock::new(Arc::new(cloud_storage)),
      snapshot_persistence,
      device_id: Default::default(),
    }
  }

  pub fn initialize(&self, workspace_id: String) {
    *self.workspace_id.write() = Some(workspace_id);
  }

  pub fn set_sync_device(&self, device_id: String) {
    *self.device_id.lock() = device_id;
  }

  pub fn update_network(&self, reachable: bool) {
    if reachable {
      self
        .network_reachability
        .set_state(CollabNetworkState::Connected)
    } else {
      self
        .network_reachability
        .set_state(CollabNetworkState::Disconnected)
    }
  }

  /// Create a new collab builder with default config.
  /// The [MutexCollab] will be create if the object is not exist. So, if you need to check
  /// the object is exist or not. You should use the transaction returned by the [read_txn] method of
  /// [RocksCollabDB], and calling [is_exist] method.
  ///
  pub fn build(
    &self,
    uid: i64,
    object_id: &str,
    object_type: CollabType,
    raw_data: CollabRawData,
    db: Weak<RocksCollabDB>,
  ) -> Result<Arc<MutexCollab>, Error> {
    self.build_with_config(
      uid,
      object_id,
      object_type,
      db,
      raw_data,
      &CollabPersistenceConfig::default(),
    )
  }

  /// Create a new collab builder with custom config.
  /// The [MutexCollab] will be create if the object is not exist. So, if you need to check
  /// the object is exist or not. You should use the transaction returned by the [read_txn] method of
  /// [RocksCollabDB], and calling [is_exist] method.
  ///
  pub fn build_with_config(
    &self,
    uid: i64,
    object_id: &str,
    object_type: CollabType,
    collab_db: Weak<RocksCollabDB>,
    collab_raw_data: CollabRawData,
    config: &CollabPersistenceConfig,
  ) -> Result<Arc<MutexCollab>, Error> {
    let collab = Arc::new(
      CollabBuilder::new(uid, object_id)
        .with_raw_data(collab_raw_data)
        .with_plugin(RocksdbDiskPlugin::new_with_config(
          uid,
          collab_db.clone(),
          config.clone(),
        ))
        .with_device_id(self.device_id.lock().clone())
        .build()?,
    );

    let cloud_storage = self.cloud_storage.read();
    let cloud_storage_type = cloud_storage.storage_type();
    match cloud_storage_type {
      CollabStorageType::AWS => {
        #[cfg(feature = "aws_storage_plugin")]
        {
          // let collab_config = CollabPluginConfig::from_env();
          // if let Some(config) = collab_config.aws_config() {
          //   if !config.enable {
          //     std::env::remove_var(AWS_ACCESS_KEY_ID);
          //     std::env::remove_var(AWS_SECRET_ACCESS_KEY);
          //   } else {
          //     std::env::set_var(AWS_ACCESS_KEY_ID, &config.access_key_id);
          //     std::env::set_var(AWS_SECRET_ACCESS_KEY, &config.secret_access_key);
          //     let plugin = AWSDynamoDBPlugin::new(
          //       object_id.to_string(),
          //       Arc::downgrade(&collab),
          //       10,
          //       config.region.clone(),
          //     );
          //     collab.lock().add_plugin(Arc::new(plugin));
          //     // tracing::debug!("add aws plugin: {:?}", cloud_storage_type);
          //   }
          // }
        }
      },
      CollabStorageType::Supabase => {
        #[cfg(feature = "postgres_storage_plugin")]
        {
          let workspace_id = self.workspace_id.read().clone().ok_or_else(|| {
            anyhow::anyhow!("When using supabase plugin, the workspace_id should not be empty")
          })?;
          let collab_object = CollabObject::new(uid, object_id.to_string(), object_type.clone())
            .with_workspace_id(workspace_id)
            .with_device_id(self.device_id.lock().clone());
          let local_collab_storage = collab_db.clone();
          if let Some(remote_collab_storage) =
            cloud_storage.get_storage(&collab_object, &cloud_storage_type)
          {
            let plugin = SupabaseDBPlugin::new(
              uid,
              collab_object,
              Arc::downgrade(&collab),
              1,
              remote_collab_storage,
              local_collab_storage,
            );
            collab.lock().add_plugin(Arc::new(plugin));
          }
        }
      },
      CollabStorageType::Local => {},
    }

    if let Some(snapshot_persistence) = &self.snapshot_persistence {
      if config.enable_snapshot {
        let collab_object = CollabObject::new(uid, object_id.to_string(), object_type)
          .with_device_id(self.device_id.lock().clone());
        let snapshot_plugin = CollabSnapshotPlugin::new(
          uid,
          collab_object,
          snapshot_persistence.clone(),
          collab_db,
          config.snapshot_per_update,
        );
        // tracing::trace!("add snapshot plugin: {}", object_id);
        collab.lock().add_plugin(Arc::new(snapshot_plugin));
      }
    }

    collab.lock().initialize();
    Ok(collab)
  }
}

pub struct DefaultCollabStorageProvider();
impl CollabStorageProvider for DefaultCollabStorageProvider {
  fn storage_type(&self) -> CollabStorageType {
    CollabStorageType::Local
  }

  fn get_storage(
    &self,
    _collab_object: &CollabObject,
    _storage_type: &CollabStorageType,
  ) -> Option<Arc<dyn RemoteCollabStorage>> {
    None
  }

  fn is_sync_enabled(&self) -> bool {
    false
  }
}
