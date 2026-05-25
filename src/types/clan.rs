// Player-organization "clan" entity. Distinct from short-lived combat
// groups (`CharacterData.is_grouped` / `following`). Authored via the
// in-game `clan` command (admins create; ranked members manage roster).

use serde::{Deserialize, Serialize};

/// Default color when a clan hasn't picked one — bright yellow ANSI.
pub const DEFAULT_CLAN_COLOR: &str = "\x1b[1;33m";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClanRank {
    Leader,
    Officer,
    Member,
}

impl ClanRank {
    pub fn as_str(self) -> &'static str {
        match self {
            ClanRank::Leader => "leader",
            ClanRank::Officer => "officer",
            ClanRank::Member => "member",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "leader" => Some(ClanRank::Leader),
            "officer" => Some(ClanRank::Officer),
            "member" => Some(ClanRank::Member),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanMember {
    pub name: String,
    pub rank: ClanRank,
    #[serde(default)]
    pub joined_day: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClanData {
    /// Primary key — uppercased on save. 2..=6 chars, [A-Z0-9].
    pub tag: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub motd: String,
    /// ANSI prefix to color the [TAG] in `who`. Empty = use DEFAULT_CLAN_COLOR.
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub founded_day: i32,
    #[serde(default)]
    pub founder: String,
    #[serde(default)]
    pub members: Vec<ClanMember>,
}

impl ClanData {
    pub fn new(tag: &str, name: &str, founded_day: i32) -> Self {
        Self {
            tag: tag.to_ascii_uppercase(),
            name: name.to_string(),
            description: String::new(),
            motd: String::new(),
            color: String::new(),
            founded_day,
            founder: String::new(),
            members: Vec::new(),
        }
    }

    pub fn display_color(&self) -> &str {
        if self.color.is_empty() {
            DEFAULT_CLAN_COLOR
        } else {
            &self.color
        }
    }

    /// Case-insensitive member lookup.
    pub fn member(&self, name: &str) -> Option<&ClanMember> {
        self.members
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }

    pub fn member_mut(&mut self, name: &str) -> Option<&mut ClanMember> {
        self.members
            .iter_mut()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }

    pub fn rank_of(&self, name: &str) -> Option<ClanRank> {
        self.member(name).map(|m| m.rank)
    }

    pub fn leader_count(&self) -> usize {
        self.members
            .iter()
            .filter(|m| m.rank == ClanRank::Leader)
            .count()
    }

    /// Validate a tag: 2..=6 chars, each [A-Z0-9] after uppercasing.
    pub fn valid_tag(tag: &str) -> bool {
        let n = tag.chars().count();
        if n < 2 || n > 6 {
            return false;
        }
        tag.chars().all(|c| c.is_ascii_alphanumeric())
    }
}
