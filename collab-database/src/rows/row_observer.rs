use crate::rows::{Cell, Row, ROW_CELLS, ROW_HEIGHT, ROW_VISIBILITY};
use collab::core::value::YrsValueExtension;

use collab::preclude::{DeepEventsSubscription, DeepObservable, EntryChange, Event, MapRefWrapper};
use collab::preclude::{PathSegment, ToJson};
use std::ops::Deref;

use tokio::sync::broadcast;
use tracing::trace;

pub type RowChangeSender = broadcast::Sender<RowChange>;
pub type RowChangeReceiver = broadcast::Receiver<RowChange>;

#[derive(Debug, Clone)]
pub enum RowChange {
  DidUpdateVisibility { value: bool },
  DidUpdateHeight { value: i32 },
  DidUpdateCell { key: String, value: Cell },
  DidUpdateRowComment { row: Row },
}

pub(crate) fn subscribe_row_data_change(
  row_data_map: &mut MapRefWrapper,
  change_tx: RowChangeSender,
) -> DeepEventsSubscription {
  row_data_map.observe_deep(move |txn, events| {
    for event in events.iter() {
      // trace!(
      //   "row observe event: {:?}, {:?}",
      //   event.path(),
      //   event.target().to_json(txn)
      // );
      match event {
        Event::Text(_) => {},
        Event::Array(_) => {},
        Event::Map(map_event) => {
          let path = RowChangePath::from(event);
          for (key, enctry_change) in map_event.keys(txn).iter() {
            match &path {
              RowChangePath::Unknown(_s) => {
                // When the event path is identified as [RowChangePath::Unknown], it indicates that the path itself remains unchanged.
                // In this scenario, the modification is confined to the key/value pairs within the map at the existing path.
                // Essentially, even though the overall path stays the same, the contents (specific key/value pairs) at this path are the ones being updated.
                if let EntryChange::Updated(_, value) = enctry_change {
                  let change_value = RowChangeValue::from(key.deref());
                  match change_value {
                    RowChangeValue::Unknown(_s) => {
                      trace!("row observe value update: {}:{:?}", key, value.to_json(txn))
                    },
                    RowChangeValue::Height => {
                      if let Some(value) = value.as_i64() {
                        let _ = change_tx.send(RowChange::DidUpdateHeight {
                          value: value as i32,
                        });
                      }
                    },
                    RowChangeValue::Visibility => {
                      if let Some(value) = value.as_bool() {
                        let _ = change_tx.send(RowChange::DidUpdateVisibility { value });
                      }
                    },
                  }
                }
              },
              RowChangePath::Cells => {
                match enctry_change {
                  EntryChange::Inserted(value) => {
                    // When a cell's value is newly inserted, the corresponding event exhibits specific characteristics:
                    // - The event path is set to "/cells", indicating the operation is within the cells structure.
                    // - The 'key' in the event corresponds to the unique identifier of the newly inserted cell.
                    // - The 'value' represents the actual content or data inserted into this cell.
                    if let Some(cell) = Cell::from_value(txn, value) {
                      let _ = change_tx.send(RowChange::DidUpdateCell {
                        key: key.to_string(),
                        value: cell,
                      });
                    }
                  },
                  EntryChange::Updated(_, _) => {
                    // Processing an update to a cell's value:
                    // The event path for an updated cell value is structured as "/cells/{key}", where {key} is the unique identifier of the cell.
                    // The 'target' of the event represents the new, updated value of the cell.
                    // To accurately identify which cell has been updated, we need to extract its key from the event path.
                    // This extraction is achieved by removing the last segment of the path, which is "/{key}".
                    // After this removal, the remaining part of the path directly corresponds to the key of the cell.
                    // In the current implementation, this key is used as the identifier (ID) of the field within the cells map.
                    if let Some(PathSegment::Key(key)) = event.path().pop_back() {
                      if let Some(cell) = Cell::from_value(txn, &event.target()) {
                        let _ = change_tx.send(RowChange::DidUpdateCell {
                          key: key.deref().to_string(),
                          value: cell,
                        });
                      }
                    }
                    //
                  },
                  EntryChange::Removed(_value) => {
                    trace!("row observe delete: {}", key);
                  },
                }
              },
            }
          }
        },
        Event::XmlFragment(_) => {},
        Event::XmlText(_) => {},
      }
    }
  })
}

enum RowChangePath {
  Unknown(String),
  Cells,
}

impl From<&Event> for RowChangePath {
  fn from(event: &Event) -> Self {
    match event.path().pop_front() {
      Some(segment) => match segment {
        PathSegment::Key(s) => RowChangePath::from(s.deref()),
        PathSegment::Index(_) => Self::Unknown("index".to_string()),
      },
      None => Self::Unknown("".to_string()),
    }
  }
}

impl From<&str> for RowChangePath {
  fn from(s: &str) -> Self {
    match s {
      ROW_CELLS => Self::Cells,
      s => Self::Unknown(s.to_string()),
    }
  }
}
enum RowChangeValue {
  Unknown(String),
  Height,
  Visibility,
}

impl From<&str> for RowChangeValue {
  fn from(s: &str) -> Self {
    match s {
      ROW_HEIGHT => Self::Height,
      ROW_VISIBILITY => Self::Visibility,
      s => Self::Unknown(s.to_string()),
    }
  }
}
