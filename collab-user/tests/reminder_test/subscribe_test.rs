use collab_user::core::{Reminder, ReminderChange};

use crate::util::{receive_with_timeout, UserAwarenessTest};

#[tokio::test]
async fn subscribe_insert_reminder_test() {
  let test = UserAwarenessTest::new(1);
  let mut rx = test.reminder_change_tx.subscribe();
  let reminder = Reminder::new("1".to_string(), 123, 0);
  let cloned_test = test.clone();
  let cloned_reminder = reminder.clone();
  tokio::spawn(async move {
    cloned_test.lock().add_reminder(cloned_reminder);
  });

  let change = receive_with_timeout(&mut rx, std::time::Duration::from_secs(2))
    .await
    .unwrap();
  match change {
    ReminderChange::DidCreateReminders { reminders } => {
      assert_eq!(reminders.len(), 1);
      assert_eq!(reminders[0], reminder);
    },
    _ => panic!("Expected DidCreateReminders"),
  }
}

#[tokio::test]
async fn subscribe_delete_reminder_test() {
  let test = UserAwarenessTest::new(1);
  let mut rx = test.reminder_change_tx.subscribe();
  for i in 0..5 {
    let reminder = Reminder::new(format!("{}", i), 123, 0);
    test.lock().add_reminder(reminder);
  }

  let cloned_test = test.clone();
  tokio::spawn(async move {
    cloned_test.lock().remove_reminder("1");
  });

  // Continuously receive changes until the change we want is received.
  while let Ok(change) = rx.recv().await {
    if let ReminderChange::DidDeleteReminder { index } = change {
      assert_eq!(index, 1);
      break;
    }
  }
}
