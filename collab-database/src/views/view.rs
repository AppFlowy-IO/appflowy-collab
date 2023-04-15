use collab::core::any_array::ArrayMapUpdate;
use collab::preclude::map::MapPrelim;
use collab::preclude::{
  lib0Any, Array, ArrayRef, Map, MapRef, MapRefExtension, MapRefWrapper, ReadTxn, TransactionMut,
  YrsValue,
};
use serde::{Deserialize, Serialize};

use crate::block::CreateRowParams;
use crate::fields::Field;
use crate::views::layout::{DatabaseLayout, LayoutSettings};
use crate::views::{
  FieldOrder, FieldOrderArray, FilterArray, FilterMap, GroupSettingArray, GroupSettingMap,
  LayoutSetting, RowOrder, RowOrderArray, SortArray, SortMap,
};
use crate::{impl_any_update, impl_i64_update, impl_order_update, impl_str_update};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatabaseView {
  pub id: String,
  pub database_id: String,
  pub name: String,
  pub layout: DatabaseLayout,
  pub layout_settings: LayoutSettings,
  pub filters: Vec<FilterMap>,
  pub group_settings: Vec<GroupSettingMap>,
  pub sorts: Vec<SortMap>,
  pub row_orders: Vec<RowOrder>,
  pub field_orders: Vec<FieldOrder>,
  pub created_at: i64,
  pub modified_at: i64,
}

pub struct ViewDescription {
  pub id: String,
  pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateViewParams {
  pub database_id: String,
  pub view_id: String,
  pub name: String,
  pub layout: DatabaseLayout,
  pub layout_settings: LayoutSettings,
  pub filters: Vec<FilterMap>,
  pub groups: Vec<GroupSettingMap>,
  pub sorts: Vec<SortMap>,
}

impl CreateViewParams {
  pub fn new(database_id: String, view_id: String, name: String, layout: DatabaseLayout) -> Self {
    Self {
      database_id,
      view_id,
      name,
      layout,
      layout_settings: LayoutSettings::default(),
      filters: vec![],
      groups: vec![],
      sorts: vec![],
    }
  }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateDatabaseParams {
  pub database_id: String,
  pub view_id: String,
  pub name: String,
  pub layout: DatabaseLayout,
  pub layout_settings: LayoutSettings,
  pub filters: Vec<FilterMap>,
  pub groups: Vec<GroupSettingMap>,
  pub sorts: Vec<SortMap>,
  pub created_rows: Vec<CreateRowParams>,
  pub fields: Vec<Field>,
}

impl CreateDatabaseParams {
  pub fn from_view(view: DatabaseView, fields: Vec<Field>, rows: Vec<CreateRowParams>) -> Self {
    let mut params: Self = view.into();
    params.fields = fields;
    params.created_rows = rows;
    params
  }

  pub fn split(self) -> (Vec<CreateRowParams>, Vec<Field>, CreateViewParams) {
    (
      self.created_rows,
      self.fields,
      CreateViewParams {
        database_id: self.database_id,
        view_id: self.view_id,
        name: self.name,
        layout: self.layout,
        layout_settings: self.layout_settings,
        filters: self.filters,
        groups: self.groups,
        sorts: self.sorts,
      },
    )
  }
}

impl From<DatabaseView> for CreateDatabaseParams {
  fn from(view: DatabaseView) -> Self {
    Self {
      database_id: view.database_id,
      view_id: view.id,
      name: view.name,
      layout: view.layout,
      layout_settings: view.layout_settings,
      filters: view.filters,
      groups: view.group_settings,
      sorts: view.sorts,
      created_rows: vec![],
      fields: vec![],
    }
  }
}

const VIEW_ID: &str = "id";
const VIEW_NAME: &str = "name";
const VIEW_DATABASE_ID: &str = "database_id";
pub const VIEW_LAYOUT: &str = "layout";
const VIEW_LAYOUT_SETTINGS: &str = "layout_settings";
const VIEW_FILTERS: &str = "filters";
const VIEW_GROUPS: &str = "groups";
const VIEW_SORTS: &str = "sorts";
pub const ROW_ORDERS: &str = "row_orders";
pub const FIELD_ORDERS: &str = "field_orders";
const VIEW_CREATE_AT: &str = "created_at";
const VIEW_MODIFY_AT: &str = "modified_at";

pub struct ViewBuilder<'a, 'b> {
  id: &'a str,
  map_ref: MapRefWrapper,
  txn: &'a mut TransactionMut<'b>,
}

impl<'a, 'b> ViewBuilder<'a, 'b> {
  pub fn new(id: &'a str, txn: &'a mut TransactionMut<'b>, map_ref: MapRefWrapper) -> Self {
    map_ref.insert_str_with_txn(txn, VIEW_ID, id);
    Self { id, map_ref, txn }
  }

  pub fn update<F>(self, f: F) -> Self
  where
    F: FnOnce(ViewUpdate),
  {
    let update = ViewUpdate::new(self.id, self.txn, &self.map_ref);
    f(update);
    self
  }
  pub fn done(self) {}
}

pub struct ViewUpdate<'a, 'b> {
  #[allow(dead_code)]
  id: &'a str,
  map_ref: &'a MapRef,
  txn: &'a mut TransactionMut<'b>,
}

impl<'a, 'b> ViewUpdate<'a, 'b> {
  pub fn new(id: &'a str, txn: &'a mut TransactionMut<'b>, map_ref: &'a MapRef) -> Self {
    Self { id, map_ref, txn }
  }

  impl_str_update!(
    set_database_id,
    set_database_id_if_not_none,
    VIEW_DATABASE_ID
  );

  impl_i64_update!(set_created_at, set_created_at_if_not_none, VIEW_CREATE_AT);
  impl_i64_update!(set_modified_at, set_modified_at_if_not_none, VIEW_MODIFY_AT);

  impl_str_update!(set_name, set_name_if_not_none, VIEW_NAME);

  impl_any_update!(
    set_layout_type,
    set_layout_type_if_not_none,
    VIEW_LAYOUT,
    DatabaseLayout
  );

  impl_order_update!(
    set_row_orders,
    push_row_order,
    remove_row_order,
    move_row_order,
    insert_row_order,
    ROW_ORDERS,
    RowOrder,
    RowOrderArray
  );

  impl_order_update!(
    set_field_orders,
    push_field_order,
    remove_field_order,
    move_field_order,
    insert_field_order,
    FIELD_ORDERS,
    FieldOrder,
    FieldOrderArray
  );

  pub fn set_layout_settings(self, layout_settings: LayoutSettings) -> Self {
    let map_ref = self
      .map_ref
      .get_or_insert_map_with_txn(self.txn, VIEW_LAYOUT_SETTINGS);
    layout_settings.fill_map_ref(self.txn, &map_ref);
    self
  }

  pub fn update_layout_settings(
    self,
    layout_ty: &DatabaseLayout,
    layout_setting: LayoutSetting,
  ) -> Self {
    let layout_settings = self
      .map_ref
      .get_or_insert_map_with_txn(self.txn, VIEW_LAYOUT_SETTINGS);

    let inner_map = layout_settings.get_or_insert_map_with_txn(self.txn, layout_ty.as_ref());
    layout_setting.fill_map_ref(self.txn, &inner_map);
    self
  }

  pub fn remove_layout_setting(self, layout_ty: &DatabaseLayout) -> Self {
    let layout_settings = self
      .map_ref
      .get_or_insert_map_with_txn(self.txn, VIEW_LAYOUT_SETTINGS);

    layout_settings.remove(self.txn, layout_ty.as_ref());
    self
  }

  pub fn set_filters(mut self, filters: Vec<FilterMap>) -> Self {
    let array_ref = self.get_filter_array();
    let filter_array = FilterArray::from_any_maps(filters);
    filter_array.extend_array_ref(self.txn, array_ref);
    self
  }

  pub fn update_filters<F>(mut self, f: F) -> Self
  where
    F: FnOnce(ArrayMapUpdate),
  {
    let array_ref = self.get_filter_array();
    let update = ArrayMapUpdate::new(self.txn, array_ref);
    f(update);
    self
  }

  pub fn set_groups(mut self, group_settings: Vec<GroupSettingMap>) -> Self {
    let array_ref = self.get_group_array();
    let group_settings = GroupSettingArray::from_any_maps(group_settings);
    group_settings.extend_array_ref(self.txn, array_ref);
    self
  }

  pub fn update_groups<F>(mut self, f: F) -> Self
  where
    F: FnOnce(ArrayMapUpdate),
  {
    let array_ref = self.get_group_array();
    let update = ArrayMapUpdate::new(self.txn, array_ref);
    f(update);
    self
  }

  pub fn set_sorts(mut self, sorts: Vec<SortMap>) -> Self {
    let array_ref = self.get_sort_array();
    let sort_array = SortArray::from_any_maps(sorts);
    sort_array.extend_array_ref(self.txn, array_ref);
    self
  }

  pub fn update_sorts<F>(mut self, f: F) -> Self
  where
    F: FnOnce(ArrayMapUpdate),
  {
    let array_ref = self.get_sort_array();
    let update = ArrayMapUpdate::new(self.txn, array_ref);
    f(update);
    self
  }

  fn get_sort_array(&mut self) -> ArrayRef {
    self
      .map_ref
      .get_or_insert_array_with_txn::<MapPrelim<lib0Any>>(self.txn, VIEW_SORTS)
  }

  fn get_group_array(&mut self) -> ArrayRef {
    self
      .map_ref
      .get_or_insert_array_with_txn::<MapPrelim<lib0Any>>(self.txn, VIEW_GROUPS)
  }

  fn get_filter_array(&mut self) -> ArrayRef {
    self
      .map_ref
      .get_or_insert_array_with_txn::<MapPrelim<lib0Any>>(self.txn, VIEW_FILTERS)
  }
  pub fn done(self) -> Option<DatabaseView> {
    view_from_map_ref(self.map_ref, self.txn)
  }
}

pub fn view_id_from_map_ref<T: ReadTxn>(map_ref: &MapRef, txn: &T) -> Option<String> {
  map_ref.get_str_with_txn(txn, VIEW_ID)
}

pub fn view_description_from_value<T: ReadTxn>(
  value: YrsValue,
  txn: &T,
) -> Option<ViewDescription> {
  let map_ref = value.to_ymap()?;
  let id = map_ref.get_str_with_txn(txn, VIEW_ID)?;
  let name = map_ref.get_str_with_txn(txn, VIEW_NAME).unwrap_or_default();
  Some(ViewDescription { id, name })
}

pub fn view_from_value<T: ReadTxn>(value: YrsValue, txn: &T) -> Option<DatabaseView> {
  let map_ref = value.to_ymap()?;
  view_from_map_ref(&map_ref, txn)
}

pub fn group_setting_from_map_ref<T: ReadTxn>(txn: &T, map_ref: &MapRef) -> Vec<GroupSettingMap> {
  map_ref
    .get_array_ref_with_txn(txn, VIEW_GROUPS)
    .map(|array_ref| GroupSettingArray::from_array_ref(txn, &array_ref).0)
    .unwrap_or_default()
}

pub fn sorts_from_map_ref<T: ReadTxn>(txn: &T, map_ref: &MapRef) -> Vec<SortMap> {
  map_ref
    .get_array_ref_with_txn(txn, VIEW_SORTS)
    .map(|array_ref| SortArray::from_array_ref(txn, &array_ref).0)
    .unwrap_or_default()
}

pub fn filters_from_map_ref<T: ReadTxn>(txn: &T, map_ref: &MapRef) -> Vec<FilterMap> {
  map_ref
    .get_array_ref_with_txn(txn, VIEW_FILTERS)
    .map(|array_ref| FilterArray::from_array_ref(txn, &array_ref).0)
    .unwrap_or_default()
}

pub fn layout_setting_from_map_ref<T: ReadTxn>(txn: &T, map_ref: &MapRef) -> LayoutSettings {
  map_ref
    .get_map_with_txn(txn, VIEW_LAYOUT_SETTINGS)
    .map(|map_ref| LayoutSettings::from_map_ref(txn, map_ref))
    .unwrap_or_default()
}

pub fn view_from_map_ref<T: ReadTxn>(map_ref: &MapRef, txn: &T) -> Option<DatabaseView> {
  let id = map_ref.get_str_with_txn(txn, VIEW_ID)?;
  let name = map_ref.get_str_with_txn(txn, VIEW_NAME)?;
  let database_id = map_ref
    .get_str_with_txn(txn, VIEW_DATABASE_ID)
    .unwrap_or_default();
  let layout = map_ref
    .get_i64_with_txn(txn, VIEW_LAYOUT)
    .map(|value| value.try_into().ok())??;

  let layout_settings = map_ref
    .get_map_with_txn(txn, VIEW_LAYOUT_SETTINGS)
    .map(|map_ref| LayoutSettings::from_map_ref(txn, map_ref))
    .unwrap_or_default();

  let filters = map_ref
    .get_array_ref_with_txn(txn, VIEW_FILTERS)
    .map(|array_ref| FilterArray::from_array_ref(txn, &array_ref).0)
    .unwrap_or_default();

  let group_settings = map_ref
    .get_array_ref_with_txn(txn, VIEW_GROUPS)
    .map(|array_ref| GroupSettingArray::from_array_ref(txn, &array_ref).0)
    .unwrap_or_default();

  let sorts = map_ref
    .get_array_ref_with_txn(txn, VIEW_SORTS)
    .map(|array_ref| SortArray::from_array_ref(txn, &array_ref).0)
    .unwrap_or_default();

  let row_orders = map_ref
    .get_array_ref_with_txn(txn, ROW_ORDERS)
    .map(|array_ref| RowOrderArray::new(array_ref).get_orders_with_txn(txn))
    .unwrap_or_default();

  let field_orders = map_ref
    .get_array_ref_with_txn(txn, FIELD_ORDERS)
    .map(|array_ref| FieldOrderArray::new(array_ref).get_orders_with_txn(txn))
    .unwrap_or_default();

  let created_at = map_ref
    .get_i64_with_txn(txn, VIEW_CREATE_AT)
    .unwrap_or_default();

  let modified_at = map_ref
    .get_i64_with_txn(txn, VIEW_MODIFY_AT)
    .unwrap_or_default();

  Some(DatabaseView {
    id,
    database_id,
    name,
    layout,
    layout_settings,
    filters,
    group_settings,
    sorts,
    row_orders,
    field_orders,
    created_at,
    modified_at,
  })
}

pub trait OrderIdentifiable {
  fn identify_id(&self) -> String;
}

pub trait OrderArray {
  type Object: OrderIdentifiable + Into<lib0Any>;

  fn array_ref(&self) -> &ArrayRef;

  fn object_from_value_with_txn<T: ReadTxn>(
    &self,
    value: YrsValue,
    txn: &T,
  ) -> Option<Self::Object>;

  fn extends_with_txn(&self, txn: &mut TransactionMut, others: Vec<Self::Object>) {
    let array_ref = self.array_ref();
    for order in others {
      array_ref.push_back(txn, order);
    }
  }

  fn push_with_txn(&self, txn: &mut TransactionMut, object: Self::Object) {
    self.array_ref().push_back(txn, object);
  }

  fn insert_with_txn(
    &self,
    txn: &mut TransactionMut,
    object: Self::Object,
    prev_object_id: Option<&String>,
  ) {
    if let Some(prev_object_id) = prev_object_id {
      match self.get_position_with_txn(txn, &prev_object_id) {
        None => {
          self.array_ref().push_back(txn, object);
        },
        Some(pos) => {
          let next: u32 = pos as u32 + 1;
          self.array_ref().insert(txn, next, object);
        },
      }
    } else {
      self.array_ref().push_front(txn, object);
    }
  }

  fn get_orders_with_txn<T: ReadTxn>(&self, txn: &T) -> Vec<Self::Object> {
    self
      .array_ref()
      .iter(txn)
      .flat_map(|v| self.object_from_value_with_txn(v, txn))
      .collect::<Vec<Self::Object>>()
  }

  fn remove_with_txn(&self, txn: &mut TransactionMut, id: &str) -> Option<()> {
    let pos = self.array_ref().iter(txn).position(|value| {
      match self.object_from_value_with_txn(value, txn) {
        None => false,
        Some(order) => order.identify_id() == id,
      }
    })?;
    self.array_ref().remove(txn, pos as u32);
    None
  }

  fn move_to(&self, txn: &mut TransactionMut, from: u32, to: u32) {
    let array_ref = self.array_ref();
    if let Some(YrsValue::Any(value)) = array_ref.get(txn, from) {
      if to <= array_ref.len(txn) {
        array_ref.remove(txn, from);
        array_ref.insert(txn, to, value);
      }
    }
  }

  fn get_position_with_txn<T: ReadTxn>(&self, txn: &T, id: &str) -> Option<u32> {
    self
      .array_ref()
      .iter(txn)
      .position(|value| match self.object_from_value_with_txn(value, txn) {
        None => false,
        Some(order) => order.identify_id() == id,
      })
      .map(|pos| pos as u32)
  }
}
