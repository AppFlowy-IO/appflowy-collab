use crate::blocks::{
  deserialize_text_delta, parse_event, Block, BlockAction, BlockActionPayload, BlockActionType,
  BlockEvent, BlockOperation, ChildrenOperation, DocumentData, DocumentMeta, TextDelta,
  TextOperation, EXTERNAL_TYPE_TEXT,
};
use crate::document_awareness::DocumentAwarenessState;
use crate::error::DocumentError;
use crate::importer::define::BlockType;
use anyhow::anyhow;
use collab::core::collab::{DataSource, DATA_SECTION};
use collab::core::origin::CollabOrigin;
use collab::entity::EncodedCollab;
use collab::preclude::block::ClientID;
use collab::preclude::*;
use collab_entity::define::DOCUMENT_ROOT;
use collab_entity::schema::Schema;
use collab_entity::CollabType;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::vec;

/// The page_id is a reference that points to the block’s id.
/// The block that is referenced by this page_id is the first block of the document.
/// Crossing this block, we can build the whole document tree.
const PAGE_ID: &str = "page_id";
/// Document's all [Block] Map.
const BLOCKS: &str = "blocks";
/// Document's meta data.
const META: &str = "meta";
/// [Block]'s relation map. And it's also in [META].
/// The key is the parent block's children_id, and the value is the children block's id.
const CHILDREN_MAP: &str = "children_map";
/// [Block]'s yText map. And it's also in [META].
/// The key is the text block's external_id, and the value is the text block's yText.
const TEXT_MAP: &str = "text_map";

pub struct Document {
  collab: Collab,
  body: DocumentBody,
}

impl Document {
  /// Opening a document with given [Collab]
  /// If the required fields are not present in the current [Collab] instance, it will return an error.
  pub fn open(mut collab: Collab) -> Result<Self, DocumentError> {
    CollabType::Document.validate_require_data(&collab)?;
    let body = DocumentBody::new(&mut collab, None)?;
    Ok(Self { collab, body })
  }

  /// Opening a document with given [DataSource]
  /// If the required fields are not present in the current [Collab] instance, it will return an error.
  pub fn open_with_options(
    origin: CollabOrigin,
    doc_state: DataSource,
    document_id: &str,
    plugins: Vec<Box<dyn CollabPlugin>>,
  ) -> Result<Self, DocumentError> {
    let collab = Collab::new_with_source(origin, document_id, doc_state, plugins, true)?;
    Document::open(collab)
  }

  pub fn create_with_data(mut collab: Collab, data: DocumentData) -> Result<Self, DocumentError> {
    let body = DocumentBody::new(&mut collab, Some(data))?;
    Ok(Self { collab, body })
  }

  pub fn create(document_id: &str, data: DocumentData) -> Result<Self, DocumentError> {
    let collab = Collab::new_with_origin(CollabOrigin::Empty, document_id, vec![], false);
    Self::create_with_data(collab, data)
  }

  #[inline]
  pub fn split(self) -> (Collab, DocumentBody) {
    (self.collab, self.body)
  }

  pub fn validate(&self) -> Result<(), DocumentError> {
    CollabType::Document
      .validate_require_data(&self.collab)
      .map_err(|_| DocumentError::NoRequiredData)?;
    Ok(())
  }

  pub fn encode_collab(&self) -> Result<EncodedCollab, DocumentError> {
    self.collab.encode_collab_v1(|collab| {
      CollabType::Document
        .validate_require_data(collab)
        .map_err(|_| DocumentError::NoRequiredData)
    })
  }

  /// open a document and subscribe to the document changes.
  pub fn subscribe_block_changed<K, F>(&mut self, key: K, callback: F)
  where
    K: Into<Origin>,
    F: Fn(&Vec<BlockEvent>, bool) + Send + Sync + 'static,
  {
    let object_id = self.object_id().to_string();
    let self_origin = self.origin().clone();
    self.body.root.observe_deep_with(key, move |txn, events| {
      let origin = CollabOrigin::from(txn);
      let block_events = events
        .iter()
        .map(|deep_event| parse_event(&object_id, txn, deep_event))
        .collect::<Vec<BlockEvent>>();
      let is_remote = self_origin != origin;
      callback(&block_events, is_remote);
    });
  }

  /// Get document data.
  pub fn get_document_data(&self) -> Result<DocumentData, DocumentError> {
    let txn = self.collab.transact();
    self.body.get_document_data(&txn)
  }

  /// Get page id
  pub fn get_page_id(&self) -> Option<String> {
    let txn = self.collab.transact();
    self.body.root.get_with_txn(&txn, PAGE_ID)
  }

  #[deprecated(note = "use apply_text_delta instead")]
  pub fn create_text(&mut self, text_id: &str, delta: String) {
    self.apply_text_delta(text_id, delta);
  }

  /// Create a yText for incremental synchronization.
  /// Apply a delta to the yText.
  /// - @param text_id: The text block's external_id.
  /// - @param delta: The text block's delta. "\[{"insert": "Hello", "attributes": { "bold": true, "italic": true } }, {"insert": " World!"}]".
  pub fn apply_text_delta(&mut self, text_id: &str, delta: String) {
    let mut txn = self.collab.transact_mut();
    let delta = deserialize_text_delta(&delta).ok().unwrap_or_default();
    #[cfg(feature = "verbose_log")]
    tracing::trace!("apply_text_delta: text_id: {}, delta: {:?}", text_id, delta);

    self
      .body
      .text_operation
      .apply_delta(&mut txn, text_id, delta);
  }

  /// Apply actions to the document.
  pub fn apply_action(&mut self, actions: Vec<BlockAction>) -> Result<(), DocumentError> {
    let mut txn = self.collab.transact_mut();
    for action in actions {
      #[cfg(feature = "verbose_log")]
      tracing::trace!("apply_action: {:?}", action);

      let result = match action.action {
        BlockActionType::Insert => self.body.handle_insert_action(&mut txn, action.payload),
        BlockActionType::Update => self.body.handle_update_action(&mut txn, action.payload),
        BlockActionType::Delete => self.body.handle_delete_action(&mut txn, action.payload),
        BlockActionType::Move => self.body.handle_move_action(&mut txn, action.payload),
        BlockActionType::InsertText | BlockActionType::ApplyTextDelta => self
          .body
          .handle_apply_text_delta_action(&mut txn, action.payload),
      };
      result?;
    }
    Ok(())
  }

  /// Get block with the given id.
  pub fn get_block(&self, block_id: &str) -> Option<Block> {
    let txn = self.collab.transact();
    self.body.block_operation.get_block_with_txn(&txn, block_id)
  }

  pub fn get_block_data(&self, block_id: &str) -> Option<(BlockType, HashMap<String, Value>)> {
    let block = self.get_block(block_id)?;
    let block_type = BlockType::from_block_ty(&block.ty);
    Some((block_type, block.data))
  }

  /// Get the children of the block with the given id.
  pub fn get_block_children_ids(&self, block_id: &str) -> Vec<String> {
    let block = self.get_block(block_id);
    let txn = self.collab.transact();
    match block {
      Some(block) => self
        .body
        .children_operation
        .get_children(&txn, &block.children)
        .into_iter()
        .map(|child| child.to_string(&txn))
        .collect(),
      None => vec![],
    }
  }

  /// Insert block to the document.
  pub fn insert_block(
    &mut self,
    block: Block,
    prev_id: Option<String>,
  ) -> Result<Block, DocumentError> {
    let mut txn = self.collab.transact_mut();
    self.body.insert_block(&mut txn, block, prev_id)
  }

  pub fn delete_block(&mut self, block_id: &str) -> Result<(), DocumentError> {
    let mut txn = self.collab.transact_mut();
    self.body.delete_block(&mut txn, block_id)
  }

  pub fn get_all_block_ids(&self) -> Vec<String> {
    let txn = self.collab.transact();
    let blocks = self.body.block_operation.get_all_blocks(&txn);
    let block_ids = blocks
      .values()
      .map(|block| block.id.clone())
      .collect::<Vec<_>>();
    block_ids
  }

  pub fn get_block_ids<T: AsRef<str>>(
    &self,
    block_types: Vec<T>,
  ) -> Result<Vec<String>, DocumentError> {
    let txn = self.collab.transact();
    let blocks = self.body.block_operation.get_all_blocks(&txn);
    let block_ids = blocks
      .values()
      .filter_map(|block| {
        block_types
          .iter()
          .find(|&t| block.ty == t.as_ref())
          .map(|_| block.id.clone())
      })
      .collect::<Vec<_>>();
    Ok(block_ids)
  }

  /// Get the plain text from the text block with the given id.
  ///
  /// If the block is not found, return None.
  /// If the block is found but the external_id is not found, return None.
  pub fn get_plain_text_from_block(&self, block_id: &str) -> Option<String> {
    let block = self.get_block(block_id)?;
    let text_id = block.external_id.as_ref()?;
    let txn = self.collab.transact();
    self
      .body
      .text_operation
      .get_delta_with_txn(&txn, text_id)
      .map(|delta| {
        let text: Vec<String> = delta
          .iter()
          .filter_map(|d| match d {
            TextDelta::Inserted(s, _) => Some(s.clone()),
            _ => None,
          })
          .collect();
        text.join("")
      })
  }
  pub fn get_block_delta_json<T: AsRef<str>>(&self, block_id: T) -> Option<Value> {
    let delta = self.get_block_delta(block_id)?.1;
    serde_json::to_value(delta).ok()
  }

  pub fn get_block_delta<T: AsRef<str>>(&self, block_id: T) -> Option<(BlockType, Vec<TextDelta>)> {
    let block_id = block_id.as_ref();
    let txn = self.collab.transact();
    let block = self
      .body
      .block_operation
      .get_block_with_txn(&txn, block_id)?;
    let external_id = block.external_id?;
    let delta = self
      .body
      .text_operation
      .get_delta_with_txn(&txn, &external_id)?;

    let block_type = BlockType::from_block_ty(&block.ty);
    Some((block_type, delta))
  }

  pub fn remove_block_delta<T: AsRef<str>>(&mut self, block_id: T) {
    let block_id = block_id.as_ref();
    let mut txn = self.collab.transact_mut();
    let block = self.body.block_operation.get_block_with_txn(&txn, block_id);
    if let Some(block) = block {
      if let Some(external_id) = &block.external_id {
        self
          .body
          .text_operation
          .delete_text_with_txn(&mut txn, external_id);
      }
    }
  }

  pub fn set_block_delta<T: AsRef<str>>(
    &mut self,
    block_id: T,
    delta: Vec<TextDelta>,
  ) -> Result<(), DocumentError> {
    if delta.is_empty() {
      return Ok(());
    }

    let block_id = block_id.as_ref();
    let mut txn = self.collab.transact_mut();
    let block = self.body.block_operation.get_block_with_txn(&txn, block_id);
    if let Some(block) = block {
      let external_id = block
        .external_id
        .as_ref()
        .ok_or(DocumentError::ExternalIdIsNotFound)?;
      self
        .body
        .text_operation
        .set_delta(&mut txn, external_id, delta);
      Ok(())
    } else {
      Err(DocumentError::BlockIsNotFound)
    }
  }

  pub fn delete_block_from_parent(&mut self, block_id: &str, parent_id: &str) {
    let mut txn = self.collab.transact_mut();
    self
      .body
      .delete_block_from_parent(&mut txn, block_id, parent_id);
  }

  pub fn update_block(
    &mut self,
    block_id: &str,
    data: HashMap<String, Value>,
  ) -> Result<(), DocumentError> {
    let mut txn = self.collab.transact_mut();
    self
      .body
      .update_block_data(&mut txn, block_id, data, None, None)
  }

  pub fn move_block(
    &mut self,
    block_id: &str,
    parent_id: Option<String>,
    prev_id: Option<String>,
  ) -> Result<(), DocumentError> {
    let mut txn = self.collab.transact_mut();
    self.body.move_block(&mut txn, block_id, parent_id, prev_id)
  }

  pub fn redo(&mut self) -> bool {
    self.collab.redo().unwrap_or(false)
  }

  pub fn undo(&mut self) -> bool {
    self.collab.undo().unwrap_or(false)
  }

  /// Set the local state of the awareness.
  /// It will override the previous state.
  pub fn set_awareness_local_state(&mut self, state: DocumentAwarenessState) {
    if let Err(e) = self.collab.get_mut_awareness().set_local_state(state) {
      tracing::error!("Failed to serialize DocumentAwarenessState, state: {}", e);
    }
  }

  pub fn get_awareness_local_state(&self) -> Option<DocumentAwarenessState> {
    self.collab.get_awareness().local_state()
  }

  /// Clean the local state of the awareness.
  /// It should be called when the document is closed.
  pub fn clean_awareness_local_state(&mut self) {
    self.collab.get_mut_awareness().clean_local_state()
  }

  /// Subscribe to the awareness state change.
  /// This function only allowed to be called once for each document.
  pub fn subscribe_awareness_state<K, F>(&mut self, key: K, f: F)
  where
    K: Into<Origin>,
    F: Fn(HashMap<ClientID, DocumentAwarenessState>) + Send + Sync + 'static,
  {
    self.collab.get_awareness().on_update_with(key, move |awareness, e, _| {
      if let Ok(full_update) = awareness.update_with_clients(e.all_changes()) {
        let result: HashMap<_, _> = full_update.clients.iter().filter_map(|(&client_id, entry)| {
          match serde_json::from_str::<Option<DocumentAwarenessState>>(&entry.json) {
            Ok(state) => state.map(|state| (client_id, state)),
            Err(e) => {
              tracing::error!(
                "subscribe_awareness_state error: failed to parse state for id: {:?}, state: {:?} - {}",
                client_id,
                entry.json,
                e
              );
              None
            },
          }
        }).collect();
        f(result);
      }
    });
  }

  pub fn to_plain_text(&self) -> Result<String, DocumentError> {
    let page_id = self
      .get_page_id()
      .ok_or_else(|| DocumentError::Internal(anyhow!("Page id is not found")))?;
    let mut text = self.get_plain_text_from_block(&page_id).unwrap_or_default();
    let children = self.get_block_children_ids(&page_id);
    for child_id in children {
      text.push('\n');
      if let Some(child_text) = self.get_plain_text_from_block(&child_id) {
        text.push_str(&child_text);
      }
    }
    Ok(text)
  }

  // pub fn to_delta(&self) -> Result<Vec<String>, DocumentError> {
  //   let txn = self.collab.transact();
  //   let blocks = self.body.block_operation.get_all_blocks(&txn);
  //   let children_map = self.body.children_operation.get_all_children(&txn);
  //   let text_map = self.body.text_operation.serialize_all_text_delta(txn);
  //   let document_data = DocumentData {
  //     page_id,
  //     blocks,
  //     meta: DocumentMeta {
  //       children_map,
  //       text_map: Some(text_map),
  //     },
  //   };
  //   Ok(document_data)
  // }
}

impl Deref for Document {
  type Target = Collab;

  #[inline]
  fn deref(&self) -> &Self::Target {
    &self.collab
  }
}

impl DerefMut for Document {
  #[inline]
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.collab
  }
}

impl Borrow<Collab> for Document {
  #[inline]
  fn borrow(&self) -> &Collab {
    &self.collab
  }
}

impl BorrowMut<Collab> for Document {
  fn borrow_mut(&mut self) -> &mut Collab {
    &mut self.collab
  }
}

impl TryFrom<Collab> for Document {
  type Error = DocumentError;

  #[inline]
  fn try_from(collab: Collab) -> Result<Self, Self::Error> {
    Self::open(collab)
  }
}

impl Schema for Document {
  fn schema() -> JsonValue {
    json!({
      "data": {
        "document": {
          "blocks": {},
          "meta": {
            "children_map": {},
            "text_map": {}
          },
        }
      }
    })
  }
}

pub struct DocumentBody {
  pub root: MapRef,
  pub children_operation: ChildrenOperation,
  pub block_operation: BlockOperation,
  pub text_operation: TextOperation,
}

impl DocumentBody {
  /// Create new [Document] body based on the given [Collab] instance. If the required fields are
  /// not present in the current [Collab] instance, they will be initialized.
  ///
  /// If [DocumentData] was provided it will be applied on the document.
  pub(crate) fn new(
    collab: &mut Collab,
    data: Option<DocumentData>,
  ) -> Result<Self, DocumentError> {
    let mut txn = collab.context.transact_mut();

    Document::init_schema(&mut txn);

    let root: MapRef = collab.data.get_with_txn(&txn, DOCUMENT_ROOT).unwrap();
    let children_map: MapRef = collab
      .data
      .get_with_path(&txn, [DOCUMENT_ROOT, META, CHILDREN_MAP])
      .unwrap();
    let text_map: MapRef = collab
      .data
      .get_with_path(&txn, [DOCUMENT_ROOT, META, TEXT_MAP])
      .unwrap();
    let blocks: MapRef = collab
      .data
      .get_with_path(&txn, [DOCUMENT_ROOT, BLOCKS])
      .unwrap();

    let children_operation = ChildrenOperation::new(children_map);
    let text_operation = TextOperation::new(text_map);
    let block_operation = BlockOperation::new(blocks, children_operation.clone());

    // If the data is not None, insert the data to the document.
    if let Some(data) = data {
      root.insert(&mut txn, PAGE_ID, data.page_id);

      for (_, block) in data.blocks {
        block_operation.create_block_with_txn(&mut txn, block)?;
      }

      for (id, child_ids) in data.meta.children_map {
        let map = children_operation.get_or_init_children(&mut txn, &id);
        child_ids.iter().for_each(|child_id| {
          map.push_back(&mut txn, child_id.to_string());
        });
      }
      if let Some(text_map) = data.meta.text_map {
        for (id, delta) in text_map {
          let delta = serde_json::from_str(&delta).unwrap_or_else(|_| vec![]);
          text_operation.apply_delta(&mut txn, &id, delta)
        }
      }
    }
    drop(txn);
    collab.enable_undo_redo();
    Ok(Self {
      root,
      block_operation,
      children_operation,
      text_operation,
    })
  }

  /// Creates a [Document] body from the given [Collab] instance. If the required fields are not
  /// present, it will return `None`.
  pub fn from_collab(collab: &Collab) -> Option<Self> {
    let txn = collab.context.transact();
    // { document: {:} }
    let root: MapRef = collab.data.get_with_txn(&txn, DOCUMENT_ROOT)?;
    // { document: { blocks: {:} } }
    let blocks: MapRef = root.get_with_txn(&txn, BLOCKS)?;
    // { document: { blocks: {:}, meta: {:} } }
    let meta: MapRef = root.get_with_txn(&txn, META)?;
    // {document: { blocks: {:}, meta: { children_map: {:} } }
    let children_map: MapRef = meta.get_with_txn(&txn, CHILDREN_MAP)?;
    // { document: { blocks: {:}, meta: { text_map: {:} } }
    let text_map: MapRef = meta.get_with_txn(&txn, TEXT_MAP)?;

    let children_operation = ChildrenOperation::new(children_map);
    let text_operation = TextOperation::new(text_map);
    let block_operation = BlockOperation::new(blocks, children_operation.clone());

    Some(Self {
      root,
      block_operation,
      children_operation,
      text_operation,
    })
  }

  fn insert_block(
    &self,
    txn: &mut TransactionMut,
    block: Block,
    prev_id: Option<String>,
  ) -> Result<Block, DocumentError> {
    let block = self.block_operation.create_block_with_txn(txn, block)?;
    self.insert_block_to_parent(txn, &block, prev_id)
  }
  /// Insert block with the given parent id and prev id.
  fn insert_block_to_parent(
    &self,
    txn: &mut TransactionMut,
    block: &Block,
    prev_id: Option<String>,
  ) -> Result<Block, DocumentError> {
    let parent_id = &block.parent;
    // If the parent is not found, return an error.
    if parent_id.is_empty() {
      return Err(DocumentError::ParentIsNotFound);
    }

    let parent = match self.block_operation.get_block_with_txn(txn, parent_id) {
      Some(parent) => parent,
      None => return Err(DocumentError::ParentIsNotFound),
    };

    let parent_children_id = &parent.children;
    // If the prev_id is not found, insert the block to the first position.
    // so the default index is 0.
    let index = prev_id
      .and_then(|prev_id| {
        self
          .children_operation
          .get_child_index_with_txn(txn, parent_children_id, &prev_id)
      })
      .map(|prev_index| prev_index + 1)
      .unwrap_or(0);
    self
      .children_operation
      .insert_child_with_txn(txn, parent_children_id, &block.id, index);
    let block = self
      .block_operation
      .get_block_with_txn(txn, &block.id)
      .unwrap();
    Ok(block)
  }

  /// remove the reference of the block from its parent.
  fn delete_block_from_parent(&self, txn: &mut TransactionMut, block_id: &str, parent_id: &str) {
    let parent = self.block_operation.get_block_with_txn(txn, parent_id);
    if let Some(parent) = parent {
      let parent_children_id = &parent.children;
      self
        .children_operation
        .delete_child_with_txn(txn, parent_children_id, block_id);
    }
  }

  /// delete the block from the document
  ///
  /// 1. delete all the children of this block
  /// 2. delete the block from its parent
  /// 3. delete the block from the block map
  fn delete_block(&self, txn: &mut TransactionMut, block_id: &str) -> Result<(), DocumentError> {
    let block = match self.block_operation.get_block_with_txn(txn, block_id) {
      Some(block) => block,
      None => return Err(DocumentError::BlockIsNotFound),
    };

    let external_id = &block.external_id;

    // Delete all the children of this block.
    let children = self
      .children_operation
      .get_or_init_children(txn, &block.children);
    children
      .iter(txn)
      .map(|child| child.to_string(txn))
      .collect::<Vec<String>>()
      .iter()
      .for_each(|child| self.delete_block(txn, child).unwrap_or_default());

    // Delete the block from its parent.
    let parent_id = &block.parent;
    self.delete_block_from_parent(txn, block_id, parent_id);

    // Delete the text
    if let Some(external_id) = external_id {
      self.text_operation.delete_text_with_txn(txn, external_id);
    }
    // Delete the block
    self
      .block_operation
      .delete_block_with_txn(txn, block_id)
      .map(|_| ())
  }

  /// update the block data or external_id or external_type
  ///
  /// If the external_id and external_type are not provided, use the block's external_id and
  /// external_type.
  pub fn update_block_data(
    &self,
    txn: &mut TransactionMut,
    block_id: &str,
    data: HashMap<String, Value>,
    external_id: Option<String>,
    external_type: Option<String>,
  ) -> Result<(), DocumentError> {
    let block = match self.block_operation.get_block_with_txn(txn, block_id) {
      Some(block) => block,
      None => return Err(DocumentError::BlockIsNotFound),
    };
    self.block_operation.set_block_with_txn(
      txn,
      &block.id,
      Some(data),
      None,
      external_id.or(block.external_id),
      external_type.or(block.external_type),
    )
  }

  pub fn get_document_data<T: ReadTxn>(&self, txn: &T) -> Result<DocumentData, DocumentError> {
    let page_id = self
      .root
      .get(txn, PAGE_ID)
      .and_then(|v| v.cast::<String>().ok())
      .ok_or(DocumentError::PageIdIsEmpty)?;

    let blocks = self.block_operation.get_all_blocks(txn);
    let children_map = self.children_operation.get_all_children(txn);
    let text_map = self.text_operation.serialize_all_text_delta(txn);
    let document_data = DocumentData {
      page_id,
      blocks,
      meta: DocumentMeta {
        children_map,
        text_map: Some(text_map),
      },
    };
    Ok(document_data)
  }

  /// move the block to the new parent.
  pub fn move_block(
    &self,
    txn: &mut TransactionMut,
    block_id: &str,
    parent_id: Option<String>,
    prev_id: Option<String>,
  ) -> Result<(), DocumentError> {
    // If the parent is not found, return an error.
    let new_parent = match parent_id {
      Some(parent_id) => match self.block_operation.get_block_with_txn(txn, &parent_id) {
        Some(parent) => parent,
        None => return Err(DocumentError::ParentIsNotFound),
      },
      None => return Err(DocumentError::ParentIsNotFound),
    };

    let block = match self.block_operation.get_block_with_txn(txn, block_id) {
      Some(block) => block,
      None => return Err(DocumentError::BlockIsNotFound),
    };

    // If the old parent is not found, return an error.
    let old_parent = match self.block_operation.get_block_with_txn(txn, &block.parent) {
      Some(parent) => parent,
      None => return Err(DocumentError::ParentIsNotFound),
    };

    let new_parent_children_id = new_parent.children;
    let old_parent_children_id = old_parent.children;

    // It must delete the block from the old parent.
    // And this operation must be done before insert the block to the new parent,
    // because the block may be moved to the same parent.
    self
      .children_operation
      .delete_child_with_txn(txn, &old_parent_children_id, block_id);

    // If the prev_id is not found, insert the block to the first position.
    // so the default index is 0.
    let index = prev_id
      .and_then(|prev_id| {
        self
          .children_operation
          .get_child_index_with_txn(txn, &new_parent_children_id, &prev_id)
      })
      .map(|prev_index| prev_index + 1)
      .unwrap_or(0);

    // Insert the block to the new parent.
    self
      .children_operation
      .insert_child_with_txn(txn, &new_parent_children_id, block_id, index);

    // Update the parent of the block.
    self.block_operation.set_block_with_txn(
      txn,
      block_id,
      Some(block.data),
      Some(&new_parent.id),
      None,
      None,
    )
  }

  fn handle_insert_action(
    &self,
    txn: &mut TransactionMut,
    payload: BlockActionPayload,
  ) -> Result<(), DocumentError> {
    if let Some(mut block) = payload.block {
      // Check if the block's parent_id is empty, if it is empty, assign the parent_id to the block
      if block.parent.is_empty() && payload.parent_id.is_some() {
        block.parent = payload.parent_id.unwrap();
      }
      self.insert_block(txn, block, payload.prev_id).map(|_| ())
    } else {
      Err(DocumentError::BlockIsNotFound)
    }
  }

  fn handle_update_action(
    &self,
    txn: &mut TransactionMut,
    payload: BlockActionPayload,
  ) -> Result<(), DocumentError> {
    if let Some(block) = payload.block {
      let data = &block.data;
      let external_id = block.external_id;
      let external_type = block.external_type;
      self.update_block_data(txn, &block.id, data.to_owned(), external_id, external_type)
    } else {
      Err(DocumentError::BlockIsNotFound)
    }
  }

  fn handle_delete_action(
    &self,
    txn: &mut TransactionMut,
    payload: BlockActionPayload,
  ) -> Result<(), DocumentError> {
    if let Some(block) = payload.block {
      self.delete_block(txn, &block.id)
    } else {
      Err(DocumentError::BlockIsNotFound)
    }
  }

  fn handle_move_action(
    &self,
    txn: &mut TransactionMut,
    payload: BlockActionPayload,
  ) -> Result<(), DocumentError> {
    if let Some(block) = payload.block {
      self.move_block(txn, &block.id, payload.parent_id, payload.prev_id)
    } else {
      Err(DocumentError::BlockIsNotFound)
    }
  }

  fn handle_apply_text_delta_action(
    &self,
    txn: &mut TransactionMut,
    payload: BlockActionPayload,
  ) -> Result<(), DocumentError> {
    if let Some(text_id) = payload.text_id {
      if let Some(delta) = payload.delta {
        let delta = deserialize_text_delta(&delta).ok().unwrap_or_default();
        self.text_operation.apply_delta(txn, &text_id, delta);
        Ok(())
      } else {
        Err(DocumentError::TextActionParamsError)
      }
    } else {
      Err(DocumentError::TextActionParamsError)
    }
  }
}

/// Represents a the index content of a document.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DocumentIndexContent {
  pub page_id: String,
  pub text: String,
}

impl From<&Document> for DocumentIndexContent {
  fn from(value: &Document) -> Self {
    let collab = &value.collab;
    let txn = collab.transact();
    let page_id = value
      .body
      .root
      .get_with_txn(&txn, PAGE_ID)
      .expect("document should have page_id");

    let blocks = value.body.block_operation.get_all_blocks(&txn);
    let children_map = value.body.children_operation.get_all_children(&txn);
    let text_map = value.body.text_operation.stringify_all_text_delta(&txn);

    drop(txn);

    let page_block = blocks
      .get(&page_id)
      .expect("document data should contain page block");
    let children_key = &page_block.children;
    let children_ids = children_map
      .get(children_key)
      .expect("children map should contain page's children key");

    let text: Vec<_> = children_ids
      .iter()
      .filter_map(|id| blocks.get(id)) // get block of child
      .filter_map(|block| { // get external id of blocks with external type text
        match block.external_type.as_ref() {
          Some(ty) if ty == EXTERNAL_TYPE_TEXT => block.external_id.as_ref(),
          _ => None,
        }
      })
      .filter_map(|ext_id| text_map.get(ext_id).filter(|t| !t.is_empty())) // get text of block
      .cloned()
      .collect();

    let text = text.join(" "); // all text of document

    Self { page_id, text }
  }
}

pub fn gen_document_id() -> String {
  uuid::Uuid::new_v4().to_string()
}
