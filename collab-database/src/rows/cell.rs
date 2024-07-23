use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use collab::preclude::{Any, Map, MapRef, ReadTxn, TransactionMut, YrsValue};
use serde::{Deserialize, Serialize};

use crate::database::timestamp;
use crate::rows::{RowId, CREATED_AT, LAST_MODIFIED};

/// Store lists of cells
/// The key is the id of the [Field]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Cells(HashMap<String, Cell>);

impl Cells {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn into_inner(self) -> HashMap<String, Cell> {
    self.0
  }

  /// Returns a new instance of [Cells] from a [MapRef]
  pub fn fill_map_ref(self, txn: &mut TransactionMut, map_ref: &MapRef) {
    self.into_inner().into_iter().for_each(|(k, v)| {
      let cell_map_ref: MapRef = map_ref.get_or_init(txn, k);
      v.fill_map_ref(txn, &cell_map_ref);
    });
  }

  /// Returns a [Cell] from the [Cells] by the [Field] id
  pub fn cell_for_field_id(&self, field_id: &str) -> Option<&Cell> {
    self.get(field_id)
  }
}

impl<T: ReadTxn> From<(&'_ T, &MapRef)> for Cells {
  fn from(params: (&'_ T, &MapRef)) -> Self {
    let mut this = Self::new();
    params.1.iter(params.0).for_each(|(k, v)| {
      if let YrsValue::YMap(map_ref) = v {
        this.insert(k.to_string(), (params.0, &map_ref).into());
      }
    });
    this
  }
}

impl From<HashMap<String, Cell>> for Cells {
  fn from(data: HashMap<String, Cell>) -> Self {
    Self(data)
  }
}

impl Deref for Cells {
  type Target = HashMap<String, Cell>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl DerefMut for Cells {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

pub struct CellsUpdate<'a, 'b> {
  map_ref: &'a MapRef,
  txn: &'a mut TransactionMut<'b>,
}

impl<'a, 'b> CellsUpdate<'a, 'b> {
  pub fn new(txn: &'a mut TransactionMut<'b>, map_ref: &'a MapRef) -> Self {
    Self { map_ref, txn }
  }

  pub fn insert_cell(self, key: &str, cell: Cell) -> Self {
    let cell_map_ref: MapRef = self.map_ref.get_or_init(self.txn, key);
    if cell_map_ref.get(self.txn, CREATED_AT).is_none() {
      cell_map_ref.insert(self.txn, CREATED_AT, timestamp());
    }

    cell.fill_map_ref(self.txn, &cell_map_ref);
    cell_map_ref.insert(self.txn, LAST_MODIFIED, timestamp());
    self
  }

  /// Override the existing cell's key/value contained in the [Cell]
  /// It will create the cell if it's not exist
  pub fn insert<T: Into<Cell>>(self, key: &str, value: T) -> Self {
    let cell = value.into();
    self.insert_cell(key, cell)
  }

  pub fn clear(self, key: &str) -> Self {
    let cell_map_ref: MapRef = self.map_ref.get_or_init(self.txn, key);
    cell_map_ref.clear(self.txn);

    self
  }
}

pub type Cell = HashMap<String, Any>;
pub type CellBuilder = HashMap<String, Any>;
pub type CellUpdate = MapRef;

pub fn get_field_type_from_cell<T: From<i64>>(cell: &Cell) -> Option<T> {
  let any = cell.get("field_type")?.clone();
  let field_type: i64 = any.cast().ok()?;
  Some(T::from(field_type))
}

/// Create a new [CellBuilder] with the field type.
pub fn new_cell_builder(field_type: impl Into<i64>) -> CellBuilder {
  HashMap::from([("field_type".into(), Any::from(field_type.into()))])
}

pub struct RowCell {
  pub row_id: RowId,
  /// The cell might be empty if no value is written before
  pub cell: Option<Cell>,
}

impl RowCell {
  pub fn new(row_id: RowId, cell: Option<Cell>) -> Self {
    Self { row_id, cell }
  }
}

impl Deref for RowCell {
  type Target = Option<Cell>;

  fn deref(&self) -> &Self::Target {
    &self.cell
  }
}
