use collab_document::{blocks::Block, document::Document};
use nanoid::nanoid;

use crate::util::DocumentTest;

#[test]
fn plain_text_1_test() {
  let doc_id = "1";
  let test = DocumentTest::new(1, doc_id);
  let mut document = test.document;
  let paragraphs = vec![
    "Welcome to AppFlowy!".to_string(),
    "Here are the basics".to_string(),
    "Click anywhere and just start typing.".to_string(),
    "Highlight any text, and use the editing menu to _style_ **your** <u>writing</u> `however` you ~~like.~~".to_string(),
    "As soon as you type `/` a menu will pop up. Select different types of content blocks you can add.".to_string(),
    "Type `/` followed by `/bullet` or `/num` to create a list.".to_string(),
    "Click `+ New Page `button at the bottom of your sidebar to add a new page.".to_string(),
    "Click `+` next to any page title in the sidebar to quickly add a new subpage, `Document`, `Grid`, or `Kanban Board`.".to_string(),
  ];
  insert_paragraphs(&mut document, paragraphs.clone());

  let splitted = document.paragraphs();
  // remove the empty lines at the beginning and the end
  // the first one and the last one are empty
  assert_eq!(splitted.len(), 8);

  for i in 0..splitted.len() {
    assert_eq!(splitted[i], paragraphs[i]);
  }
}

fn insert_paragraphs(document: &mut Document, paragraphs: Vec<String>) {
  let page_id = document.get_page_id().unwrap();
  let mut prev_id = "".to_string();
  for paragraph in paragraphs {
    let block_id = nanoid!(6);
    let text_id = nanoid!(6);
    let block = Block {
      id: block_id.clone(),
      ty: "paragraph".to_owned(),
      parent: page_id.clone(),
      children: "".to_string(),
      external_id: Some(text_id.clone()),
      external_type: Some("text".to_owned()),
      data: Default::default(),
    };

    document.insert_block(block, Some(prev_id.clone())).unwrap();
    prev_id.clone_from(&block_id);

    document.apply_text_delta(&text_id, format!(r#"[{{"insert": "{}"}}]"#, paragraph));
  }
}
