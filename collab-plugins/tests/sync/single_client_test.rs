use collab::preclude::MapRefExtension;
use serde_json::json;

use crate::util::{
  make_test_collab_group, spawn_client, spawn_client_with_disk, spawn_server,
  spawn_server_with_data, wait_a_sec,
};

#[tokio::test]
async fn send_single_update_to_server_test() {
  let uid = 1;
  let object_id = "1";
  let server = spawn_server(uid, object_id).await.unwrap();
  let client = spawn_client(uid, object_id, server.address).await.unwrap();

  wait_a_sec().await;
  client.lock().collab.insert("1", "a");
  wait_a_sec().await;

  let json1 = client.to_json_value();
  assert_json_diff::assert_json_eq!(
    json1,
    json!( {
      "1": "a"
    })
  );

  let json2 = server.get_doc_json(object_id);
  assert_eq!(json1, json2);
}

#[tokio::test]
async fn send_multiple_updates_to_server_test() {
  let uid = 1;
  let object_id = "1";
  let server = spawn_server(uid, object_id).await.unwrap();
  let client = spawn_client(uid, object_id, server.address).await.unwrap();
  wait_a_sec().await;
  {
    let client = client.lock();
    client.collab.with_transact_mut(|txn| {
      let map = client.collab.create_map_with_txn(txn, "map");
      map.insert_with_txn(txn, "task1", "a");
      map.insert_with_txn(txn, "task2", "b");
    });
  }
  wait_a_sec().await;
  {
    let client = client.lock();
    client.collab.with_transact_mut(|txn| {
      let map = client.collab.get_map_with_txn(txn, vec!["map"]).unwrap();
      map.insert_with_txn(txn, "task3", "c");
    });
  }
  wait_a_sec().await;

  let json = server.get_doc_json(object_id);
  assert_json_diff::assert_json_eq!(
    json,
    json!( {
      "map": {
        "task1": "a",
        "task2": "b",
        "task3": "c"
      }
    })
  );
}

#[tokio::test]
async fn fetch_initial_state_from_server_test() {
  let uid = 1;
  let object_id = "1";

  let group = make_test_collab_group(uid, object_id).await;
  group.mut_collab(|collab| {
    collab.insert("1", "a");
  });
  let server = spawn_server_with_data(group).await.unwrap();
  let client = spawn_client(uid, object_id, server.address).await.unwrap();
  wait_a_sec().await;

  let json = client.to_json_value();
  assert_json_diff::assert_json_eq!(
    json,
    json!( {
      "1": "a"
    })
  );
}

#[tokio::test]
async fn send_local_doc_initial_state_to_server() {
  let uid = 1;
  let object_id = "1";

  let server = spawn_server(uid, object_id).await.unwrap();
  let (_db, client) = spawn_client_with_disk(uid, object_id, server.address, None)
    .await
    .unwrap();
  wait_a_sec().await;
  {
    let client = client.lock();
    client.collab.with_transact_mut(|txn| {
      let map = client.collab.create_map_with_txn(txn, "map");
      map.insert_with_txn(txn, "task1", "a");
      map.insert_with_txn(txn, "task2", "b");
    });
  }
  wait_a_sec().await;
  let json = server.get_doc_json(object_id);
  assert_json_diff::assert_json_eq!(
    json,
    json!( {
      "map": {
        "task1": "a",
        "task2": "b"
      }
    })
  );
}

#[tokio::test]
async fn send_local_doc_initial_state_to_server_multiple_times() {
  let uid = 1;
  let object_id = "1";

  let server = spawn_server(uid, object_id).await.unwrap();
  let (db, client) = spawn_client_with_disk(uid, object_id, server.address, None)
    .await
    .unwrap();
  wait_a_sec().await;
  {
    let client = client.lock();
    client.collab.with_transact_mut(|txn| {
      let map = client.collab.create_map_with_txn(txn, "map");
      map.insert_with_txn(txn, "task1", "a");
      map.insert_with_txn(txn, "task2", "b");
    });
  }
  wait_a_sec().await;

  let remote_doc_json = server.get_doc_json(object_id);

  for _i in 0..3 {
    let (_, _client) = spawn_client_with_disk(uid, object_id, server.address, Some(db.clone()))
      .await
      .unwrap();
    wait_a_sec().await;
    assert_eq!(remote_doc_json, server.get_doc_json(object_id));
  }
}
