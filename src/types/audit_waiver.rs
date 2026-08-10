//! Reviewed audit findings — a builder's "I looked at this, it is fine".
//!
//! The auditor is a lint, and every lint eventually flags something the author
//! meant. Before this existed a builder's only options were to change correct
//! content or live with a permanently depressed grade, and both of those teach
//! the same lesson: ignore the auditor. A waiver is the third option.
//!
//! Two properties make it safe to hand builders:
//!
//! * **It is keyed on `(code, target)`** — the same pair the bounty board
//!   dedupes on ([`crate::types::BuildRequest::auditor_key`]) — so a waiver
//!   silences exactly one finding on exactly one entity, never a code globally.
//! * **It carries a fingerprint of the message it was granted against.** Edit
//!   the short description a keyword waiver was written for and the message
//!   changes, the fingerprint stops matching, and the finding comes back. A
//!   waiver cannot outlive the content it was a judgement about.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// One reviewed finding, approved as a false positive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditWaiver {
    /// The audit code this silences, e.g. `item.keywords_miss_nouns`.
    pub code: String,
    /// Entity the finding was about — a vnum, an area prefix, or `world`.
    /// The same string `AuditEntry.label` carries.
    pub target: String,
    /// Owning area prefix, for scoped listing. Empty for world findings.
    #[serde(default)]
    pub area_prefix: String,
    /// Why this is not a defect. Required — a waiver with no reason is
    /// indistinguishable from a builder silencing an inconvenience.
    pub reason: String,
    /// Hash of the finding message as it read when the waiver was granted.
    /// See [`fingerprint`].
    pub fingerprint: u64,
    /// Severity at the time of waiving, for the audit trail. Blockers need an
    /// admin, and the record of that decision should survive.
    #[serde(default)]
    pub severity: String,
    pub reviewed_by: String,
    pub created_at: i64,
}

impl AuditWaiver {
    /// The storage and lookup key. Matches
    /// [`crate::types::BuildRequest::auditor_key`] deliberately: the bounty
    /// board and the waiver list are two views of the same identity.
    pub fn key(code: &str, target: &str) -> String {
        format!("{code}@{target}")
    }

    pub fn storage_key(&self) -> String {
        AuditWaiver::key(&self.code, &self.target)
    }

    /// Does this waiver still apply to the finding in front of us?
    ///
    /// A mismatched fingerprint means the content moved underneath the
    /// judgement, so the finding is live again and the waiver is stale.
    pub fn covers(&self, message: &str) -> bool {
        self.fingerprint == fingerprint(&self.code, message)
    }
}

/// Identity of a finding's *content*, not just its code.
///
/// The message is the right thing to hash because every message that can change
/// meaningfully already embeds what changed: `keywords_miss_nouns` lists the
/// words, `orphan_rooms` carries the count, `thin_desc` carries the length. No
/// extra plumbing from each check site, and no way for a waiver to keep hiding
/// a finding that now says something different.
pub fn fingerprint(code: &str, message: &str) -> u64 {
    let mut h = DefaultHasher::new();
    code.hash(&mut h);
    message.hash(&mut h);
    h.finish()
}
