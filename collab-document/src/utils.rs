use crate::blocks::{Block, TextDelta};
use std::collections::HashMap;

#[inline]
pub(crate) fn push_deltas_to_str(buf: &mut String, deltas: Vec<TextDelta>) {
  for delta in deltas {
    if let TextDelta::Inserted(text, _) = delta {
      // trim all whitespace characters from start and end of the text
      let mut start = 0;
      let mut end = 0;
      let mut i = 0;
      for c in text.chars() {
        i += c.len_utf8();
        if char::is_whitespace(c) {
          if end == 0 {
            start += c.len_utf8();
          }
        } else {
          end = i;
        }
      }
      if start < end {
        if start > 0 {
          // if there were any whitespaces at the start, add a space before the text
          buf.push(' ');
        }
        buf.push_str(&text[start..end]);
        if end < text.len() {
          // if there were any whitespaces at the end, add a space after the text
          buf.push(' ');
        }
      }
    }
  }
}

/// Try to retrieve deltas from `block.data.delta`.
#[inline]
pub(crate) fn get_delta_from_block_data(block: &Block) -> Option<Vec<TextDelta>> {
  if let Some(delta) = block.data.get("delta") {
    if let Ok(deltas) = serde_json::from_value::<Vec<TextDelta>>(delta.clone()) {
      return Some(deltas);
    }
  }
  None
}

/// Try to retrieve deltas from text_map's text associated with `block.external_id`.
#[inline]
pub(crate) fn get_delta_from_external_text_id(
  block: &Block,
  text_map: &mut HashMap<String, Vec<TextDelta>>,
) -> Option<Vec<TextDelta>> {
  if block.external_type.as_deref() == Some("text") {
    if let Some(text_id) = block.external_id.as_deref() {
      if let Some(deltas) = text_map.remove(text_id) {
        return Some(deltas);
      }
    }
  }
  None
}
