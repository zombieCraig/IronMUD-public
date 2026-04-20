use anyhow::Result;

use crate::db::Db;
use crate::types::PropertyTemplate;

use super::seed_uuid;

pub fn seed_properties(db: &Db) -> Result<()> {
    // Cozy Cottage — rentable property in Oakvale
    let cottage = PropertyTemplate {
        id: seed_uuid("property:cozy_cottage"),
        vnum: "oakvale:cozy_cottage".to_string(),
        name: "A Cozy Cottage".to_string(),
        description: "A charming stone cottage with a thatched roof, perfect for an adventurer \
            seeking a place to call home. Comes with a storage chest and a warm hearth."
            .to_string(),
        monthly_rent: 50,
        entrance_room_id: seed_uuid("oakvale:cottage_entry"),
        max_instances: 0, // Unlimited
        level_requirement: 0,
        area_id: Some(seed_uuid("area:oakvale")),
    };

    db.save_property_template(&cottage)?;

    tracing::info!("Seeded 1 property template (Cozy Cottage)");
    Ok(())
}
