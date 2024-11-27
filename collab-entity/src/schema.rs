use crate::CollabValidateError;
use collab::preclude::{
  Array, ArrayPrelim, ArrayRef, Map as _, MapPrelim, MapRef, Out, ReadTxn, TransactionMut, WriteTxn,
};
use serde_json::{Map, Value};

pub trait Schema {
  fn schema() -> Value;

  fn init_schema(tx: &mut TransactionMut) {
    let schema = Self::schema();
    match schema {
      Value::Object(map) => {
        for (field, v) in map {
          match v {
            Value::Array(array) => {
              let array_ref = tx.get_or_insert_array(field);
              init_array(array_ref, tx, array)
            },
            Value::Object(map) => {
              let map_ref = tx.get_or_insert_map(field);
              init_map(map_ref, tx, map)
            },
            _ => { /* ignore */ },
          }
        }
      },
      _ => { /* ignore other types at top level */ },
    }
  }

  fn validate_schema<T: ReadTxn>(tx: &T) -> Result<(), CollabValidateError> {
    let schema = Self::schema();
    match &schema {
      Value::Object(map) => {
        for (field, v) in map {
          match v {
            Value::Array(array) => match tx.get_array(field.as_str()) {
              Some(array_ref) => validate_array(array_ref, tx, array)?,
              None => return Err(construct_err(&schema)),
            },
            Value::Object(map) => match tx.get_map(field.as_str()) {
              Some(map_ref) => validate_map(map_ref, tx, map)?,
              None => return Err(construct_err(&schema)),
            },
            _ => { /* ignore */ },
          }
        }
      },
      _ => { /* ignore other types at top level */ },
    }
    Ok(())
  }
}

fn construct_err(value: &Value) -> CollabValidateError {
  CollabValidateError::NoRequiredData(format!("missing data: {}", value))
}

fn validate_map<T: ReadTxn>(
  map_ref: MapRef,
  tx: &T,
  map: &Map<String, Value>,
) -> Result<(), CollabValidateError> {
  for (field, value) in map.iter() {
    match map_ref.get(tx, field) {
      Some(out) => {
        match (out, value) {
          (Out::YArray(array_ref), Value::Array(array)) => validate_array(array_ref, tx, array)?,
          (Out::YMap(map_ref), Value::Object(map)) => validate_map(map_ref, tx, map)?,
          _ => { /* ignore */ },
        }
      },
      None => return Err(construct_err(value)),
    }
  }
  Ok(())
}

fn validate_array<T: ReadTxn>(
  array_ref: ArrayRef,
  tx: &T,
  array: &Vec<Value>,
) -> Result<(), CollabValidateError> {
  for (i, value) in array.iter().enumerate() {
    match array_ref.get(tx, i as u32) {
      Some(out) => {
        match (out, value) {
          (Out::YArray(array_ref), Value::Array(array)) => validate_array(array_ref, tx, array)?,
          (Out::YMap(map_ref), Value::Object(map)) => validate_map(map_ref, tx, map)?,
          _ => { /* ignore */ },
        }
      },
      None => return Err(construct_err(value)),
    }
  }
  Ok(())
}

fn init_array(array_ref: ArrayRef, tx: &mut TransactionMut, array: Vec<Value>) {
  for value in array {
    match value {
      Value::Array(array) => {
        let array_ref = array_ref.push_back(tx, ArrayPrelim::default());
        init_array(array_ref, tx, array)
      },
      Value::Object(map) => {
        let map_ref = array_ref.push_back(tx, MapPrelim::default());
        init_map(map_ref, tx, map)
      },
      _ => { /* ignore */ },
    }
  }
}

fn init_map(map_ref: MapRef, tx: &mut TransactionMut, schema: Map<String, Value>) {
  for (field, value) in schema {
    match value {
      Value::Array(array) => {
        let array_ref = map_ref.insert(tx, field, ArrayPrelim::default());
        init_array(array_ref, tx, array)
      },
      Value::Object(map) => {
        let map_ref = map_ref.insert(tx, field, MapPrelim::default());
        init_map(map_ref, tx, map)
      },
      _ => { /* ignore */ },
    }
  }
}

#[cfg(test)]
mod test {
  use crate::schema::Schema;
  use assert_matches2::assert_matches;
  use collab::preclude::Collab;
  use serde_json::{json, Value};

  struct TestSchema;
  impl Schema for TestSchema {
    fn schema() -> Value {
      json!({
        "data": {
          "document": {
            "blocks": {},
            "meta": {
              "text_map": {}
            }
          }
        },
        "meta":{}
      })
    }
  }

  struct TestSchema2;
  impl Schema for TestSchema2 {
    fn schema() -> Value {
      json!({
        "data": {
          "database": {
            "fields": {},
            "views": {},
            "metas": {}
          }
        }
      })
    }
  }

  #[test]
  fn test_schema() {
    let mut collab = Collab::new(0, "oid-1", "device-1", vec![], false);
    let mut tx = collab.transact_mut();
    // initialize collab state with schema
    TestSchema::init_schema(&mut tx);

    // test collab state against a valid schema
    assert_matches!(TestSchema::validate_schema(&tx), Ok(_));

    // test collab state against another (invalid) schema
    assert_matches!(TestSchema2::validate_schema(&tx), Err(_));
  }
}
