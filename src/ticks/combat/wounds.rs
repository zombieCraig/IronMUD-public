//! Unified wound handling for characters and mobiles

use ironmud::{BodyPart, CharacterData, MobileData, Wound, WoundLevel, WoundType};

/// Trait for entities that can receive wounds (characters and mobiles)
pub trait Woundable {
    /// Get mutable access to the entity's wounds
    fn wounds_mut(&mut self) -> &mut Vec<Wound>;
}

impl Woundable for CharacterData {
    fn wounds_mut(&mut self) -> &mut Vec<Wound> {
        &mut self.wounds
    }
}

impl Woundable for MobileData {
    fn wounds_mut(&mut self) -> &mut Vec<Wound> {
        &mut self.wounds
    }
}

/// Add bleeding to a wound on a specific body part.
/// If a wound exists on that body part, increase its bleeding severity.
/// Otherwise, create a new wound with the specified bleeding severity.
pub fn add_wound_bleeding<T: Woundable>(
    entity: &mut T,
    body_part: &str,
    severity: i32,
) {
    let target_bp = BodyPart::from_str(body_part).unwrap_or(BodyPart::Torso);
    let wounds = entity.wounds_mut();

    // Find existing wound on this body part
    if let Some(wound) = wounds.iter_mut().find(|w| w.body_part == target_bp) {
        wound.bleeding_severity += severity;
    } else {
        // Create a new wound with bleeding
        wounds.push(Wound {
            body_part: target_bp,
            level: WoundLevel::Minor,
            wound_type: WoundType::Cut,
            bleeding_severity: severity,
        });
    }
}

/// Escalate a wound to Severe level (limb disable).
/// If a wound exists on that body part, set it to Severe.
/// Otherwise, create a new Severe wound.
pub fn escalate_wound_to_severe<T: Woundable>(
    entity: &mut T,
    body_part: &str,
) {
    let target_bp = BodyPart::from_str(body_part).unwrap_or(BodyPart::Torso);
    let wounds = entity.wounds_mut();

    // Find existing wound on this body part
    if let Some(wound) = wounds.iter_mut().find(|w| w.body_part == target_bp) {
        wound.level = WoundLevel::Severe;
    } else {
        // Create a new Severe wound
        wounds.push(Wound {
            body_part: target_bp,
            level: WoundLevel::Severe,
            wound_type: WoundType::Fracture,
            bleeding_severity: 0,
        });
    }
}
