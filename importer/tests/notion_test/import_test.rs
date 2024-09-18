use crate::util::{parse_csv, print_view, unzip};
use assert_json_diff::assert_json_eq;

use collab_database::template::entity::CELL_DATA;
use collab_document::document::gen_document_id;
use importer::notion::{NotionImporter, NotionView};
use nanoid::nanoid;
use serde_json::{json, Value};

#[tokio::test]
async fn import_project_and_task_test2() {
  let parent_dir = nanoid!(6);
  let (_cleaner, file_path) = unzip("project&task", &parent_dir).unwrap();
  let importer = NotionImporter::new(&file_path).unwrap();
  let imported_view = importer.import().await.unwrap();
  assert!(!imported_view.views.is_empty());
  assert_eq!(imported_view.name, "project&task");
  assert_eq!(imported_view.num_of_csv(), 2);
  assert_eq!(imported_view.num_of_markdown(), 1);

  /*
  - Projects & Tasks: Markdown
  - Tasks: CSV
  - Projects: CSV
  */
  let root_view = &imported_view.views[0];
  assert_eq!(root_view.notion_name, "Projects & Tasks");
  assert_eq!(imported_view.views.len(), 1);
  let expected = vec![
    json!([{"insert":"Projects & Tasks"}]),
    json!([{"attributes":{"href":"Projects%20&%20Tasks%20104d4deadd2c805fb3abcaab6d3727e7/Projects%2058b8977d6e4444a98ec4d64176a071e5.md"},"insert":"Projects"},{"insert":": This is your overview of all the projects in the pipeline"}]),
    json!([{"attributes":{"href":"Projects%20&%20Tasks%20104d4deadd2c805fb3abcaab6d3727e7/Tasks%2076aaf8a4637542ed8175259692ca08bb.md"},"insert":"Tasks"},{"attributes":{"bold":true},"insert":":"},{"insert":" This is your detailed breakdown of every task under your projects"}]),
    json!([{"attributes":{"href":"Projects%20&%20Tasks%20104d4deadd2c805fb3abcaab6d3727e7/Tasks%2076aaf8a4637542ed8175259692ca08bb.csv"},"insert":"Tasks"}]),
    json!([{"insert":"↓ Click through the different database tabs to see the same data in different ways"}]),
    json!([{"insert":"Hover over any project name and click "},{"attributes":{"code":true},"insert":"◨ OPEN"},{"insert":" to view more info and its associated tasks"}]),
    json!([{"attributes":{"href":"Projects%20&%20Tasks%20104d4deadd2c805fb3abcaab6d3727e7/Projects%2058b8977d6e4444a98ec4d64176a071e5.csv"},"insert":"Projects"}]),
  ];
  check_document(root_view, expected).await;

  let linked_views = root_view.get_linked_views();
  assert_eq!(linked_views.len(), 2);
  assert_eq!(linked_views[0].notion_name, "Tasks");
  assert_eq!(linked_views[1].notion_name, "Projects");

  check_database_view(&linked_views[0], "Tasks", 17, 13).await;
  check_database_view(&linked_views[1], "Projects", 4, 11).await;

  let views = root_view.get_external_link_notion_view();
  assert_eq!(views.len(), 2);
  assert_eq!(views[0].notion_id, linked_views[0].notion_id);
  assert_eq!(views[1].notion_id, linked_views[1].notion_id);
}

async fn replace_links(document_view: &NotionView, linked_views: Vec<NotionView>) {
  let document_id = gen_document_id();
  let document = document_view.as_document(&document_id).await.unwrap();
}

async fn check_document(document_view: &NotionView, expected: Vec<Value>) {
  let document_id = gen_document_id();
  let document = document_view.as_document(&document_id).await.unwrap();
  let first_block_id = document.get_page_id().unwrap();
  let block_ids = document.get_block_children_ids(&first_block_id);
  for (index, block_id) in block_ids.iter().enumerate() {
    let delta = document.get_delta_json(block_id).unwrap();
    assert_json_eq!(delta, expected[index]);
  }
}

async fn check_database_view(
  linked_view: &NotionView,
  expected_name: &str,
  expected_rows_count: usize,
  expected_fields_count: usize,
) {
  assert_eq!(linked_view.notion_name, expected_name);

  let (csv_fields, csv_rows) = parse_csv(linked_view.notion_file.file_path().unwrap());
  let database = linked_view.as_database().await.unwrap();
  let fields = database.get_fields_in_view(&database.get_inline_view_id(), None);
  let rows = database.collect_all_rows().await;
  assert_eq!(rows.len(), expected_rows_count);
  assert_eq!(fields.len(), csv_fields.len());
  assert_eq!(fields.len(), expected_fields_count);

  for (index, field) in csv_fields.iter().enumerate() {
    assert_eq!(&fields[index].name, field);
  }
  for (row_index, row) in rows.into_iter().enumerate() {
    let row = row.unwrap();
    assert_eq!(row.cells.len(), fields.len());
    for (field_index, field) in fields.iter().enumerate() {
      let cell = row
        .cells
        .get(&field.id)
        .unwrap()
        .get(CELL_DATA)
        .cloned()
        .unwrap();
      let cell_data = cell.cast::<String>().unwrap();
      assert_eq!(
        cell_data, csv_rows[row_index][field_index],
        "Row: {}, Field: {}:{}",
        row_index, field.name, field_index
      );
    }
  }
}

#[tokio::test]
async fn test_importer() {
  let parent_dir = nanoid!(6);
  let (_cleaner, file_path) = unzip("import_test", &parent_dir).unwrap();
  let importer = NotionImporter::new(&file_path).unwrap();
  let imported_view = importer.import().await.unwrap();
  assert!(!imported_view.views.is_empty());
  assert_eq!(imported_view.name, "import_test");

  /*
  - Root2:Markdown
    - root2-link:Markdown
  - Home:Markdown
    - Home views:Markdown
    - My tasks:Markdown
  - Root:Markdown
    - root-2:Markdown
      - root-2-1:Markdown
        - root-2-database:CSV
    - root-1:Markdown
      - root-1-1:Markdown
    - root 3:Markdown
      - root 3 1:Markdown
      */
  for view in imported_view.views {
    print_view(&view, 0);
  }
}
