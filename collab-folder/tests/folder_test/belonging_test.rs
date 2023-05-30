use crate::util::{create_folder_with_workspace, make_test_view};
use collab_folder::core::ViewIdentifier;

#[test]
fn create_belongings_test() {
  let folder_test = create_folder_with_workspace("1", "w1");
  let view_1_1 = make_test_view("1_1", "1", vec![]);
  let view_1_2 = make_test_view("1_2", "1", vec![]);
  let view_1_2_1 = make_test_view("1_2_1", "1_2", vec![]);
  let view_1_2_2 = make_test_view("1_2_2", "1_2", vec![]);
  let view_1_3 = make_test_view("1_3", "1", vec![]);
  let view_1 = make_test_view("1", "w1", vec![]);

  folder_test.insert_view(view_1.clone());
  folder_test.insert_view(view_1_1);
  folder_test.insert_view(view_1_2.clone());
  folder_test.insert_view(view_1_2_1);
  folder_test.insert_view(view_1_2_2);
  folder_test.insert_view(view_1_3);

  let belongings = folder_test
    .view_relations
    .get_children(&view_1.id)
    .unwrap()
    .get_children();
  assert_eq!(belongings.len(), 3);

  let belongings = folder_test
    .view_relations
    .get_children(&view_1_2.id)
    .unwrap()
    .get_children();
  assert_eq!(belongings.len(), 2);
}

#[test]
fn move_belongings_test() {
  let folder_test = create_folder_with_workspace("1", "w1");
  let view_1_1 = make_test_view("1_1", "1", vec![]);
  let view_1_2 = make_test_view("1_2", "1", vec![]);
  let view_1_3 = make_test_view("1_3", "1", vec![]);
  let view_1 = make_test_view(
    "1",
    "w1",
    vec!["1_1".to_string(), "1_2".to_string(), "1_3".to_string()],
  );

  folder_test.insert_view(view_1.clone());
  folder_test.insert_view(view_1_1);
  folder_test.insert_view(view_1_2);
  folder_test.insert_view(view_1_3);

  let belonging_array = folder_test.view_relations.get_children(&view_1.id).unwrap();
  let belongings = belonging_array.get_children();
  assert_eq!(belongings[0].id, "1_1");
  assert_eq!(belongings[1].id, "1_2");
  assert_eq!(belongings[2].id, "1_3");

  belonging_array.move_child(2, 0);
  belonging_array.move_child(0, 1);

  let view = folder_test.views.get_view(&view_1.id).unwrap();
  assert_eq!(view.children[0].id, "1_1");
  assert_eq!(view.children[1].id, "1_3");
  assert_eq!(view.children[2].id, "1_2");
}

#[test]
fn delete_belongings_test() {
  let folder_test = create_folder_with_workspace("1", "w1");
  let belonging_array = folder_test.view_relations.get_children("w1").unwrap();
  belonging_array.add_children(vec![
    ViewIdentifier::new("1_1".to_string()),
    ViewIdentifier::new("1_2".to_string()),
    ViewIdentifier::new("1_3".to_string()),
  ]);

  let view_1_1 = make_test_view("1_1", "w1", vec![]);
  let view_1_2 = make_test_view("1_2", "w1", vec![]);
  let view_1_3 = make_test_view("1_3", "w1", vec![]);
  folder_test.insert_view(view_1_1);
  folder_test.insert_view(view_1_2);
  folder_test.insert_view(view_1_3);

  let belonging_array = folder_test.view_relations.get_children("w1").unwrap();
  belonging_array.remove_child(1);
  let belongings = belonging_array.get_children();
  assert_eq!(belongings[0].id, "1_1");
  assert_eq!(belongings[1].id, "1_3");
}

#[test]
fn delete_duplicate_belongings_test() {
  let folder_test = create_folder_with_workspace("1", "w1");
  let belonging_array = folder_test.view_relations.get_children("w1").unwrap();
  belonging_array.add_children(vec![
    ViewIdentifier::new("1_1".to_string()),
    ViewIdentifier::new("1_2".to_string()),
    ViewIdentifier::new("1_2".to_string()),
    ViewIdentifier::new("1_3".to_string()),
    ViewIdentifier::new("1_3".to_string()),
  ]);

  let belongings = belonging_array.get_children();
  assert_eq!(belongings.len(), 3);
  assert_eq!(belongings[0].id, "1_1");
  assert_eq!(belongings[1].id, "1_2");
  assert_eq!(belongings[2].id, "1_3");
}
