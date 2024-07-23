use collab::preclude::{Collab, Map, MapExt, MapRef};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::workspace_database::relation::RowRelationMap;

pub struct DatabaseRelation {
  #[allow(dead_code)]
  inner: Arc<Mutex<Collab>>,
  row_relation_map: RowRelationMap,
}

const ROW_RELATION_MAP: &str = "row_relations";
impl DatabaseRelation {
  pub fn new(collab: Arc<Mutex<Collab>>) -> DatabaseRelation {
    let relation_map = {
      let lock = collab.blocking_lock();
      let mut txn = lock.context.transact_mut();
      lock.data.get_or_init(&mut txn, ROW_RELATION_MAP)
    };

    Self {
      inner: collab,
      row_relation_map: RowRelationMap::from_map_ref(relation_map),
    }
  }

  pub fn row_relations(&self) -> &RowRelationMap {
    &self.row_relation_map
  }
}
