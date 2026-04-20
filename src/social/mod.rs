//! Social simulation helpers for pair-bonded NPCs.
//!
//! `apply_mood` computes the derived [`MoodState`] from a mobile's current
//! `happiness` and applies/refreshes the corresponding buff on
//! `MobileData.active_buffs`. The function is pure w.r.t. the mobile passed in —
//! callers are responsible for persisting the mutation.
//!
//! Mood buffs are tagged with `source = MOOD_BUFF_SOURCE` so each call can
//! cleanly retire prior mood effects without touching other active buffs
//! (combat buffs, consumables, etc.).

use rand::Rng;

use crate::db::Db;
use crate::types::{ActiveBuff, EffectType, MobileData, MoodState, Relationship, RelationshipKind};
use uuid::Uuid;

pub const MOOD_BUFF_SOURCE: &str = "mood";
const MOOD_BUFF_DURATION_SECS: i32 = 600;

/// Map `happiness` to a [`MoodState`]. Thresholds are inclusive on the top end.
pub fn mood_for_happiness(happiness: i32) -> MoodState {
    match happiness {
        h if h >= 85 => MoodState::Content,
        h if h >= 40 => MoodState::Normal,
        h if h > 20 => MoodState::Sad,
        h if h >= 5 => MoodState::Depressed,
        _ => MoodState::Breakdown,
    }
}

/// Recompute `mobile.social.mood` from happiness and refresh mood-tagged buffs.
/// Returns true if the stored mood changed this call.
pub fn apply_mood(mobile: &mut MobileData) -> bool {
    let Some(social) = mobile.social.as_mut() else {
        return false;
    };

    let new_mood = mood_for_happiness(social.happiness);
    let changed = new_mood != social.mood;
    social.mood = new_mood;

    mobile.active_buffs.retain(|b| b.source != MOOD_BUFF_SOURCE);

    match new_mood {
        MoodState::Content => {
            mobile.active_buffs.push(ActiveBuff {
                effect_type: EffectType::StrengthBoost,
                magnitude: 1,
                remaining_secs: MOOD_BUFF_DURATION_SECS,
                source: MOOD_BUFF_SOURCE.to_string(),
            });
            mobile.active_buffs.push(ActiveBuff {
                effect_type: EffectType::WisdomBoost,
                magnitude: 1,
                remaining_secs: MOOD_BUFF_DURATION_SECS,
                source: MOOD_BUFF_SOURCE.to_string(),
            });
        }
        MoodState::Normal | MoodState::Sad => {}
        MoodState::Depressed => {
            mobile.active_buffs.push(ActiveBuff {
                effect_type: EffectType::StrengthBoost,
                magnitude: -1,
                remaining_secs: MOOD_BUFF_DURATION_SECS,
                source: MOOD_BUFF_SOURCE.to_string(),
            });
            mobile.active_buffs.push(ActiveBuff {
                effect_type: EffectType::WisdomBoost,
                magnitude: -2,
                remaining_secs: MOOD_BUFF_DURATION_SECS,
                source: MOOD_BUFF_SOURCE.to_string(),
            });
        }
        MoodState::Breakdown => {
            mobile.active_buffs.push(ActiveBuff {
                effect_type: EffectType::StrengthBoost,
                magnitude: -3,
                remaining_secs: MOOD_BUFF_DURATION_SECS,
                source: MOOD_BUFF_SOURCE.to_string(),
            });
            mobile.active_buffs.push(ActiveBuff {
                effect_type: EffectType::WisdomBoost,
                magnitude: -4,
                remaining_secs: MOOD_BUFF_DURATION_SECS,
                source: MOOD_BUFF_SOURCE.to_string(),
            });
        }
    }

    changed
}

/// Affinity at-or-below this floor on the dying side means the survivor hated
/// the deceased and feels no grief. Used by `grief_params` and family Rhai
/// bindings.
pub const GRIEF_AFFINITY_FLOOR: i32 = -30;

/// Cohabitant grief parameters: (-happiness delta, days bereaved).
pub const COHABITANT_GRIEF: (i32, i32) = (-40, 14);

/// Family (Partner/Parent/Child/Sibling) grief parameters: larger hit, longer
/// mourning window than a non-family cohabitant.
pub const FAMILY_GRIEF: (i32, i32) = (-60, 28);

/// Compute the happiness delta and bereavement window to apply when a mobile
/// with the given relationship to the deceased learns of the death. Returns
/// `None` when affinity is deep enough in the negative that they felt no
/// attachment worth mourning.
///
/// `affinity` is the survivor's stored affinity toward the deceased.
pub fn grief_params(kind: RelationshipKind, affinity: i32) -> Option<(i32, i32)> {
    if affinity <= GRIEF_AFFINITY_FLOOR {
        return None;
    }
    let (base_delta, days) = match kind {
        RelationshipKind::Partner | RelationshipKind::Parent | RelationshipKind::Child | RelationshipKind::Sibling => {
            FAMILY_GRIEF
        }
        RelationshipKind::Cohabitant => COHABITANT_GRIEF,
        RelationshipKind::Friend => return None,
    };
    // Mildly negative affinity halves the grief — they had drifted apart.
    let scaled = if affinity < 0 {
        (base_delta as f32 * 0.5).round() as i32
    } else {
        base_delta
    };
    Some((scaled, days))
}

/// Apply a family-affinity bias to a raw conversation delta. Family kinds
/// (Partner/Parent/Child/Sibling) dampen negative deltas (0.5×) and boost
/// positive deltas (1.2×) so direct kin are slower to dislike each other but
/// still accumulate resentment over time if pushed.
pub fn apply_family_bias(delta: i32, kind: RelationshipKind) -> i32 {
    match kind {
        RelationshipKind::Partner | RelationshipKind::Parent | RelationshipKind::Child | RelationshipKind::Sibling => {
            if delta < 0 {
                (delta as f32 * 0.5).round() as i32
            } else {
                (delta as f32 * 1.2).round() as i32
            }
        }
        RelationshipKind::Friend | RelationshipKind::Cohabitant => delta,
    }
}

/// Maximum female age at which conception can occur. Keep in sync with
/// `is_fertile_female` — this is the hard cap, stage still matters.
pub const FEMALE_FERTILITY_CAP: i32 = 45;

/// Maximum male age at which conception can occur.
pub const MALE_FERTILITY_CAP: i32 = 65;

/// True if the mobile is a viable biological mother for conception this tick.
/// Uses Characteristics age + LifeStage + gender + existing pregnancy state.
pub fn is_fertile_female(mobile: &MobileData, current_game_day: i32) -> bool {
    let Some(chars) = mobile.characteristics.as_ref() else {
        return false;
    };
    if chars.gender != "female" {
        return false;
    }
    if chars.age >= FEMALE_FERTILITY_CAP {
        return false;
    }
    use crate::types::{LifeStage, life_stage_for_age};
    if !matches!(life_stage_for_age(chars.age), LifeStage::YoungAdult | LifeStage::Adult) {
        return false;
    }
    let Some(social) = mobile.social.as_ref() else {
        return false;
    };
    if social.pregnant_until_day.is_some() {
        return false;
    }
    if social.bereaved_until_day.map(|d| d > current_game_day).unwrap_or(false) {
        return false;
    }
    true
}

/// True if the mobile is a viable biological father for conception this tick.
pub fn is_fertile_male(mobile: &MobileData) -> bool {
    let Some(chars) = mobile.characteristics.as_ref() else {
        return false;
    };
    if chars.gender != "male" {
        return false;
    }
    if chars.age >= MALE_FERTILITY_CAP {
        return false;
    }
    use crate::types::{LifeStage, life_stage_for_age};
    matches!(
        life_stage_for_age(chars.age),
        LifeStage::YoungAdult | LifeStage::Adult | LifeStage::MiddleAged
    )
}

/// Minimum affinity for the partner side before conception rolls. Below this
/// the pair is in drift / estrangement and shouldn't be starting families.
pub const CONCEPTION_MIN_AFFINITY: i32 = 50;

/// Return the partner's id if `mobile` has an eligible mate for conception:
/// a Partner/Cohabitant with affinity >= CONCEPTION_MIN_AFFINITY sharing
/// `resident_of` and `household_id`.
pub fn eligible_mate(db: &Db, mobile: &MobileData) -> Option<Uuid> {
    let my_household = mobile.household_id?;
    let my_vnum = mobile.resident_of.as_deref().filter(|v| !v.is_empty())?;
    for rel in &mobile.relationships {
        if !matches!(rel.kind, RelationshipKind::Partner | RelationshipKind::Cohabitant) {
            continue;
        }
        if rel.affinity < CONCEPTION_MIN_AFFINITY {
            continue;
        }
        let Ok(Some(other)) = db.get_mobile_data(&rel.other_id) else {
            continue;
        };
        if other.current_hp <= 0 {
            continue;
        }
        if other.household_id != Some(my_household) {
            continue;
        }
        if other.resident_of.as_deref() != Some(my_vnum) {
            continue;
        }
        return Some(rel.other_id);
    }
    None
}

/// Starting affinity for a newly-adopted child ↔ adoptive parent (and
/// partner if present). Warmer than a stranger, cooler than biological kin.
pub const ADOPTION_STARTING_AFFINITY: i32 = 50;

/// Adoption weight for a single sim adult with no partner.
pub const ADOPT_WEIGHT_SINGLE: f32 = 1.0;
/// Weight for an opposite-gender Partner/Cohabitant pair.
pub const ADOPT_WEIGHT_OPPOSITE_PAIR: f32 = 2.0;
/// Weight for a same-gender Partner/Cohabitant pair — elevated so the
/// biologically-infertile cohort has first shot at parenting orphans.
pub const ADOPT_WEIGHT_SAME_PAIR: f32 = 3.0;

/// True if this mobile is a viable adopter: sim adult, alive, not a
/// prototype, not currently deeply mourning a family loss.
pub fn is_eligible_adopter(mobile: &MobileData, current_game_day: i32) -> bool {
    if mobile.is_prototype || mobile.current_hp <= 0 {
        return false;
    }
    let Some(chars) = mobile.characteristics.as_ref() else {
        return false;
    };
    use crate::types::{LifeStage, life_stage_for_age};
    if !matches!(
        life_stage_for_age(chars.age),
        LifeStage::YoungAdult | LifeStage::Adult | LifeStage::MiddleAged
    ) {
        return false;
    }
    // Must be a simulated NPC to have the capacity to care for a child.
    if mobile.social.is_none() {
        return false;
    }
    // Skip mobiles still in active family bereavement — they'd just bring
    // grief into a new home.
    if let Some(social) = mobile.social.as_ref() {
        let mourning_family = social.bereaved_for.iter().any(|n| {
            n.until_day > current_game_day
                && matches!(
                    n.kind,
                    RelationshipKind::Partner
                        | RelationshipKind::Parent
                        | RelationshipKind::Child
                        | RelationshipKind::Sibling
                )
        });
        if mourning_family {
            return false;
        }
    }
    true
}

/// Compute the adoption weight for a single eligible adopter. Inspects the
/// adopter's Partner/Cohabitant bond (if any) and compares gender to choose
/// the appropriate pair bonus. Returns `None` if a partner is referenced
/// but missing/dead (treat as unsuitable rather than downgrading silently).
pub fn adoption_weight(db: &Db, adopter: &MobileData) -> Option<f32> {
    let Some(adopter_chars) = adopter.characteristics.as_ref() else {
        return Some(ADOPT_WEIGHT_SINGLE);
    };
    let partner_rel = adopter
        .relationships
        .iter()
        .find(|r| matches!(r.kind, RelationshipKind::Partner | RelationshipKind::Cohabitant));
    let Some(rel) = partner_rel else {
        return Some(ADOPT_WEIGHT_SINGLE);
    };
    let partner = match db.get_mobile_data(&rel.other_id).ok().flatten() {
        Some(p) if p.current_hp > 0 => p,
        _ => return Some(ADOPT_WEIGHT_SINGLE), // dead/missing partner = treat as single
    };
    let Some(partner_chars) = partner.characteristics.as_ref() else {
        return Some(ADOPT_WEIGHT_OPPOSITE_PAIR);
    };
    let base = if adopter_chars.gender == partner_chars.gender {
        ADOPT_WEIGHT_SAME_PAIR
    } else {
        ADOPT_WEIGHT_OPPOSITE_PAIR
    };
    // Modulate by affinity so a distressed bond is a worse home than a
    // healthy one. Clamp floor so negative-affinity pairs still get some
    // weight (builders may have manually linked them).
    let modulator = ((100 + rel.affinity).max(10) as f32) / 100.0;
    Some(base * modulator)
}

/// Given an orphan and a full mobile list, pick an adopter via weighted
/// random selection restricted to the orphan's area (or same room as
/// fallback when area resolution fails). Returns `None` when no eligible
/// adopter exists.
pub fn pick_adopter<R: Rng>(db: &Db, orphan: &MobileData, mobiles: &[MobileData], rng: &mut R) -> Option<Uuid> {
    // Same-area filter: look up orphan's current room → area_id. If we can't
    // resolve, fall back to same-room comparisons on current_room_id.
    let orphan_area = orphan
        .current_room_id
        .and_then(|rid| db.get_room_data(&rid).ok().flatten())
        .and_then(|r| r.area_id);

    let mut weighted: Vec<(Uuid, f32)> = Vec::new();
    for adopter in mobiles {
        if adopter.id == orphan.id {
            continue;
        }
        // Don't re-adopt if already a parent of this orphan.
        if adopter
            .relationships
            .iter()
            .any(|r| r.other_id == orphan.id && matches!(r.kind, RelationshipKind::Parent))
        {
            continue;
        }
        // Shallow "today" approximation for eligibility mourning check —
        // caller's current day isn't threaded in, so pass a sentinel.
        if !is_eligible_adopter(adopter, i32::MIN) {
            continue;
        }
        // Scope to area when resolvable.
        if let Some(oa) = orphan_area {
            let adopter_area = adopter
                .current_room_id
                .and_then(|rid| db.get_room_data(&rid).ok().flatten())
                .and_then(|r| r.area_id);
            if adopter_area != Some(oa) {
                continue;
            }
        } else if adopter.current_room_id != orphan.current_room_id {
            continue;
        }
        let Some(w) = adoption_weight(db, adopter) else {
            continue;
        };
        if w > 0.0 {
            weighted.push((adopter.id, w));
        }
    }
    if weighted.is_empty() {
        return None;
    }

    // Weighted pick: sum weights, roll u in [0, sum), walk until cumulative > u.
    let total: f32 = weighted.iter().map(|(_, w)| *w).sum();
    let mut roll = rng.r#gen::<f32>() * total;
    for (id, w) in &weighted {
        if roll < *w {
            return Some(*id);
        }
        roll -= *w;
    }
    weighted.last().map(|(id, _)| *id)
}

/// Wire an adoption: reciprocal Parent/Child between adopter and orphan,
/// plus partner-as-second-parent if adopter is pair-bonded. Inherits
/// household_id from adopter (minting fresh if absent), updates orphan's
/// `resident_of` to match adopter, clears `adoption_pending`. Called after
/// `pick_adopter` chooses a target.
pub fn wire_adoption(db: &Db, adopter_id: Uuid, orphan_id: Uuid) -> anyhow::Result<()> {
    let adopter = db
        .get_mobile_data(&adopter_id)?
        .ok_or_else(|| anyhow::anyhow!("wire_adoption: adopter not found"))?;
    // Resolve partner (Partner or Cohabitant, alive).
    let partner_id: Option<Uuid> = adopter
        .relationships
        .iter()
        .find(|r| matches!(r.kind, RelationshipKind::Partner | RelationshipKind::Cohabitant))
        .and_then(|r| db.get_mobile_data(&r.other_id).ok().flatten())
        .filter(|p| p.current_hp > 0)
        .map(|p| p.id);

    // Household: prefer adopter's existing, else partner's, else mint.
    let household_id = adopter
        .household_id
        .or_else(|| partner_id.and_then(|pid| db.get_mobile_data(&pid).ok().flatten().and_then(|p| p.household_id)))
        .unwrap_or_else(Uuid::new_v4);

    let adopter_resident = adopter.resident_of.clone();
    let adopter_room = adopter.current_room_id;

    // Adopter ← Child of orphan
    db.update_mobile(&adopter_id, |m| {
        if m.household_id.is_none() {
            m.household_id = Some(household_id);
        }
        if !m.relationships.iter().any(|r| r.other_id == orphan_id) {
            m.relationships.push(Relationship {
                other_id: orphan_id,
                kind: RelationshipKind::Child,
                affinity: ADOPTION_STARTING_AFFINITY,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
        }
    })?;

    // Partner ← Child of orphan (second parent)
    if let Some(pid) = partner_id {
        db.update_mobile(&pid, |m| {
            if m.household_id.is_none() {
                m.household_id = Some(household_id);
            }
            if !m.relationships.iter().any(|r| r.other_id == orphan_id) {
                m.relationships.push(Relationship {
                    other_id: orphan_id,
                    kind: RelationshipKind::Child,
                    affinity: ADOPTION_STARTING_AFFINITY,
                    last_interaction_day: 0,
                    recent_topics: Vec::new(),
                });
            }
        })?;
    }

    // Orphan: clear flag, copy household + residency, add Parent link(s).
    db.update_mobile(&orphan_id, |m| {
        m.adoption_pending = false;
        m.household_id = Some(household_id);
        m.resident_of = adopter_resident.clone();
        if !m.relationships.iter().any(|r| r.other_id == adopter_id) {
            m.relationships.push(Relationship {
                other_id: adopter_id,
                kind: RelationshipKind::Parent,
                affinity: ADOPTION_STARTING_AFFINITY,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
        }
        if let Some(pid) = partner_id {
            if !m.relationships.iter().any(|r| r.other_id == pid) {
                m.relationships.push(Relationship {
                    other_id: pid,
                    kind: RelationshipKind::Parent,
                    affinity: ADOPTION_STARTING_AFFINITY,
                    last_interaction_day: 0,
                    recent_topics: Vec::new(),
                });
            }
        }
    })?;

    // Move the orphan to the adopter's current room.
    if let Some(room_id) = adopter_room {
        let _ = db.move_mobile_to_room(&orphan_id, &room_id);
    }
    Ok(())
}

/// Drop expired `bereaved_for` entries; caller is responsible for persisting.
/// Returns true if the list shrank.
pub fn prune_bereavement_notes(mobile: &mut MobileData, today: i32) -> bool {
    let Some(social) = mobile.social.as_mut() else {
        return false;
    };
    if social.bereaved_for.is_empty() {
        return false;
    }
    let before = social.bereaved_for.len();
    social.bereaved_for.retain(|n| n.until_day > today);
    social.bereaved_for.len() < before
}

/// Parse a user-facing kind string into the (source, target) reciprocal pair.
/// Partner/Sibling are symmetric; Parent/Child reflect each other. Returns
/// `None` for unknown or non-family kinds (Cohabitant is managed by the
/// migration tick, not by direct family edits).
pub fn parse_family_kinds(kind_str: &str) -> Option<(RelationshipKind, RelationshipKind)> {
    match kind_str.to_lowercase().as_str() {
        "parent" => Some((RelationshipKind::Parent, RelationshipKind::Child)),
        "child" => Some((RelationshipKind::Child, RelationshipKind::Parent)),
        "sibling" => Some((RelationshipKind::Sibling, RelationshipKind::Sibling)),
        "partner" | "spouse" => Some((RelationshipKind::Partner, RelationshipKind::Partner)),
        _ => None,
    }
}

/// Create or update a reciprocal family relationship. Enforces Partner
/// monogamy: returns `Err(FamilyError::PartnerConflict)` if either side
/// already has a Partner to someone else. Both halves are written through
/// `db.update_mobile` — no lock is held across the two calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyError {
    InvalidKind,
    SelfLink,
    MissingMobile,
    PartnerConflict,
}

pub fn set_family_relationship(db: &Db, src: Uuid, tgt: Uuid, kind_str: &str) -> Result<(), FamilyError> {
    if src == tgt {
        return Err(FamilyError::SelfLink);
    }
    let (src_kind, tgt_kind) = parse_family_kinds(kind_str).ok_or(FamilyError::InvalidKind)?;

    let src_snapshot = db
        .get_mobile_data(&src)
        .ok()
        .flatten()
        .ok_or(FamilyError::MissingMobile)?;
    let tgt_snapshot = db
        .get_mobile_data(&tgt)
        .ok()
        .flatten()
        .ok_or(FamilyError::MissingMobile)?;

    if matches!(src_kind, RelationshipKind::Partner) {
        let src_has_other = src_snapshot
            .relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Partner && r.other_id != tgt);
        let tgt_has_other = tgt_snapshot
            .relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Partner && r.other_id != src);
        if src_has_other || tgt_has_other {
            return Err(FamilyError::PartnerConflict);
        }
    }

    fn upsert(m: &mut MobileData, other: Uuid, kind: RelationshipKind) {
        if let Some(rel) = m.relationships.iter_mut().find(|r| r.other_id == other) {
            rel.kind = kind;
        } else {
            m.relationships.push(Relationship {
                other_id: other,
                kind,
                affinity: 0,
                last_interaction_day: 0,
                recent_topics: Vec::new(),
            });
        }
    }

    let _ = db.update_mobile(&src, |m| upsert(m, tgt, src_kind));
    let _ = db.update_mobile(&tgt, |m| upsert(m, src, tgt_kind));
    Ok(())
}

/// Remove the relationship entries linking `src` and `tgt` in both directions.
pub fn unset_family_relationship(db: &Db, src: Uuid, tgt: Uuid) -> bool {
    let a = db
        .update_mobile(&src, |m| m.relationships.retain(|r| r.other_id != tgt))
        .ok()
        .flatten()
        .is_some();
    let b = db
        .update_mobile(&tgt, |m| m.relationships.retain(|r| r.other_id != src))
        .ok()
        .flatten()
        .is_some();
    a && b
}

/// Decrement buff timers and drop expired entries on a simulated mobile.
/// Returns true if any buff expired. Caller persists.
pub fn decay_mobile_buffs(mobile: &mut MobileData, tick_secs: i32) -> bool {
    if mobile.active_buffs.is_empty() {
        return false;
    }
    for buff in mobile.active_buffs.iter_mut() {
        if buff.remaining_secs > 0 {
            buff.remaining_secs = (buff.remaining_secs - tick_secs).max(0);
        }
    }
    let before = mobile.active_buffs.len();
    mobile
        .active_buffs
        .retain(|b| b.remaining_secs == -1 || b.remaining_secs > 0);
    mobile.active_buffs.len() < before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SocialState;

    fn mobile_with_happiness(h: i32) -> MobileData {
        let mut m = MobileData::new("test".to_string());
        let mut s = SocialState::default();
        s.happiness = h;
        m.social = Some(s);
        m
    }

    #[test]
    fn content_applies_positive_buff() {
        let mut m = mobile_with_happiness(95);
        apply_mood(&mut m);
        assert_eq!(m.social.as_ref().unwrap().mood, MoodState::Content);
        assert!(
            m.active_buffs
                .iter()
                .any(|b| b.magnitude > 0 && b.source == MOOD_BUFF_SOURCE)
        );
    }

    #[test]
    fn breakdown_applies_heavy_debuff() {
        let mut m = mobile_with_happiness(2);
        apply_mood(&mut m);
        assert_eq!(m.social.as_ref().unwrap().mood, MoodState::Breakdown);
        assert!(m.active_buffs.iter().any(|b| b.magnitude == -3));
    }

    #[test]
    fn mood_transitions_replace_prior_buffs() {
        let mut m = mobile_with_happiness(95);
        apply_mood(&mut m);
        assert_eq!(m.active_buffs.len(), 2);

        // Crash happiness and re-apply
        m.social.as_mut().unwrap().happiness = 2;
        apply_mood(&mut m);
        assert_eq!(m.active_buffs.len(), 2);
        assert!(m.active_buffs.iter().all(|b| b.magnitude < 0));
    }

    #[test]
    fn decay_removes_expired_buffs() {
        let mut m = mobile_with_happiness(95);
        apply_mood(&mut m);
        assert!(!m.active_buffs.is_empty());
        decay_mobile_buffs(&mut m, MOOD_BUFF_DURATION_SECS + 1);
        assert!(m.active_buffs.is_empty());
    }

    #[test]
    fn family_bias_damps_negatives_boosts_positives() {
        // Family kinds: negative × 0.5, positive × 1.2 (rounded).
        assert_eq!(apply_family_bias(-10, RelationshipKind::Parent), -5);
        assert_eq!(apply_family_bias(-1, RelationshipKind::Child), -1); // round half to even/nearest
        assert_eq!(apply_family_bias(10, RelationshipKind::Sibling), 12);
        assert_eq!(apply_family_bias(5, RelationshipKind::Partner), 6);
        // Non-family kinds pass through unchanged.
        assert_eq!(apply_family_bias(-10, RelationshipKind::Friend), -10);
        assert_eq!(apply_family_bias(10, RelationshipKind::Cohabitant), 10);
    }

    #[test]
    fn grief_params_scale_with_affinity() {
        // Deeply negative affinity = no grief.
        assert_eq!(grief_params(RelationshipKind::Parent, GRIEF_AFFINITY_FLOOR), None);
        assert_eq!(grief_params(RelationshipKind::Sibling, -50), None);
        // Mildly negative: halved delta.
        let (d, days) = grief_params(RelationshipKind::Partner, -5).unwrap();
        assert_eq!(d, -30);
        assert_eq!(days, 28);
        // Healthy: full family grief.
        assert_eq!(grief_params(RelationshipKind::Child, 80), Some(FAMILY_GRIEF));
        // Cohabitant curve intact.
        assert_eq!(grief_params(RelationshipKind::Cohabitant, 80), Some(COHABITANT_GRIEF));
        assert_eq!(grief_params(RelationshipKind::Friend, 80), None);
    }

    #[test]
    fn prune_drops_expired_bereavement_notes() {
        use crate::types::BereavementNote;
        use uuid::Uuid;

        let mut m = mobile_with_happiness(50);
        let social = m.social.as_mut().unwrap();
        social.bereaved_for = vec![
            BereavementNote {
                other_id: Uuid::new_v4(),
                other_name: "old loss".to_string(),
                kind: RelationshipKind::Parent,
                until_day: 50,
            },
            BereavementNote {
                other_id: Uuid::new_v4(),
                other_name: "fresh loss".to_string(),
                kind: RelationshipKind::Sibling,
                until_day: 200,
            },
        ];
        assert!(prune_bereavement_notes(&mut m, 100));
        let remaining = &m.social.as_ref().unwrap().bereaved_for;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].other_name, "fresh loss");
    }
}
