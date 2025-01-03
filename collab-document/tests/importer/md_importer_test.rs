use assert_json_diff::assert_json_eq;
use collab_document::document::{gen_document_id, Document};
use serde_json::json;

use crate::importer::util::{
  get_block_by_type, get_children_blocks, get_delta_json, get_page_block,
  markdown_to_document_data, parse_json,
};

#[test]
fn test_override_document() {
  let markdown_1 = "hello world";
  let doc_data_1 = markdown_to_document_data(markdown_1);

  let doc_id = gen_document_id();
  let doc = Document::create(&doc_id, doc_data_1).unwrap();
  {
    let plain_txt = doc.paragraphs().join("");
    assert_eq!(markdown_1, plain_txt);
  }

  let (mut collab, mut body) = doc.split();

  let markdown_2 = "foo bar";
  let doc_data_2 = markdown_to_document_data(markdown_2);
  {
    let mut txn = collab.transact_mut();
    body.reset_with_data(&mut txn, Some(doc_data_2)).unwrap();
  }
  {
    let modified_doc = Document::open(collab).unwrap();
    let plain_txt = modified_doc.paragraphs().join("");
    assert_eq!(markdown_2, plain_txt);
  }
}

#[test]
fn test_inline_elements() {
  let markdown = "This is **bold**, *italic*, ~~delete~~, and [a link](https://example.com).";

  let result = markdown_to_document_data(markdown);

  assert_eq!(result.blocks.len(), 2); // 1 page + 1 paragraph

  let paragraph = get_block_by_type(&result, "paragraph");
  let delta_json = get_delta_json(&result, &paragraph.id);

  let expected_delta = json!([
      {"insert": "This is "},
      {"insert": "bold", "attributes": {"bold": true}},
      {"insert": ", "},
      {"insert": "italic", "attributes": {"italic": true}},
      {"insert": ", "},
      {"insert": "delete", "attributes": {"strikethrough": true}},
      {"insert": ", and "},
      {"insert": "a link", "attributes": {"href": "https://example.com"}},
      {"insert": "."}
  ]);

  assert_eq!(delta_json, expected_delta);
}

#[test]
fn test_href_link() {
  let markdown = r#"
  ## Project tasks
  [Tasks](Marketing%20campaign%2088ac0cea4cb245efb44d63ace0a37d1e/Tasks%2042a63a9fe6df4a39a8d5b4804e0eae9f.csv)
  "#;
  let result = markdown_to_document_data(markdown);
  let paragraph = get_block_by_type(&result, "paragraph");
  let delta_json = get_delta_json(&result, &paragraph.id);
  let expected_delta = json!( [
    {
      "attributes": {
        "href": "Marketing%20campaign%2088ac0cea4cb245efb44d63ace0a37d1e/Tasks%2042a63a9fe6df4a39a8d5b4804e0eae9f.csv"
      },
      "insert": "Tasks"
    }
  ]);
  assert_json_eq!(delta_json, expected_delta);
}

#[test]
fn test_inline_math() {
  let markdown = "This is an inline math formula: $E=mc^2$.";

  let result = markdown_to_document_data(markdown);
  let paragraph = get_block_by_type(&result, "paragraph");
  let delta_json = get_delta_json(&result, &paragraph.id);

  let expected_delta = json!([
      {"insert": "This is an inline math formula: "},
      {"insert": "$", "attributes": {"formula": "E=mc^2"}},
      {"insert": "."}
  ]);

  assert_eq!(delta_json, expected_delta);
}
#[test]
fn test_mixed_inline_elements() {
  let markdown = "This is ***bold and italic*** and `code`.";

  let result = markdown_to_document_data(markdown);
  let paragraph = get_block_by_type(&result, "paragraph");
  let delta_json = get_delta_json(&result, &paragraph.id);

  let expected_delta = json!([
      {"insert": "This is "},
      {"insert": "bold and italic", "attributes": {"bold": true, "italic": true}},
      {"insert": " and "},
      {"insert": "code", "attributes": {"code": true}},
      {"insert": "."}
  ]);

  assert_eq!(delta_json, expected_delta);
}

#[test]
fn test_nested_inline_elements() {
  let markdown = "This is **bold with *nested italic* text**.";

  let result = markdown_to_document_data(markdown);

  let paragraph = get_block_by_type(&result, "paragraph");
  let delta_json = get_delta_json(&result, &paragraph.id);

  let expected_delta = json!([
      {"insert": "This is "},
      {"insert": "bold with ", "attributes": {"bold": true}},
      {"insert": "nested italic", "attributes": {"bold": true, "italic": true}},
      {"insert": " text", "attributes": {"bold": true}},
      {"insert": "."}
  ]);

  assert_eq!(delta_json, expected_delta);
}

#[test]
fn test_headings() {
  let markdown = r"
# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6
";

  let result = markdown_to_document_data(markdown);

  let page = get_page_block(&result);
  let headings: Vec<_> = get_children_blocks(&result, &page.id);

  assert_eq!(headings.len(), 6);
  assert_eq!(headings[0].data["level"], 1);
  assert_eq!(headings[1].data["level"], 2);
  assert_eq!(headings[2].data["level"], 3);
  assert_eq!(headings[3].data["level"], 4);
  assert_eq!(headings[4].data["level"], 5);
  assert_eq!(headings[5].data["level"], 6);

  for (i, heading) in headings.iter().enumerate() {
    assert_eq!(heading.data["level"], i + 1);
    assert_eq!(heading.ty, "heading");

    let delta_json = get_delta_json(&result, &heading.id);
    let expected_delta = json!([
        {"insert": format!("Heading {}", i + 1)}
    ]);
    assert_eq!(delta_json, expected_delta);
  }
}

#[test]
fn test_numbered_list() {
  let markdown = "1. First item\n2. Second item\n3. Third item";

  let result = markdown_to_document_data(markdown);
  let page = get_page_block(&result);
  let list = get_children_blocks(&result, &page.id);

  assert_eq!(list.len(), 3);

  for (i, item) in list.iter().enumerate() {
    assert_eq!(item.ty, "numbered_list");

    let delta_json = get_delta_json(&result, &item.id);
    let expected_delta = json!([
        {"insert": format!("{} item", ["First", "Second", "Third"][i])}
    ]);
    assert_eq!(delta_json, expected_delta);
  }
}

#[test]
fn test_bulleted_list() {
  let markdown = r#"* First item
- Second item
* Third item"#;

  let result = markdown_to_document_data(markdown);

  let page = get_page_block(&result);
  let list = get_children_blocks(&result, &page.id);

  assert_eq!(list.len(), 3);

  for (i, item) in list.iter().enumerate() {
    assert_eq!(item.ty, "bulleted_list");
    let delta_json = get_delta_json(&result, &item.id);
    let expected_delta = json!([
        {"insert": format!("{} item", ["First", "Second", "Third"][i])}
    ]);
    assert_eq!(delta_json, expected_delta);
  }
}

#[test]
fn test_checkbox() {
  let markdown = r#"
- [ ] Unchecked
- [x] Checked"#;

  let result = markdown_to_document_data(markdown);

  let page = get_page_block(&result);
  let list = get_children_blocks(&result, &page.id);

  assert_eq!(list.len(), 2);

  for (i, item) in list.iter().enumerate() {
    assert_eq!(item.ty, "todo_list");

    let delta_json = get_delta_json(&result, &item.id);
    let expected_delta = json!([
        {"insert": format!("{}", ["Unchecked", "Checked"][i])}
    ]);
    assert_eq!(delta_json, expected_delta);

    let checked = item.data.get("checked").unwrap();
    assert_eq!(checked, i != 0);
  }
}

#[test]
fn test_mix_list() {
  let markdown = r#"1. First item
- Second item
3. Third item
- [ ] Fourth item"#;

  let result = markdown_to_document_data(markdown);

  let page = get_page_block(&result);
  let list = get_children_blocks(&result, &page.id);

  assert_eq!(list.len(), 4);

  for (i, item) in list.iter().enumerate() {
    let delta_json = get_delta_json(&result, &item.id);
    let expected_delta = json!([
        {"insert": format!("{} item", ["First", "Second", "Third", "Fourth"][i])}
    ]);
    assert_eq!(delta_json, expected_delta);

    let data = item.data.clone();
    let ty = item.ty.clone();

    if i == 0 {
      assert_eq!(ty, "numbered_list");
    } else if i == 1 {
      assert_eq!(ty, "bulleted_list");
    } else if i == 2 {
      assert_eq!(ty, "numbered_list");
    }

    if i == 3 {
      assert_eq!(ty, "todo_list");
      assert!(!data
        .get("checked")
        .and_then(|v| v.as_bool())
        .expect("'checked' should be a boolean value"));
    }
  }
}

#[test]
fn test_quote_list() {
  let markdown = r#"> First item
This is a paragraph

> Second item

> Third item"#;

  let result = markdown_to_document_data(markdown);
  let page = get_page_block(&result);

  let list = get_children_blocks(&result, &page.id);

  assert_eq!(list.len(), 3);

  for (i, item) in list.iter().enumerate() {
    assert_eq!(item.ty, "quote");

    let text_map = result.meta.text_map.as_ref().unwrap();
    let delta_json = parse_json(text_map.get(&item.id).unwrap());

    if i == 0 {
      let expected_delta = json!([
          {"insert": "First item\nThis is a paragraph"}
      ]);
      assert_eq!(delta_json, expected_delta);
    } else {
      let expected_delta = json!([
          {"insert": format!("{} item", ["Second", "Third"][i - 1])}
      ]);
      assert_eq!(delta_json, expected_delta);
    }
  }
}

#[test]
fn test_code_block() {
  let markdown = r#"
```rust
fn main() {
    println!("Hello, world!");
}
```
"#;

  let result = markdown_to_document_data(markdown);
  let code_block = get_block_by_type(&result, "code");
  let delta_json = get_delta_json(&result, &code_block.id);

  assert_eq!(
    delta_json,
    json!([
      {"insert": "fn main() {\n    println!(\"Hello, world!\");\n}"}
    ])
  );

  assert_eq!(
    json!(code_block.data),
    json!({
      "language": "rust"
    })
  );
}

#[test]
fn test_divider() {
  let markdown = "---";

  let result = markdown_to_document_data(markdown);
  let divider = get_block_by_type(&result, "divider");
  assert_eq!(divider.ty, "divider");
}

#[test]
fn test_image() {
  let image_with_title = "![Alt text](https://example.com/image.png \"Image title\")";
  let image_without_title = "![Alt text](https://example.com/image.png)";
  let local_image = "![In the Getty Center auditorium for the recent “There Will Be Food“ panel.](Blog%20Post%20104d4deadd2c808aa7dbd79eadeff0eb/maarten-van-den-heuvel-400626-unsplash.jpg)";
  let local_image_with_desc = r#"
![Dishes at Broken Spanish, in Downtown LA.](Blog%20Post%20104d4deadd2c808aa7dbd79eadeff0eb/christine-siracusa-363257-unsplash.jpg)

Dishes at Broken Spanish, in Downtown LA.
"#;

  let result = markdown_to_document_data(image_with_title);
  let image = get_block_by_type(&result, "image");
  assert_eq!(
    json!(image.data),
    json!({
      "url": "https://example.com/image.png",
      "image_type": 2
    })
  );

  let result = markdown_to_document_data(image_without_title);
  let image = get_block_by_type(&result, "image");
  assert_eq!(
    json!(image.data),
    json!({
      "url": "https://example.com/image.png",
      "image_type": 2
    })
  );

  let result = markdown_to_document_data(local_image);
  let image = get_block_by_type(&result, "image");
  assert_eq!(
    json!(image.data),
    json!({
      "url": "Blog%20Post%20104d4deadd2c808aa7dbd79eadeff0eb/maarten-van-den-heuvel-400626-unsplash.jpg",
      "image_type": 2
    })
  );

  let result = markdown_to_document_data(local_image_with_desc);
  let image = get_block_by_type(&result, "image");
  assert_eq!(
    json!(image.data),
    json!({
      "url": "Blog%20Post%20104d4deadd2c808aa7dbd79eadeff0eb/christine-siracusa-363257-unsplash.jpg",
      "image_type": 2
    })
  );
}

#[test]
fn test_math_equation() {
  let markdown = "$$\nE=mc^2\n$$";

  let result = markdown_to_document_data(markdown);
  let math = get_block_by_type(&result, "math_equation");

  assert_eq!(
    json!(math.data),
    json!({
      "formula": "E=mc^2"
    })
  );
}

#[test]
fn test_link_reference() {
  let markdown = "[link]: https://example.com";

  let result = markdown_to_document_data(markdown);
  let link_preview = get_block_by_type(&result, "link_preview");
  assert_eq!(
    json!(link_preview.data),
    json!({
      "url": "https://example.com"
    })
  );
}

#[test]
fn test_image_reference() {
  let markdown = "[image]: https://example.com/image.png";

  let result = markdown_to_document_data(markdown);
  let image = get_block_by_type(&result, "image");

  assert_eq!(
    json!(image.data),
    json!({
      "url": "https://example.com/image.png",
      "image_type": 2
    })
  );
}

#[test]
fn test_table() {
  let markdown = r#"| Header 1 | Header 2 | Header 3 |
| --- | --- | --- |
| Row 1, Col 0 | Row 1, Col 1 | Row 1, Col 2 |
| Row 2, Col 0 | Row 2, Col 1 | Row 2, Col 2 |
"#;

  let result = markdown_to_document_data(markdown);
  let table = get_block_by_type(&result, "table");

  assert_eq!(table.ty, "table");
  assert_eq!(table.data["rowsLen"], 3);
  assert_eq!(table.data["colsLen"], 3);

  let table_cells = result
    .blocks
    .values()
    .filter(|b| b.ty == "table/cell")
    .collect::<Vec<_>>();

  assert_eq!(table_cells.len(), 9);

  for cell in table_cells.iter() {
    let paragraph_block_id = get_children_blocks(&result, &cell.id)
      .first()
      .unwrap()
      .id
      .clone();
    let delta_json = get_delta_json(&result, &paragraph_block_id);

    let row_position = cell.data["rowPosition"].as_u64().unwrap();
    let col_position = cell.data["colPosition"].as_u64().unwrap();

    if row_position == 0 {
      let expected_delta = json!([
          {"insert": format!("Header {}", col_position + 1)}
      ]);
      assert_eq!(delta_json, expected_delta);
    } else {
      let expected_delta = json!([
          {"insert": format!("Row {}, Col {}", row_position, col_position)}
      ]);
      assert_eq!(delta_json, expected_delta);
    }
  }
}

#[test]
fn test_aside() {
  let markdown = r#"<aside>
💡 **Notion Tip:** Create a new page and select `Daily entry` ****from the list of template options to automatically generate the format below every day.

</aside>"#;

  let result = markdown_to_document_data(markdown);

  let page = get_page_block(&result);
  let paragraphs = get_children_blocks(&result, &page.id);

  assert_eq!(paragraphs.len(), 2);

  let first_paragraph = paragraphs.first().unwrap();
  let delta_json = get_delta_json(&result, &first_paragraph.id);
  let expected_delta = json!([
      {"insert": "<aside>\n💡 **Notion Tip:** Create a new page and select `Daily entry` ****from the list of template options to automatically generate the format below every day."},
  ]);

  assert_eq!(delta_json, expected_delta);

  let second_paragraph = paragraphs.last().unwrap();
  let delta_json = get_delta_json(&result, &second_paragraph.id);
  let expected_delta = json!([
      {"insert": "</aside>"}
  ]);
  assert_eq!(delta_json, expected_delta);
}
