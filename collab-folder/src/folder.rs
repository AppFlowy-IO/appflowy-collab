use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use collab::core::collab::DataSource;
pub use collab::core::origin::CollabOrigin;
use collab::entity::EncodedCollab;
use collab::preclude::*;
use collab::util::any_to_json_value;
use collab_entity::define::{FOLDER, FOLDER_META, FOLDER_WORKSPACE_ID};
use collab_entity::CollabType;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::error::FolderError;
use crate::folder_observe::ViewChangeSender;
use crate::hierarchy_builder::{FlattedViews, ParentChildViews};
use crate::section::{Section, SectionItem, SectionMap};
use crate::view::view_from_map_ref;
use crate::{
  impl_section_op, subscribe_folder_change, FolderData, ParentChildRelations, SectionChangeSender,
  TrashInfo, View, ViewUpdate, ViewsMap, Workspace,
};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct UserId(pub(crate) String);

impl From<i64> for UserId {
  fn from(value: i64) -> Self {
    Self(value.to_string())
  }
}

impl From<&i64> for UserId {
  fn from(value: &i64) -> Self {
    Self(value.to_string())
  }
}

impl UserId {
  pub fn as_i64(&self) -> i64 {
    self.0.parse::<i64>().unwrap()
  }
}

impl AsRef<str> for UserId {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

const VIEWS: &str = "views";
const PARENT_CHILD_VIEW_RELATION: &str = "relation";
const CURRENT_VIEW: &str = "current_view";
const CURRENT_VIEW_FOR_USER: &str = "current_view_for_user";

pub(crate) const FAVORITES_V1: &str = "favorites";
const SECTION: &str = "section";

#[derive(Clone)]
pub struct FolderNotify {
  pub view_change_tx: ViewChangeSender,
  pub section_change_tx: SectionChangeSender,
}

/// Represents the folder hierarchy in a workspace.
///
/// The `Folder` structure organizes different aspects of a workspace into individual components
/// such as workspaces, views, trash, favorites, meta, and relation.
///
/// The folder hierarchy can be visualized as follows:
/// Folder: [workspaces: [], views: {}, trash: [], favorites: { uid: [] }, meta: {}, relation: {}]
///
///
/// # Fields
///
/// * `inner`: A mutex-protected shared pointer for managing access to the folder data.
/// * `root`: Wrapper around the root map reference.
/// * `workspaces`: An array of `WorkspaceArray` objects, representing different workspaces in the folder.
///   Currently, we only use one workspace to manage all the views in the folder.
/// * `views`: A shared pointer to a map (`ViewsMap`) from view id to view data, keeping track of each view's data.
/// * `trash`: An array of `TrashArray` objects, representing the trash items in the folder.
/// * `section`: An map of `SectionMap` objects, representing the favorite items in the folder.
/// * `meta`: Wrapper around the metadata map reference.
/// * `subscription`: A `DeepEventsSubscription` object, managing the subscription for folder changes, like inserting a new view.
/// * `notifier`: An optional `FolderNotify` object for notifying about changes in the folder.
pub struct Folder {
  pub collab: Collab,
  pub body: FolderBody,
}

impl Folder {
  pub fn open<T: Into<UserId>>(
    uid: T,
    mut collab: Collab,
    notifier: Option<FolderNotify>,
  ) -> Result<Self, FolderError> {
    let uid = uid.into();
    let body = FolderBody::open(&mut collab, uid, notifier)?;
    let folder = Folder { collab, body };
    if folder.get_workspace_id().is_none() {
      // When the folder is opened, the workspace id must be present.
      Err(FolderError::NoRequiredData("missing workspace id".into()))
    } else {
      Ok(folder)
    }
  }

  pub fn create<T: Into<UserId>>(
    uid: T,
    mut collab: Collab,
    notifier: Option<FolderNotify>,
    data: FolderData,
  ) -> Self {
    let body = FolderBody::open_with(uid.into(), &mut collab, notifier, Some(data));
    Folder { collab, body }
  }

  pub fn from_collab_doc_state<T: Into<UserId>>(
    uid: T,
    origin: CollabOrigin,
    collab_doc_state: DataSource,
    workspace_id: &str,
    plugins: Vec<Box<dyn CollabPlugin>>,
  ) -> Result<Self, FolderError> {
    let collab = Collab::new_with_source(origin, workspace_id, collab_doc_state, plugins, false)?;
    Self::open(uid, collab, None)
  }

  pub fn close(&self) {
    self.collab.remove_all_plugins();
  }

  pub fn validate(&self) -> Result<(), FolderError> {
    CollabType::Folder
      .validate_require_data(&self.collab)
      .map_err(|err| FolderError::NoRequiredData(err.to_string()))?;
    Ok(())
  }

  pub fn uid(&self) -> &UserId {
    &self.body.uid
  }

  /// Returns the doc state and the state vector.
  pub fn encode_collab(&self) -> Result<EncodedCollab, FolderError> {
    self.collab.encode_collab_v1(|collab| {
      CollabType::Folder
        .validate_require_data(collab)
        .map_err(|err| FolderError::NoRequiredData(err.to_string()))
    })
  }

  /// Fetches the folder data based on the current workspace and view.
  ///
  /// This function initiates a transaction on the root node and uses it to fetch the current workspace
  /// and view. It also fetches all workspaces and their respective views.
  ///
  /// It goes through all the workspaces and fetches the views recursively for each workspace.
  ///
  /// # Returns
  ///
  /// * `Some(FolderData)`: If the operation is successful, it returns `Some` variant wrapping `FolderData`
  ///   object, which consists of current workspace ID, current view, a list of workspaces, and their respective views.
  ///
  /// * `None`: If the operation is unsuccessful (though it should typically not be the case as `Some`
  ///   is returned explicitly), it returns `None`.
  pub fn get_folder_data(&self, workspace_id: &str) -> Option<FolderData> {
    let txn = self.collab.transact();
    self.body.get_folder_data(&txn, workspace_id)
  }

  /// Fetches the current workspace.
  ///
  /// This function fetches the ID of the current workspace from the meta object,
  /// and uses this ID to fetch the actual workspace object.
  ///
  pub fn get_workspace_info(&self, workspace_id: &str) -> Option<Workspace> {
    let txn = self.collab.transact();
    self.body.get_workspace_info(&txn, workspace_id)
  }

  pub fn get_workspace_id(&self) -> Option<String> {
    let txn = self.collab.transact();
    self.body.get_workspace_id(&txn)
  }

  pub fn get_all_views(&self) -> Vec<Arc<View>> {
    let txn = self.collab.transact();
    self.body.views.get_all_views(&txn)
  }

  pub fn get_views<T: AsRef<str>>(&self, view_ids: &[T]) -> Vec<Arc<View>> {
    let txn = self.collab.transact();
    self.body.views.get_views(&txn, view_ids)
  }

  pub fn get_views_belong_to(&self, parent_id: &str) -> Vec<Arc<View>> {
    let txn = self.collab.transact();
    self.body.views.get_views_belong_to(&txn, parent_id)
  }

  pub fn move_view(&mut self, view_id: &str, from: u32, to: u32) -> Option<Arc<View>> {
    let mut txn = self.collab.transact_mut();
    self.body.move_view(&mut txn, view_id, from, to)
  }

  /// Moves a nested view to a new location in the hierarchy.
  ///
  /// This function takes the `view_id` of the view to be moved,
  /// `new_parent_id` of the view under which the `view_id` should be moved,
  /// and an optional `new_prev_id` to position the `view_id` right after
  /// this specific view.
  ///
  /// If `new_prev_id` is provided, the moved view will be placed right after
  /// the view corresponding to `new_prev_id` under the `new_parent_id`.
  /// If `new_prev_id` is `None`, the moved view will become the first child of the new parent.
  ///
  /// # Arguments
  ///
  /// * `view_id` - A string slice that holds the id of the view to be moved.
  /// * `new_parent_id` - A string slice that holds the id of the new parent view.
  /// * `prev_view_id` - An `Option<String>` that holds the id of the view after which the `view_id` should be positioned.
  ///
  pub fn move_nested_view(
    &mut self,
    view_id: &str,
    new_parent_id: &str,
    prev_view_id: Option<String>,
  ) -> Option<Arc<View>> {
    let mut txn = self.collab.transact_mut();
    self
      .body
      .move_nested_view(&mut txn, view_id, new_parent_id, prev_view_id)
  }

  pub fn set_current_view(&mut self, view_id: String) {
    let mut txn = self.collab.transact_mut();
    self.body.set_current_view(&mut txn, view_id);
  }

  pub fn get_current_view(&self) -> Option<String> {
    let txn = self.collab.transact();
    self.body.get_current_view(&txn)
  }

  pub fn update_view<F>(&mut self, view_id: &str, f: F) -> Option<Arc<View>>
  where
    F: FnOnce(ViewUpdate) -> Option<View>,
  {
    let mut txn = self.collab.transact_mut();
    self.body.views.update_view(&mut txn, view_id, f)
  }

  pub fn delete_views<T: AsRef<str>>(&mut self, views: Vec<T>) {
    let mut txn = self.collab.transact_mut();
    self.body.views.delete_views(&mut txn, views);
  }

  // Section operations
  // Favorites
  impl_section_op!(
    Section::Favorite,
    set_favorite,
    add_favorite_view_ids,
    delete_favorite_view_ids,
    get_my_favorite_sections,
    get_all_favorites_sections,
    remove_all_my_favorite_sections
  );

  // Recent
  impl_section_op!(
    Section::Recent,
    set_recent,
    add_recent_view_ids,
    delete_recent_view_ids,
    get_my_recent_sections,
    get_all_recent_sections,
    remove_all_my_recent_sections
  );

  // Trash
  impl_section_op!(
    Section::Trash,
    set_trash,
    add_trash_view_ids,
    delete_trash_view_ids,
    get_my_trash_sections,
    get_all_trash_sections,
    remove_all_my_trash_sections
  );

  // Private
  impl_section_op!(
    Section::Private,
    set_private,
    add_private_view_ids,
    delete_private_view_ids,
    get_my_private_sections,
    get_all_private_sections,
    remove_all_my_private_sections
  );

  pub fn get_my_trash_info(&self) -> Vec<TrashInfo> {
    let txn = self.collab.transact();
    self
      .get_my_trash_sections()
      .into_iter()
      .flat_map(|section| {
        self
          .body
          .views
          .get_view_name_with_txn(&txn, &section.id)
          .map(|name| TrashInfo {
            id: section.id,
            name,
            created_at: section.timestamp,
          })
      })
      .collect()
  }

  /// Inserts a new view into the specified workspace under a given parent view.
  ///
  /// # Parameters:
  /// - `parent_view_id`: The ID of the parent view under which the new view will be added.
  /// - `index`: Optional. If provided, the new view will be inserted at the specified position
  ///    among the parent view's children. If `None`, the new view will be added at the end of
  ///    the children list.
  ///
  /// # Behavior:
  /// - When `index` is `Some`, the new view is inserted at that position in the list of the
  ///   parent view’s children.
  /// - When `index` is `None`, the new view is appended to the end of the parent view’s children.
  ///
  /// Represents a view that serves as an identifier for a specific [`Collab`] object.
  /// A view can represent different types of [`Collab`] objects, such as a document or a database.
  /// When a view is inserted, its id is the[`Collab`] object id.
  ///
  pub fn insert_view(&mut self, view: View, index: Option<u32>) {
    let mut txn = self.collab.transact_mut();
    self.body.views.insert(&mut txn, view, index);
  }

  /// Insert a list of views at the end of its parent view
  pub fn insert_views(&mut self, views: Vec<View>) {
    let mut txn = self.collab.transact_mut();
    for view in views {
      self.body.views.insert(&mut txn, view, None);
    }
  }

  /// Insert parent-children views into the folder.
  /// when only insert one view, user [Self::insert_view] instead.
  pub fn insert_nested_views(&mut self, views: Vec<ParentChildViews>) {
    let views = FlattedViews::flatten_views(views);
    let mut txn = self.collab.transact_mut();
    for view in views {
      self.body.views.insert(&mut txn, view, None);
    }
  }

  pub fn get_view(&self, view_id: &str) -> Option<Arc<View>> {
    let txn = self.collab.transact();
    self.body.views.get_view(&txn, view_id)
  }

  pub fn is_view_in_section(&self, section: Section, view_id: &str) -> bool {
    let txn = self.collab.transact();
    if let Some(op) = self.body.section.section_op(&txn, section) {
      op.contains_with_txn(&txn, view_id)
    } else {
      false
    }
  }

  pub fn to_json(&self) -> String {
    self.to_json_value().to_string()
  }

  pub fn to_json_value(&self) -> JsonValue {
    let txn = self.collab.transact();
    let any = self.body.root.to_json(&txn);
    any_to_json_value(any).unwrap()
  }

  /// Recursively retrieves all views associated with the provided `view_id` using a transaction.
  ///
  /// The function begins by attempting to retrieve the parent view associated with the `view_id`.
  /// If the parent view is not found, an empty vector is returned.
  /// If the parent view is found, the function proceeds to retrieve all of its child views recursively.
  ///
  /// The function finally returns a vector containing the parent view and all of its child views.
  /// The views are clones of the original objects.
  ///
  /// # Parameters
  ///
  /// * `txn`: A read transaction object which is used to execute the view retrieval.
  /// * `view_id`: The ID of the parent view.
  ///
  /// # Returns
  ///
  /// * `Vec<View>`: A vector of `View` objects that includes the parent view and all of its child views.
  pub fn get_view_recursively(&self, view_id: &str) -> Vec<View> {
    let txn = self.collab.transact();
    self.body.get_view_recursively_with_txn(&txn, view_id)
  }
}

impl Deref for Folder {
  type Target = Collab;

  fn deref(&self) -> &Self::Target {
    &self.collab
  }
}

impl DerefMut for Folder {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.collab
  }
}

impl Borrow<Collab> for Folder {
  #[inline]
  fn borrow(&self) -> &Collab {
    &self.collab
  }
}

impl BorrowMut<Collab> for Folder {
  fn borrow_mut(&mut self) -> &mut Collab {
    &mut self.collab
  }
}

pub fn check_folder_is_valid(collab: &Collab) -> Result<String, FolderError> {
  let txn = collab.transact();
  let meta: MapRef = collab
    .data
    .get_with_path(&txn, vec![FOLDER, FOLDER_META])
    .ok_or_else(|| FolderError::NoRequiredData("No meta data".to_string()))?;
  match meta.get_with_txn::<_, String>(&txn, FOLDER_WORKSPACE_ID) {
    None => Err(FolderError::NoRequiredData("No workspace id".to_string())),
    Some(workspace_id) => {
      if workspace_id.is_empty() {
        Err(FolderError::NoRequiredData("No workspace id".to_string()))
      } else {
        Ok(workspace_id)
      }
    },
  }
}

fn get_views_from_root<T: ReadTxn>(
  root: &MapRef,
  _uid: &UserId,
  view_relations: &Arc<ParentChildRelations>,
  section_map: &Arc<SectionMap>,
  txn: &T,
) -> HashMap<String, Arc<View>> {
  root
    .iter(txn)
    .flat_map(|(key, value)| {
      if let YrsValue::YMap(map) = value {
        view_from_map_ref(&map, txn, view_relations, section_map)
          .map(|view| (key.to_string(), Arc::new(view)))
      } else {
        None
      }
    })
    .collect()
}

pub struct FolderBody {
  pub(crate) uid: UserId,
  pub root: MapRef,
  pub views: Arc<ViewsMap>,
  pub section: Arc<SectionMap>,
  pub meta: MapRef,
  #[allow(dead_code)]
  subscription: Subscription,
  #[allow(dead_code)]
  notifier: Option<FolderNotify>,
}

impl FolderBody {
  pub fn open(
    collab: &mut Collab,
    uid: UserId,
    notifier: Option<FolderNotify>,
  ) -> Result<Self, FolderError> {
    CollabType::Folder.validate_require_data(collab)?;
    Ok(Self::open_with(uid, collab, notifier, None))
  }

  pub fn open_with(
    uid: UserId,
    collab: &mut Collab,
    notifier: Option<FolderNotify>,
    folder_data: Option<FolderData>,
  ) -> Self {
    let index_json_sender = collab.index_json_sender.clone();
    let mut txn = collab.context.transact_mut();
    // create the folder
    let mut folder = collab.data.get_or_init_map(&mut txn, FOLDER);
    let subscription = subscribe_folder_change(&mut folder);

    // create the folder data
    let views: MapRef = folder.get_or_init(&mut txn, VIEWS);
    let section: MapRef = folder.get_or_init(&mut txn, SECTION);
    let meta: MapRef = folder.get_or_init(&mut txn, FOLDER_META);
    let parent_child_relations = Arc::new(ParentChildRelations::new(
      folder.get_or_init(&mut txn, PARENT_CHILD_VIEW_RELATION),
    ));

    let section = Arc::new(SectionMap::create(
      &mut txn,
      &uid,
      section,
      notifier
        .as_ref()
        .map(|notifier| notifier.section_change_tx.clone()),
    ));
    let all_views = get_views_from_root(&views, &uid, &parent_child_relations, &section, &txn);
    let views = Arc::new(ViewsMap::new(
      &uid,
      views,
      notifier
        .as_ref()
        .map(|notifier| notifier.view_change_tx.clone()),
      parent_child_relations,
      section.clone(),
      index_json_sender,
      all_views,
    ));

    if let Some(folder_data) = folder_data {
      let workspace_id = folder_data.workspace.id.clone();
      views.insert(&mut txn, folder_data.workspace.into(), None);

      for view in folder_data.views {
        views.insert(&mut txn, view, None);
      }

      meta.insert(&mut txn, FOLDER_WORKSPACE_ID, workspace_id);
      // For compatibility with older collab library which doesn't use CURRENT_VIEW_FOR_USER.
      meta.insert(&mut txn, CURRENT_VIEW, folder_data.current_view.clone());
      let current_view_for_user = meta.get_or_init_map(&mut txn, CURRENT_VIEW_FOR_USER);
      current_view_for_user.insert(&mut txn, uid.as_ref(), folder_data.current_view.clone());

      if let Some(fav_section) = section.section_op(&txn, Section::Favorite) {
        for (uid, sections) in folder_data.favorites {
          fav_section.add_sections_for_user_with_txn(&mut txn, &uid, sections);
        }
      }

      if let Some(trash_section) = section.section_op(&txn, Section::Trash) {
        for (uid, sections) in folder_data.trash {
          trash_section.add_sections_for_user_with_txn(&mut txn, &uid, sections);
        }
      }
    }
    Self {
      uid,
      root: folder,
      views,
      section,
      meta,
      subscription,
      notifier,
    }
  }

  pub fn get_workspace_id_with_txn<T: ReadTxn>(&self, txn: &T) -> Option<String> {
    self.meta.get_with_txn(txn, FOLDER_WORKSPACE_ID)
  }

  /// Recursively retrieves all views associated with the provided `view_id` using a transaction.
  ///
  /// The function begins by attempting to retrieve the parent view associated with the `view_id`.
  /// If the parent view is not found, an empty vector is returned.
  /// If the parent view is found, the function proceeds to retrieve all of its child views recursively.
  ///
  /// The function finally returns a vector containing the parent view and all of its child views.
  /// The views are clones of the original objects.
  ///
  /// # Parameters
  ///
  /// * `txn`: A read transaction object which is used to execute the view retrieval.
  /// * `view_id`: The ID of the parent view.
  ///
  /// # Returns
  ///
  /// * `Vec<View>`: A vector of `View` objects that includes the parent view and all of its child views.
  pub fn get_view_recursively_with_txn<T: ReadTxn>(&self, txn: &T, view_id: &str) -> Vec<View> {
    match self.views.get_view_with_txn(txn, view_id) {
      None => vec![],
      Some(parent_view) => {
        let mut views = vec![parent_view.as_ref().clone()];
        let child_views = parent_view
          .children
          .items
          .iter()
          .flat_map(|child| self.get_view_recursively_with_txn(txn, &child.id))
          .collect::<Vec<_>>();
        views.extend(child_views);
        views
      },
    }
  }

  pub fn get_workspace_info<T: ReadTxn>(&self, txn: &T, workspace_id: &str) -> Option<Workspace> {
    let folder_workspace_id: String = self.meta.get_with_txn(txn, FOLDER_WORKSPACE_ID)?;
    if folder_workspace_id != workspace_id {
      error!("Workspace id not match when get current workspace");
      return None;
    }

    let view = self.views.get_view_with_txn(txn, &folder_workspace_id)?;
    Some(Workspace::from(view.as_ref()))
  }

  pub fn get_folder_data<T: ReadTxn>(&self, txn: &T, workspace_id: &str) -> Option<FolderData> {
    let folder_workspace_id = self.get_workspace_id_with_txn(txn)?;
    if folder_workspace_id != workspace_id {
      error!(
        "Workspace id not match when get folder data, expected: {}, actual: {}",
        workspace_id, folder_workspace_id
      );
      return None;
    }
    let workspace = Workspace::from(self.views.get_view_with_txn(txn, workspace_id)?.as_ref());
    let current_view = self.get_current_view(txn).unwrap_or_default();
    let mut views = vec![];
    let orphan_views = self
      .views
      .get_orphan_views_with_txn(txn)
      .iter()
      .map(|view| view.as_ref().clone())
      .collect::<Vec<View>>();
    for view in self.views.get_views_belong_to(txn, workspace_id) {
      views.extend(self.get_view_recursively_with_txn(txn, &view.id));
    }
    views.extend(orphan_views);

    let favorites = self
      .section
      .section_op(txn, Section::Favorite)
      .map(|op| op.get_sections(txn))
      .unwrap_or_default();
    let recent = self
      .section
      .section_op(txn, Section::Recent)
      .map(|op| op.get_sections(txn))
      .unwrap_or_default();

    let trash = self
      .section
      .section_op(txn, Section::Trash)
      .map(|op| op.get_sections(txn))
      .unwrap_or_default();

    let private = self
      .section
      .section_op(txn, Section::Private)
      .map(|op| op.get_sections(txn))
      .unwrap_or_default();

    Some(FolderData {
      workspace,
      current_view,
      views,
      favorites,
      recent,
      trash,
      private,
    })
  }

  pub fn get_workspace_id<T: ReadTxn>(&self, txn: &T) -> Option<String> {
    self.meta.get_with_txn(txn, FOLDER_WORKSPACE_ID)
  }

  pub fn move_view(
    &self,
    txn: &mut TransactionMut,
    view_id: &str,
    from: u32,
    to: u32,
  ) -> Option<Arc<View>> {
    let view = self.views.get_view_with_txn(txn, view_id)?;
    self.views.move_child(txn, &view.parent_view_id, from, to);
    Some(view)
  }

  pub fn move_nested_view(
    &self,
    txn: &mut TransactionMut,
    view_id: &str,
    new_parent_id: &str,
    prev_view_id: Option<String>,
  ) -> Option<Arc<View>> {
    tracing::debug!("Move nested view: {}", view_id);
    let view = self.views.get_view_with_txn(txn, view_id)?;
    let current_workspace_id = self.get_workspace_id_with_txn(txn)?;
    let parent_id = view.parent_view_id.as_str();

    let new_parent_view = self.views.get_view_with_txn(txn, new_parent_id);

    // If the new parent is not a view, it must be a workspace.
    // Check if the new parent is the current workspace, as moving out of the current workspace is not supported yet.
    if new_parent_id != current_workspace_id && new_parent_view.is_none() {
      tracing::warn!("Unsupported move out current workspace: {}", view_id);
      return None;
    }

    // dissociate the child from its parent
    self
      .views
      .dissociate_parent_child_with_txn(txn, parent_id, view_id);
    // associate the child with its new parent and place it after the prev_view_id. If the prev_view_id is None,
    // place it as the first child.
    self
      .views
      .associate_parent_child_with_txn(txn, new_parent_id, view_id, prev_view_id.clone());
    // Update the view's parent ID.
    self
      .views
      .update_view_with_txn(&self.uid, txn, view_id, |update| {
        update.set_bid(new_parent_id).done()
      });
    Some(view)
  }

  pub fn get_current_view<T: ReadTxn>(&self, txn: &T) -> Option<String> {
    // Fallback to CURRENT_VIEW if CURRENT_VIEW_FOR_USER is not present, or if the section
    // does not contain entry for the user. This could happen if a workspace member is using
    // updated version AppFlowy Collab, while another member is using an older version.
    self
      .meta
      .get(txn, CURRENT_VIEW_FOR_USER)
      .and_then(|v| match v {
        YrsValue::YMap(map) => map.get_with_txn(txn, self.uid.as_ref()),
        _ => None,
      })
      .or(self.meta.get_with_txn(txn, CURRENT_VIEW))
  }

  pub fn set_current_view(&self, txn: &mut TransactionMut, view: String) {
    let current_view_for_user = self.meta.get_or_init_map(txn, CURRENT_VIEW_FOR_USER);
    current_view_for_user.try_update(txn, self.uid.0.clone(), view);
  }
}

pub fn default_folder_data(workspace_id: &str) -> FolderData {
  let workspace = Workspace {
    id: workspace_id.to_string(),
    name: "".to_string(),
    child_views: Default::default(),
    created_at: 0,
    created_by: None,
    last_edited_time: 0,
    last_edited_by: None,
  };
  FolderData {
    workspace,
    current_view: "".to_string(),
    views: vec![],
    favorites: HashMap::new(),
    recent: HashMap::new(),
    trash: HashMap::new(),
    private: HashMap::new(),
  }
}
