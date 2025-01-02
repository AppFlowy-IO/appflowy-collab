use serde::{Deserialize, Serialize};

pub const SPACE_IS_SPACE_KEY: &str = "is_space";
pub const SPACE_PERMISSION_KEY: &str = "space_permission";
pub const SPACE_ICON_KEY: &str = "space_icon";
pub const SPACE_ICON_COLOR_KEY: &str = "space_icon_color";
pub const SPACE_CREATED_AT_KEY: &str = "space_created_at";

/// Represents the space info of a view
///
/// Two view types are supported:
///
/// - Space view: A view associated with a space info. Parent view that can contain normal views.
///   Child views inherit the space's permissions.
///
/// - Normal view: Cannot contain space views and has no direct permission controls.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SpaceInfo {
  /// Whether the view is a space view.
  pub is_space: bool,

  /// The permission of the space view.
  ///
  /// If the space_permission is none, the space view will use the SpacePermission::PublicToAll.
  pub space_permission: Option<SpacePermission>,

  /// The created time of the space view.
  pub created_at: i64,

  /// The space icon key.
  ///
  /// If the space_icon_key is none, the space view will use the default icon.
  pub space_icon_key: Option<String>,

  /// The space icon color key.
  ///
  /// If the space_icon_color_key is none, the space view will use the default icon color.
  /// The value should be a valid hex color code: 0xFFA34AFD
  pub space_icon_color_key: Option<String>,
}

#[derive(Debug, Clone, serde_repr::Serialize_repr, serde_repr::Deserialize_repr)]
#[repr(u8)]
pub enum SpacePermission {
  PublicToAll = 0,
  Private = 1,
}
