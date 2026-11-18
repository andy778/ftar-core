use ftar_core::edit::*;
use ftar_core::parse::{parse_fta, parse_ped};
use ftar_core::write::{write_fta, write_fta_with, write_ped, LineEnding};
use ftar_core::{EventType, Tree};

fn data(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

const FTA: &[&str] = &["bottom.fta", "lift.fta", "middle.fta", "theatre.fta", "top.fta"];
const PED: &[&str] = &["bottom.ped", "lift.ped", "theatre.ped"];

#[test]
fn fta_round_trip() {
    for f in FTA {
        let raw = data(f);
        let t = parse_fta(&raw).unwrap();
        let out = write_fta(&t);
        assert_eq!(parse_fta(out.as_bytes()).unwrap(), t, "{f}");
        let crlf = write_fta_with(&t, LineEnding::CrLf);
        assert_eq!(parse_fta(crlf.as_bytes()).unwrap(), t, "{f} crlf");
    }
}

#[test]
fn fta_byte_identical_where_possible() {
    for f in FTA {
        let raw = data(f);
        let raw_s = String::from_utf8_lossy(&raw).replace("\r\n", "\n");
        let out = write_fta(&parse_fta(&raw).unwrap());
        assert_eq!(out, raw_s, "{f}");
    }
}

#[test]
fn ped_round_trip() {
    for f in PED {
        let db = parse_ped(&data(f)).unwrap();
        let out = write_ped(&db);
        assert_eq!(parse_ped(out.as_bytes()).unwrap(), db, "{f}");
    }
}

#[test]
fn ped_precision_survives() {
    let mut db = parse_ped(&data("bottom.ped")).unwrap();
    let id = db.events.keys().next().unwrap().clone();
    db.events.get_mut(&id).unwrap().probability = 0.123456789012;
    let back = parse_ped(write_ped(&db).as_bytes()).unwrap();
    assert_eq!(back, db);
}

fn small() -> Tree {
    let mut t = Tree {
        db_name: None,
        note: String::new(),
        root: new_node(EventType::Intermediate, "TOP"),
    };
    insert_child(&mut t, &[], 0, new_node(EventType::Or, "")).unwrap();
    insert_child(&mut t, &[0], 0, new_node(EventType::Basic, "A")).unwrap();
    insert_child(&mut t, &[0], 1, new_node(EventType::Basic, "B")).unwrap();
    t
}

#[test]
fn insert_rules() {
    let mut t = small();
    assert!(validate(&t).is_empty());
    // second real child of intermediate
    assert_eq!(
        insert_child(&mut t, &[], 1, new_node(EventType::Basic, "C")),
        Err(EditError::TooManyChildren)
    );
    // gate under gate
    assert_eq!(
        insert_child(&mut t, &[0], 2, new_node(EventType::And, "")),
        Err(EditError::GateUnderGate)
    );
    // child under primary
    assert_eq!(
        insert_child(&mut t, &[0, 0], 0, new_node(EventType::Basic, "X")),
        Err(EditError::ChildrenNotAllowed(EventType::Basic))
    );
    // transfer-out only first child of the root
    assert_eq!(
        insert_child(&mut t, &[0], 0, new_node(EventType::TransferOut, "T")),
        Err(EditError::TransferOutPlacement)
    );
    assert_eq!(insert_child(&mut t, &[], 0, new_node(EventType::TransferOut, "T")), Ok(vec![0]));
    assert!(parse_fta(write_fta(&t).as_bytes()).is_ok());
    // condition: one per gate
    insert_child(&mut t, &[1], 2, new_node(EventType::ConditionAnalysed, "K")).unwrap();
    assert_eq!(
        insert_child(&mut t, &[1], 3, new_node(EventType::ConditionAnalysed, "K2")),
        Err(EditError::ConditionNotAllowed)
    );
    assert_eq!(
        insert_child(&mut t, &[], 1, new_node(EventType::ConditionAnalysed, "K")),
        Err(EditError::ConditionNotAllowed)
    );
}

#[test]
fn inhibit_and_validate() {
    let mut t = small();
    change_type(&mut t, &[0], EventType::Inhibit, None).unwrap_err(); // two plain events
    delete_subtree(&mut t, &[0, 1]).unwrap();
    change_type(&mut t, &[0], EventType::Inhibit, None).unwrap();
    assert_eq!(validate(&t).len(), 1);
    insert_child(&mut t, &[0], 1, new_node(EventType::ConditionNotAnalysed, "C")).unwrap();
    assert!(validate(&t).is_empty());
    assert_eq!(
        insert_child(&mut t, &[0], 2, new_node(EventType::Basic, "Z")),
        Err(EditError::TooManyChildren)
    );
}

#[test]
fn delete_move_copy() {
    let mut t = small();
    assert_eq!(delete_subtree(&mut t, &[]), Err(EditError::CannotDeleteRoot));
    assert_eq!(delete_subtree(&mut t, &[5]), Err(EditError::InvalidPath));
    let clip = copy_subtree(&t, &[0, 0]).unwrap();
    assert_eq!(reorder(&mut t, &[0, 0], 1), Ok(vec![0, 1]));
    assert_eq!(node_at(&t, &[0, 0]).unwrap().id, "B");
    assert_eq!(node_at(&t, &[0, 1]).unwrap().id, "A");
    paste_subtree(&mut t, &[0], 2, &clip).unwrap();
    assert_eq!(node_at(&t, &[0]).unwrap().children.len(), 3);
    assert_eq!(move_subtree(&mut t, &[0], &[0, 1], 0), Err(EditError::MoveIntoOwnSubtree));
    // failed move leaves the tree unchanged
    let before = t.clone();
    assert!(move_subtree(&mut t, &[0, 0], &[], 0).is_err());
    assert_eq!(t, before);
    assert_eq!(all_paths(&t).len(), 5);
}

#[test]
fn ids_text_types() {
    let mut t = small();
    set_id(&mut t, &[0, 0], "NEW").unwrap();
    assert!(set_id(&mut t, &[0, 0], "a b").is_err());
    set_text(&mut t, &[], "Top event").unwrap();
    assert_eq!(set_text(&mut t, &[0], "x"), Err(EditError::NotIntermediate));
    assert!(set_text(&mut t, &[], "two\nlines").is_err());
    change_type(&mut t, &[0, 1], EventType::Basic, Some('D')).unwrap();
    assert_eq!(node_at(&t, &[0, 1]).unwrap().modifier, Some('D'));
    change_type(&mut t, &[0, 1], EventType::TransferIn, Some('D')).unwrap();
    assert_eq!(node_at(&t, &[0, 1]).unwrap().modifier, None);
    assert!(change_type(&mut t, &[0], EventType::Basic, None).is_err());
    assert!(change_type(&mut t, &[], EventType::And, None).is_err()); // And root w/ gate child
    let back = parse_fta(write_fta(&t).as_bytes()).unwrap();
    assert_eq!(back, t);
}

#[test]
fn history_undo_redo() {
    let mut t = small();
    let orig = t.clone();
    let mut h: History<Tree> = History::new();
    h.edit(&mut t, |t| delete_subtree(t, &[0, 0])).unwrap();
    let after_delete = t.clone();
    // failing edit records nothing and leaves tree unchanged
    assert!(h.edit(&mut t, |t| delete_subtree(t, &[])).is_err());
    assert_eq!(t, after_delete);
    h.edit(&mut t, |t| set_id(t, &[], "X")).unwrap();
    t = h.undo(&t).unwrap();
    assert_eq!(t, after_delete);
    t = h.undo(&t).unwrap();
    assert_eq!(t, orig);
    assert!(!h.can_undo() && h.can_redo());
    t = h.redo(&t).unwrap();
    assert_eq!(t, after_delete);
    h.edit(&mut t, |t| set_id(t, &[], "Y")).unwrap();
    assert!(!h.can_redo());
}
