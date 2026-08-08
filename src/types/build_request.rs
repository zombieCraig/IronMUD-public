//! The help-wanted board: work one builder wants doing that someone else can
//! pick up.
//!
//! Modelled directly on [`crate::types::BugReport`], which is the closest
//! existing thing in the tree and already solved the same problems — ticket
//! numbers people can say out loud, a status enum, admin notes, and a REST
//! surface. Copying its shape means the two boards behave the same way, which
//! matters more than any individual field.
//!
//! # Why points and not gold
//!
//! Gold is meaningless to a builder. Paying bounties in builder points makes
//! the board a **routing mechanism for effort** rather than a parallel
//! currency: it feeds the same score, the same leaderboards and the same
//! achievements as everything else, and the only thing it adds is the ability
//! to say "this, please, next".
//!
//! # Why requests are generated as well as written
//!
//! A board only builders post to is a board that is empty on the day it is
//! most needed. The auditor already knows every blocker and warning in the
//! world; turning those into requests means the board has work on it before
//! anybody has typed anything, which is the whole difference between "scored"
//! and "guided".
//!
//! Auditor-origin requests **close themselves** when the finding stops firing.
//! Nobody has to accept them, and fixing something without ever reading the
//! board still pays. Without that they rot into a stale to-do list, which is
//! how every bug tracker dies.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AdminNote, ContentKind};

/// Who asked for this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequestOrigin {
    /// A builder posted it.
    #[default]
    Builder,
    /// The auditor generated it from a finding.
    Auditor,
}

impl RequestOrigin {
    pub fn key(self) -> &'static str {
        match self {
            RequestOrigin::Builder => "builder",
            RequestOrigin::Auditor => "auditor",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    /// Anybody may claim it.
    #[default]
    Open,
    /// Somebody is working on it. Claims expire — see
    /// `crate::bounty::CLAIM_EXPIRY_SECS`.
    Claimed,
    /// The claimant says it is done, and the requester has not looked yet.
    Submitted,
    /// Done and paid.
    Accepted,
}

// There is deliberately no `Rejected` state. Rejecting sends work back to
// `Open` with a note attached, because the work is still wanted and the
// claimant is often the one to finish it — a terminal "rejected" row would be
// a dead record of something nobody decided to stop wanting. An earlier draft
// carried the variant, nothing ever assigned it, and it showed up as a
// permanently empty filter in three surfaces.

impl RequestStatus {
    pub fn key(self) -> &'static str {
        match self {
            RequestStatus::Open => "open",
            RequestStatus::Claimed => "claimed",
            RequestStatus::Submitted => "submitted",
            RequestStatus::Accepted => "accepted",
        }
    }

    pub fn from_key(s: &str) -> Option<RequestStatus> {
        match s.trim().to_lowercase().as_str() {
            "open" => Some(RequestStatus::Open),
            "claimed" => Some(RequestStatus::Claimed),
            "submitted" => Some(RequestStatus::Submitted),
            "accepted" | "done" => Some(RequestStatus::Accepted),
            _ => None,
        }
    }

    /// Still wanted. Closed requests are kept as a record, not as work.
    pub fn is_live(self) -> bool {
        matches!(
            self,
            RequestStatus::Open | RequestStatus::Claimed | RequestStatus::Submitted
        )
    }
}

/// One piece of wanted work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRequest {
    pub id: Uuid,
    /// Short number people can say out loud. Same idea as a bug ticket.
    pub ticket_number: i64,
    /// Character name, or `SYSTEM` for auditor-generated requests.
    pub requester: String,
    #[serde(default)]
    pub origin: RequestOrigin,
    /// What kind of content is wanted. `None` when the request does not care.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ContentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_id: Option<Uuid>,
    /// Area prefix, kept alongside the id so a listing does not have to load
    /// every area to draw one line.
    #[serde(default)]
    pub area_label: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    /// Builder points paid on acceptance.
    pub points: i32,
    #[serde(default)]
    pub status: RequestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_by: Option<String>,
    #[serde(default)]
    pub claimed_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fulfilled_by: Option<String>,
    /// Vnums or ids the claimant produced. Free text; the point is that a
    /// requester reviewing the work can find it.
    #[serde(default)]
    pub linked: Vec<String>,
    /// Set on auditor-origin requests. The pair `(finding_code, target)` is
    /// what dedupes them and what lets them close themselves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding_code: Option<String>,
    /// Entity the finding was about — a vnum, an area prefix, or `world`.
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub notes: Vec<AdminNote>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub closed_at: i64,
}

impl BuildRequest {
    /// The dedupe key for auditor-generated requests. Two runs of the auditor
    /// over the same unfixed finding must produce one request, not two.
    pub fn auditor_key(finding_code: &str, target: &str) -> String {
        format!("{finding_code}@{target}")
    }

    pub fn dedupe_key(&self) -> Option<String> {
        self.finding_code
            .as_ref()
            .map(|c| BuildRequest::auditor_key(c, &self.target))
    }

    /// May `name` accept or reject this? The requester, because it is their
    /// work; an admin, because somebody has to be able to unstick it; and
    /// nobody else. `SYSTEM` requests are not accepted by hand at all — they
    /// close themselves when the finding clears.
    pub fn can_judge(&self, name: &str, is_admin: bool) -> bool {
        is_admin || self.requester.eq_ignore_ascii_case(name)
    }

    /// Deliberately allows the claimant to be the requester. A builder posting
    /// work and then doing it themselves has still done the work, and blocking
    /// it would only mean they do it without telling anyone.
    pub fn can_claim(&self) -> bool {
        self.status == RequestStatus::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_keys_round_trip() {
        for s in [
            RequestStatus::Open,
            RequestStatus::Claimed,
            RequestStatus::Submitted,
            RequestStatus::Accepted,
        ] {
            assert_eq!(RequestStatus::from_key(s.key()), Some(s));
        }
        assert_eq!(RequestStatus::from_key("nonsense"), None);
        assert_eq!(
            RequestStatus::from_key("rejected"),
            None,
            "rejecting returns work to Open; a terminal rejected state would be a filter nothing can match"
        );
    }

    #[test]
    fn only_open_work_is_claimable_and_only_live_work_is_wanted() {
        assert!(RequestStatus::Open.is_live());
        assert!(RequestStatus::Claimed.is_live());
        assert!(RequestStatus::Submitted.is_live());
        assert!(!RequestStatus::Accepted.is_live());
    }

    #[test]
    fn the_requester_and_admins_judge_and_nobody_else() {
        let r = BuildRequest {
            id: Uuid::new_v4(),
            ticket_number: 1,
            requester: "Ana".into(),
            origin: RequestOrigin::Builder,
            kind: None,
            area_id: None,
            area_label: String::new(),
            title: "t".into(),
            detail: String::new(),
            points: 10,
            status: RequestStatus::Submitted,
            claimed_by: Some("Bo".into()),
            claimed_at: 0,
            fulfilled_by: None,
            linked: Vec::new(),
            finding_code: None,
            target: String::new(),
            notes: Vec::new(),
            created_at: 0,
            updated_at: 0,
            closed_at: 0,
        };
        assert!(r.can_judge("Ana", false));
        assert!(r.can_judge("ana", false), "name matching must be case-insensitive");
        assert!(r.can_judge("Zeus", true));
        assert!(!r.can_judge("Bo", false), "the claimant judged their own work");
    }

    #[test]
    fn auditor_requests_dedupe_on_the_finding_and_its_target() {
        assert_eq!(
            BuildRequest::auditor_key("room.no_desc", "oak:square"),
            BuildRequest::auditor_key("room.no_desc", "oak:square")
        );
        assert_ne!(
            BuildRequest::auditor_key("room.no_desc", "oak:square"),
            BuildRequest::auditor_key("room.no_desc", "oak:lane")
        );
    }
}
