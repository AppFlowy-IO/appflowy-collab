use crate::util::create_folder_with_workspace;
use collab_folder::core::{TrashChange, TrashChangeReceiver, TrashRecord};
use std::future::Future;
use std::time::Duration;

#[test]
fn create_trash_test() {
  let folder_test = create_folder_with_workspace("1", "w1");
  folder_test.trash.add_trash(vec![
    TrashRecord {
      id: "1".to_string(),
      created_at: 0,
    },
    TrashRecord {
      id: "2".to_string(),
      created_at: 0,
    },
    TrashRecord {
      id: "3".to_string(),
      created_at: 0,
    },
  ]);

  let trash = folder_test.trash.get_all_trash();
  assert_eq!(trash.len(), 3);
  assert_eq!(trash[0].id, "1");
  assert_eq!(trash[1].id, "2");
  assert_eq!(trash[2].id, "3");
}

#[tokio::test]
async fn delete_trash_test() {
  let folder_test = create_folder_with_workspace("1", "w1");
  folder_test.trash.add_trash(vec![
    TrashRecord {
      id: "1".to_string(),
      created_at: 0,
    },
    TrashRecord {
      id: "2".to_string(),
      created_at: 0,
    },
  ]);

  let trash = folder_test.trash.get_all_trash();
  assert_eq!(trash[0].id, "1");
  assert_eq!(trash[1].id, "2");

  folder_test.trash.delete_trash(vec!["1"]);
  let trash = folder_test.trash.get_all_trash();
  assert_eq!(trash[0].id, "2");
}

#[tokio::test]
async fn create_trash_callback_test() {
  let mut folder_test = create_folder_with_workspace("1", "w1");
  let trash_rx = folder_test.trash_rx.take().unwrap();
  tokio::spawn(async move {
    folder_test.trash.add_trash(vec![
      TrashRecord {
        id: "1".to_string(),
        created_at: 0,
      },
      TrashRecord {
        id: "2".to_string(),
        created_at: 0,
      },
    ]);
  });

  timeout(poll_tx(trash_rx, |change| match change {
    TrashChange::DidCreateTrash { ids } => {
      assert_eq!(ids, vec!["1", "2"]);
    },
    TrashChange::DidDeleteTrash { .. } => {},
  }))
  .await;
}

#[tokio::test]
async fn delete_trash_callback_test() {
  let mut folder_test = create_folder_with_workspace("1", "w1");
  let trash_rx = folder_test.trash_rx.take().unwrap();
  tokio::spawn(async move {
    folder_test.trash.add_trash(vec![
      TrashRecord {
        id: "1".to_string(),
        created_at: 0,
      },
      TrashRecord {
        id: "2".to_string(),
        created_at: 0,
      },
    ]);
    folder_test.trash.delete_trash(vec!["1", "2"]);
  });

  timeout(poll_tx(trash_rx, |change| match change {
    TrashChange::DidCreateTrash { ids } => {
      assert_eq!(ids, vec!["1", "2"]);
    },
    TrashChange::DidDeleteTrash { ids } => {
      assert_eq!(ids, vec!["1", "2"]);
    },
  }))
  .await;
}

async fn poll_tx(mut rx: TrashChangeReceiver, callback: impl Fn(TrashChange)) {
  while let Ok(change) = rx.recv().await {
    callback(change)
  }
}

async fn timeout<F: Future>(f: F) {
  tokio::time::timeout(Duration::from_secs(2), f)
    .await
    .unwrap();
}
