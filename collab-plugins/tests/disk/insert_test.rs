use crate::disk::script::{disk_plugin_with_db, CollabPersistenceTest};
use assert_json_diff::assert_json_eq;

use anyhow::Error;
use collab::preclude::CollabBuilder;
use collab_entity::CollabType;
use collab_plugins::local_storage::kv::doc::CollabKVAction;
use collab_plugins::local_storage::kv::KVTransactionDB;
use collab_plugins::local_storage::rocksdb::util::KVDBCollabPersistenceImpl;
use collab_plugins::local_storage::CollabPersistenceConfig;
use std::sync::Arc;

#[tokio::test]
async fn insert_single_change_and_restore_from_disk() {
  let doc_id = "1".to_string();
  let mut test = CollabPersistenceTest::new(CollabPersistenceConfig::new());
  let db = test.db.clone();

  // Replacing Script variants with function calls
  test
    .create_document_with_collab_db(doc_id.clone(), db.clone())
    .await;
  test
    .insert_key_value(doc_id.clone(), "1".to_string(), "a".into())
    .await;
  test.close_document(doc_id.clone()).await;
  test.open_document_with_disk_plugin(doc_id.clone()).await;
  test
    .get_value(doc_id, "1".to_string(), Some("a".into()))
    .await;
}

#[tokio::test]
async fn flush_test() {
  let doc_id = "1".to_string();
  let test = CollabPersistenceTest::new(CollabPersistenceConfig::new());
  let disk_plugin = disk_plugin_with_db(
    test.uid,
    test.workspace_id.clone(),
    test.db.clone(),
    &doc_id,
    CollabType::Unknown,
  );
  let data_source = KVDBCollabPersistenceImpl {
    db: Arc::downgrade(&test.db),
    uid: 1,
    workspace_id: test.workspace_id.clone(),
  };

  let mut collab = CollabBuilder::new(1, &doc_id, data_source.into())
    .with_device_id("1")
    .with_plugin(disk_plugin)
    .build()
    .unwrap();
  collab.initialize();

  for i in 0..100 {
    collab.insert(&i.to_string(), i.to_string());
  }
  let before_flush_value = collab.to_json_value();

  let read = test.db.read_txn();
  let before_flush_updates = read
    .get_all_updates(test.uid, &test.workspace_id, &doc_id)
    .unwrap();
  let write_txn = test.db.write_txn();
  let encode_collab = collab.encode_collab_v1(|_| Ok::<(), Error>(())).unwrap();
  write_txn
    .flush_doc(
      test.uid,
      &test.workspace_id,
      &doc_id,
      encode_collab.state_vector.to_vec(),
      encode_collab.doc_state.to_vec(),
    )
    .unwrap();
  write_txn.commit_transaction().unwrap();

  let after_flush_updates = read
    .get_all_updates(test.uid, &test.workspace_id, &doc_id)
    .unwrap();

  let after_flush_value = collab.to_json_value();
  assert_eq!(before_flush_updates.len(), 100);
  assert_eq!(after_flush_updates.len(), 0);
  assert_json_eq!(before_flush_value, after_flush_value);
}

#[tokio::test]
async fn insert_multiple_changes_and_restore_from_disk() {
  let mut test = CollabPersistenceTest::new(CollabPersistenceConfig::new());
  let doc_id = "1".to_string();
  let db = test.db.clone();

  // Replacing Script variants with function calls
  test
    .create_document_with_collab_db(doc_id.clone(), db.clone())
    .await;
  test
    .insert_key_value(doc_id.clone(), "1".to_string(), "a".into())
    .await;
  test
    .insert_key_value(doc_id.clone(), "2".to_string(), "b".into())
    .await;
  test
    .insert_key_value(doc_id.clone(), "3".to_string(), "c".into())
    .await;
  test
    .insert_key_value(doc_id.clone(), "4".to_string(), "d".into())
    .await;
  test.assert_update_len(doc_id.clone(), 4).await;
  test.close_document(doc_id.clone()).await;
  test.open_document_with_disk_plugin(doc_id.clone()).await;
  test
    .get_value(doc_id.clone(), "1".to_string(), Some("a".into()))
    .await;
  test
    .get_value(doc_id.clone(), "2".to_string(), Some("b".into()))
    .await;
  test
    .get_value(doc_id.clone(), "3".to_string(), Some("c".into()))
    .await;
  test
    .get_value(doc_id, "4".to_string(), Some("d".into()))
    .await;
}

#[tokio::test]
async fn insert_multiple_docs() {
  let mut test = CollabPersistenceTest::new(CollabPersistenceConfig::new());
  let db = test.db.clone();

  // Replacing Script variants with function calls
  test
    .create_document_with_collab_db("1".to_string(), db.clone())
    .await;
  test
    .create_document_with_collab_db("2".to_string(), db.clone())
    .await;
  test
    .create_document_with_collab_db("3".to_string(), db.clone())
    .await;
  test
    .create_document_with_collab_db("4".to_string(), db.clone())
    .await;
  test
    .create_document_with_collab_db("5".to_string(), db.clone())
    .await;
  test
    .create_document_with_collab_db("6".to_string(), db.clone())
    .await;
  test.assert_num_of_documents(6).await;
}
