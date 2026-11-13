//! Deleting a character removes everything that names them, so a new
//! character later created under the same name inherits none of it.

use ironmud::db::Db;
use ironmud::types::{ApiKey, ApiPermissions, ConsignmentListing, ItemData, ItemLocation};
use serde_json::json;
use uuid::Uuid;

fn character(db: &Db, name: &str) {
    let ch: ironmud::CharacterData = serde_json::from_value(json!({
        "name": name,
        "password_hash": "",
        "current_room_id": Uuid::nil(),
    }))
    .expect("build character");
    db.save_character_data(ch).expect("save character");
}

fn item_at(db: &Db, location: ItemLocation) -> Uuid {
    let mut item = ItemData::new("thing".into(), "a thing".into(), String::new());
    item.location = location;
    let id = item.id;
    db.save_item_data(item).expect("save item");
    id
}

#[test]
fn deleting_a_character_cascades() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(temp.path()).unwrap();
    character(&db, "Mara");
    character(&db, "Other");

    let carried = item_at(&db, ItemLocation::Inventory("mara".into()));
    let worn = item_at(&db, ItemLocation::Equipped("mara".into()));
    let theirs = item_at(&db, ItemLocation::Inventory("other".into()));
    let listed = item_at(&db, ItemLocation::Nowhere);
    let listing = ConsignmentListing::new(listed, "thing".into(), "Mara".into(), "broker".into(), 10, 0, 3600);
    db.save_consignment(&listing).unwrap();

    let key_id = Uuid::new_v4();
    db.save_api_key(&ApiKey {
        id: key_id,
        key_hash: String::new(),
        name: "mara-key".into(),
        owner_character: "Mara".into(),
        permissions: ApiPermissions {
            read: true,
            write: true,
            admin: false,
            player_logs: None,
        },
        created_at: 0,
        last_used_at: None,
        enabled: true,
    })
    .unwrap();

    let area_id = Uuid::new_v4();
    let area: ironmud::types::AreaData = serde_json::from_value(json!({
        "id": area_id, "name": "n", "prefix": "p",
        "level_min": 1, "level_max": 5,
        "owner": "Mara",
        "trusted_builders": ["mara", "Other"],
    }))
    .expect("build area");
    db.save_area_data(area).unwrap();

    db.delete_character_data("mara").unwrap();

    assert!(db.get_item_data(&carried).unwrap().is_none(), "carried item");
    assert!(db.get_item_data(&worn).unwrap().is_none(), "worn item");
    assert!(db.get_item_data(&listed).unwrap().is_none(), "listed item");
    assert!(db.get_consignment(&listing.id).unwrap().is_none(), "listing");
    assert!(db.get_api_key(&key_id).unwrap().is_none(), "API key");
    let area = db.get_area_data(&area_id).unwrap().unwrap();
    assert_eq!(area.owner, None);
    assert_eq!(area.trusted_builders, vec!["Other".to_string()]);
    assert!(
        db.get_item_data(&theirs).unwrap().is_some(),
        "someone else's item is untouched"
    );
}
