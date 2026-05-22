//! Token substitution for [`SocialAction`] message templates.
//!
//! Templates use CircleMUD/tbaMUD pronoun tokens. Lowercase resolves to
//! the actor, uppercase to the victim/secondary party:
//!
//! | Token        | Resolves to                              |
//! |--------------|------------------------------------------|
//! | `$n` / `$N`  | name (or `someone` when hidden)          |
//! | `$e` / `$E`  | subjective pronoun (he/she/they/it)      |
//! | `$m` / `$M`  | objective pronoun (him/her/them/it)      |
//! | `$s` / `$S`  | possessive pronoun (his/her/their/its)   |
//! | `$p` / `$P`  | object short-desc                        |
//! | `$t` / `$T`  | body-part / free-text argument           |
//! | `$$`         | literal `$`                              |
//!
//! Tokens whose data isn't supplied render as an empty string — Circle's
//! original behaviour for missing fields.
//!
//! Pronoun resolution mirrors the DG vars table in `src/script/dg/vars.rs`
//! (kept duplicated for now to avoid a cross-module surgery; both call
//! into the same four-row table and stay in lockstep by convention).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenderKind {
    Male,
    Female,
    Nonbinary,
    Neuter,
}

/// Parse a gender string into [`GenderKind`]. Unrecognised input resolves
/// to `Neuter`, matching tbamud/DG semantics. Accepts the same aliases as
/// `src/script/dg/vars.rs::parse_gender`.
pub fn parse_gender(g: &str) -> GenderKind {
    match g.trim().to_ascii_lowercase().as_str() {
        "male" | "m" | "man" => GenderKind::Male,
        "female" | "f" | "woman" => GenderKind::Female,
        "nonbinary" | "non-binary" | "nb" | "enby" | "they" | "them" => GenderKind::Nonbinary,
        "neuter" | "it" | "object" | "thing" | "robot" | "automaton" | "construct" => {
            GenderKind::Neuter
        }
        _ => GenderKind::Neuter,
    }
}

pub fn subjective(g: GenderKind) -> &'static str {
    match g {
        GenderKind::Male => "he",
        GenderKind::Female => "she",
        GenderKind::Nonbinary => "they",
        GenderKind::Neuter => "it",
    }
}

pub fn objective(g: GenderKind) -> &'static str {
    match g {
        GenderKind::Male => "him",
        GenderKind::Female => "her",
        GenderKind::Nonbinary => "them",
        GenderKind::Neuter => "it",
    }
}

pub fn possessive(g: GenderKind) -> &'static str {
    match g {
        GenderKind::Male => "his",
        GenderKind::Female => "her",
        GenderKind::Nonbinary => "their",
        GenderKind::Neuter => "its",
    }
}

/// Reflexive pronoun for `$mself` / `$Mself` style tokens (Circle uses
/// `$mself` literal — we resolve it here as part of `$m`+`self`).
pub fn reflexive(g: GenderKind) -> &'static str {
    match g {
        GenderKind::Male => "himself",
        GenderKind::Female => "herself",
        GenderKind::Nonbinary => "themself",
        GenderKind::Neuter => "itself",
    }
}

/// Render-time party (actor or victim). `visible_name` is rendered for
/// `$n`/`$N`; hide-mode dispatch swaps in `"someone"` for observers who
/// can't see the named party. `gender` is already parsed.
#[derive(Debug, Clone, Copy)]
pub struct RenderParty<'a> {
    pub visible_name: &'a str,
    pub gender: GenderKind,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderObject<'a> {
    pub short_desc: &'a str,
}

/// Substitute pronoun/name/object tokens into `template`. Unknown `$X`
/// tokens are passed through verbatim so an unrecognised token is loud
/// rather than silently dropped.
pub fn render(
    template: &str,
    actor: &RenderParty<'_>,
    victim: Option<&RenderParty<'_>>,
    object: Option<&RenderObject<'_>>,
    body_part: Option<&str>,
) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let Some(&next) = chars.peek() else {
            out.push('$');
            break;
        };
        match next {
            '$' => {
                chars.next();
                out.push('$');
            }
            'n' => {
                chars.next();
                out.push_str(actor.visible_name);
            }
            'N' => {
                chars.next();
                if let Some(v) = victim {
                    out.push_str(v.visible_name);
                }
            }
            'e' => {
                chars.next();
                out.push_str(subjective(actor.gender));
            }
            'E' => {
                chars.next();
                if let Some(v) = victim {
                    out.push_str(subjective(v.gender));
                }
            }
            's' => {
                chars.next();
                out.push_str(possessive(actor.gender));
            }
            'S' => {
                chars.next();
                if let Some(v) = victim {
                    out.push_str(possessive(v.gender));
                }
            }
            // $m / $M: objective pronoun. Special case $mself / $Mself
            // → reflexive — Circle relies on this literal compound.
            'm' => {
                chars.next();
                if peek_word(&mut chars, "self") {
                    out.push_str(reflexive(actor.gender));
                } else {
                    out.push_str(objective(actor.gender));
                }
            }
            'M' => {
                chars.next();
                if peek_word(&mut chars, "self") {
                    if let Some(v) = victim {
                        out.push_str(reflexive(v.gender));
                    }
                } else if let Some(v) = victim {
                    out.push_str(objective(v.gender));
                }
            }
            'p' | 'P' => {
                chars.next();
                if let Some(o) = object {
                    out.push_str(o.short_desc);
                }
            }
            't' | 'T' => {
                chars.next();
                if let Some(b) = body_part {
                    out.push_str(b);
                }
            }
            _ => {
                // Unknown token — pass through verbatim. Loud is better
                // than silently swallowing an authoring typo.
                out.push('$');
            }
        }
    }
    out
}

/// Capitalize the first ASCII letter of `s` in place. Sentences emitted
/// to the room typically begin with `$n` (the actor's name) which is
/// already capitalized, but variants like `$e looks` need a leading cap.
pub fn capitalize_first(s: &mut String) {
    if let Some(c) = s.chars().next() {
        if c.is_ascii_lowercase() {
            let upper = c.to_ascii_uppercase();
            s.replace_range(..c.len_utf8(), &upper.to_string());
        }
    }
}

/// Peek ahead for an exact lowercase word continuation. If matched,
/// consumes those chars from the iterator and returns true. Used to
/// detect `$mself` / `$Mself` compound tokens without backtracking.
fn peek_word(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, want: &str) -> bool {
    let mut clone = chars.clone();
    for w in want.chars() {
        match clone.next() {
            Some(c) if c == w => {}
            _ => return false,
        }
    }
    for _ in 0..want.len() {
        chars.next();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn male() -> RenderParty<'static> {
        RenderParty {
            visible_name: "Alice",
            gender: GenderKind::Male,
        }
    }

    fn female() -> RenderParty<'static> {
        RenderParty {
            visible_name: "Bob",
            gender: GenderKind::Female,
        }
    }

    #[test]
    fn names() {
        let a = male();
        let b = female();
        let r = render("$n waves at $N.", &a, Some(&b), None, None);
        assert_eq!(r, "Alice waves at Bob.");
    }

    #[test]
    fn pronouns_match_gender() {
        let a = male(); // he/him/his
        let b = female(); // she/her/her
        let r = render("$e gives $S hand to $M.", &a, Some(&b), None, None);
        assert_eq!(r, "he gives her hand to her.");
    }

    #[test]
    fn nonbinary_uses_they() {
        let a = RenderParty {
            visible_name: "Sam",
            gender: GenderKind::Nonbinary,
        };
        let r = render("$n waves $s hand.", &a, None, None, None);
        assert_eq!(r, "Sam waves their hand.");
    }

    #[test]
    fn neuter_defaults_for_unknown() {
        assert_eq!(parse_gender(""), GenderKind::Neuter);
        assert_eq!(parse_gender("dragon"), GenderKind::Neuter);
        let a = RenderParty {
            visible_name: "the dragon",
            gender: parse_gender(""),
        };
        let r = render("$n curls $s tail around $m.", &a, None, None, None);
        assert_eq!(r, "the dragon curls its tail around it.");
    }

    #[test]
    fn reflexive_self_token() {
        let a = male();
        let r = render("$n smiles at $mself.", &a, None, None, None);
        assert_eq!(r, "Alice smiles at himself.");
    }

    #[test]
    fn dollar_escape() {
        let a = male();
        let r = render("price: $$5", &a, None, None, None);
        assert_eq!(r, "price: $5");
    }

    #[test]
    fn object_token() {
        let a = male();
        let obj = RenderObject {
            short_desc: "a glowing sword",
        };
        let r = render("$n admires $p.", &a, None, Some(&obj), None);
        assert_eq!(r, "Alice admires a glowing sword.");
    }

    #[test]
    fn body_part_token() {
        let a = male();
        let b = female();
        let r = render("$n pats $N on the $t.", &a, Some(&b), None, Some("head"));
        assert_eq!(r, "Alice pats Bob on the head.");
    }

    #[test]
    fn missing_victim_drops_quietly() {
        let a = male();
        let r = render("$n waves at $N.", &a, None, None, None);
        assert_eq!(r, "Alice waves at .");
    }

    #[test]
    fn unknown_token_passes_through() {
        let a = male();
        let r = render("$n $z $e", &a, None, None, None);
        assert_eq!(r, "Alice $z he");
    }
}
