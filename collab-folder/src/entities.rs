use serde::{Deserialize, Serialize};

use crate::{View, Workspace};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FolderData {
  pub current_workspace_id: String,
  pub current_view: String,
  pub workspaces: Vec<Workspace>,
  pub views: Vec<View>,
}

#[derive(Clone, Debug)]
pub struct TrashInfo {
  pub id: String,
  pub name: String,
  pub created_at: i64,
}
impl AsRef<str> for TrashInfo {
  fn as_ref(&self) -> &str {
    &self.id
  }
}
