// scripts/commands/build.rhai
//
//   build                     - your builder sheet: score, output, standing
//   build who                 - every builder, best first
//   build next                - the one thing worth doing next
//   build track [area]        - your path, or an area's readiness checklist
//   build credits [area]      - who built what, here or in a named area
//   build claim [area]        - claim unattributed work in an area you own
//   build audit               - grade the room you are standing in
//   build audit room [<vnum>] - grade one room, or the one you are in
//   build audit item  <what>  - grade one item prototype
//   build audit mob   <what>  - grade one mobile prototype
//   build audit quest <vnum>  - grade one quest
//   build audit area [<key>]  - grade an area and everything in it
//   build audit area <key> full - the same, with no cap on the list
//   build audit world         - grade the whole world
//   build audit code <code> [area] - everything raising one finding code
//   build waive <code> <target> <reason>   - mark a finding reviewed
//   build waive all <code> <area> <reason> - the same, area-wide
//   build waive list [area]                - reviewed findings
//   build waive remove <code> <target>     - put a finding back
//
// `<what>` is a vnum, or the keyword of something standing or lying in the
// room. A builder auditing a mob is almost always looking at it, and `mob
// baker` is what they would type at any other command.
//
// The grades come from `src/audit.rs`, which is the only place in the codebase
// that decides whether a piece of content is any good. This script owns the
// words and the layout and nothing else — if you disagree with a grade, the
// argument is with the check, not with the renderer.
//
// A finding is one of three severities:
//
//   BLOCKER  the content is broken as shipped
//   warn     it works, but a player will notice something missing
//   polish   it is fine, it could be better
//
// Polish findings are suggestions. A room that is all polish is a finished
// room, and chasing them to zero is not the point.
//
// A BLOCKER is an F on its own — one blocker deducts more than the D floor. If
// a row is red, the content is broken as shipped, whatever else is on it.
//
// Every check is a lint, and every lint is eventually wrong about something a
// builder meant. `build waive` is the answer to that: it records a finding as
// reviewed, drops it from the grade and every tally, and revives it if the text
// it was a judgement about ever changes.

import "text" as text;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    let line = text::normalise(args);
    let parts = line.split(" ");
    let verb = "";
    if parts.len() > 0 { verb = parts[0].to_lower(); }

    if verb == "" {
        show_sheet(char, connection_id);
        return;
    }
    if verb == "who" || verb == "builders" {
        show_builders(connection_id);
        return;
    }
    if verb == "next" {
        show_next(char, connection_id);
        return;
    }
    if verb == "track" || verb == "tracks" {
        let rest = "";
        if parts.len() > 1 { rest = build_rest(parts, 1); }
        show_tracks(char, rest, connection_id);
        return;
    }
    if verb == "credits" || verb == "credit" {
        let rest = "";
        if parts.len() > 1 { rest = build_rest(parts, 1); }
        show_credits(char, rest, connection_id);
        return;
    }
    if verb == "claim" {
        let rest = "";
        if parts.len() > 1 { rest = build_rest(parts, 1); }
        do_claim(char, rest, connection_id);
        return;
    }
    if verb == "audit" {
        do_audit(char, tail(parts, 1), connection_id);
        return;
    }
    if verb == "waive" || verb == "review" {
        do_waive(char, tail(parts, 1), connection_id);
        return;
    }

    send_client_message(connection_id, usage());
}

fn tail(parts, from) {
    let out = [];
    let i = from;
    while i < parts.len() {
        out.push(parts[i]);
        i += 1;
    }
    return out;
}

// Everything before index `upto`. Used to lift a trailing modifier word off an
// argument list before the rest of it is joined back into prose.
fn tail_upto(parts, upto) {
    let out = [];
    let i = 0;
    while i < upto {
        out.push(parts[i]);
        i += 1;
    }
    return out;
}

fn usage() {
    let out = "\x1b[1;36mbuild\x1b[0m — builder tools\n";
    out += "  build                     your builder sheet\n";
    out += "  build who                 every builder, best first\n";
    out += "  build next                the one thing worth doing next\n";
    out += "  build track [area]        your path, or an area readiness checklist\n";
    out += "  build credits [area]      who built what, here or in an area\n";
    out += "  build claim [area]        put your name on unattributed work you own\n";
    out += "  build audit               grade the room you are in\n";
    out += "  build audit room [<vnum>] grade one room, or the one you are in\n";
    out += "  build audit item <what>   grade one item prototype\n";
    out += "  build audit mob <what>    grade one mobile prototype\n";
    out += "  build audit quest <vnum>  grade one quest\n";
    out += "  build audit area [<key>]  grade an area and its contents\n";
    out += "  build audit world         grade the whole world\n";
    out += "  build audit code <code>   everything raising one finding code\n";
    out += "  build waive ...           mark a finding reviewed — `build waive` for more\n";
    out += "  \x1b[0;37m<what> is a vnum, or the keyword of something in this room.\x1b[0m";
    return out;
}

fn waive_usage() {
    let out = "\x1b[1;36mbuild waive\x1b[0m — record a finding as reviewed\n";
    out += "  build waive <code> <target> <reason>    one finding on one thing\n";
    out += "  build waive all <code> <area> <reason>  that code, everywhere in an area\n";
    out += "  build waive list [area]                 what has been waived\n";
    out += "  build waive remove <code> <target>      put a finding back\n";
    out += "\n";
    out += "  \x1b[0;37m<target> is a vnum, an area prefix, or `world` — the label the audit\n";
    out += "  prints beside the finding. Use `build audit code <code>` to list them.\n";
    out += "  A reason is required. Blockers need an admin.\n";
    out += "  A waiver lapses if the text it was written about changes.\x1b[0m";
    return out;
}

// ===== dispatch =====

fn do_audit(char, rest, connection_id) {
    if rest.len() == 0 {
        audit_current_room(char, connection_id);
        return;
    }

    let what = rest[0].to_lower();
    let key = "";
    if rest.len() > 1 { key = rest[1]; }

    if what == "here" || what == "room." {
        audit_current_room(char, connection_id);
        return;
    }
    if what == "world" {
        show_world(connection_id);
        return;
    }
    if what == "code" {
        if key == "" {
            send_client_message(connection_id, "Which code? `build audit code <code> [area]` — codes are the middle column of a findings list.");
            return;
        }
        let scope = "";
        if rest.len() > 2 { scope = build_rest(rest, 2); }
        show_code(key.to_lower(), scope, connection_id);
        return;
    }
    if what == "area" || what == "zone" {
        // An area key is prose — `build audit area Dungeon Crawl Level 1` has
        // to reach the whole tail. Item and mobile keys deliberately do not:
        // they feed `find_item_in_room` / `find_mobile_by_keyword`, which take
        // one keyword with an optional `N.` prefix.
        let full = false;
        let last = rest.len() - 1;
        if rest.len() > 2 && rest[last].to_lower() == "full" {
            full = true;
            rest = tail_upto(rest, last);
        }
        if rest.len() > 1 { key = build_rest(rest, 1); }
        if key == "" { key = current_area_key(char); }
        if key == "" {
            send_client_message(connection_id, "This room belongs to no area. Try `build audit area <name>`.");
            return;
        }
        show_area(key, full, connection_id);
        return;
    }

    let kind = resolve_kind(what);
    if kind == "" {
        send_client_message(connection_id, usage());
        return;
    }
    // `build audit room` with nothing after it means the room you are in —
    // the same default as bare `build audit`, and the one an operator reaches
    // for when they have just walked somewhere.
    if key == "" && kind == "room" {
        audit_current_room(char, connection_id);
        return;
    }
    if key == "" {
        send_client_message(connection_id, "Which " + kind + "? `build audit " + what + " <vnum|keyword>`");
        return;
    }
    show_entity(kind, key, connection_id, char);
}

// A vnum, or the keyword of something in the room.
//
// The vnum is tried first: it is the explicit form, and letting a keyword
// shadow one would make `build audit item town:sword` mean different things
// depending on what happened to be on the floor. Only when nothing matches
// does the room get a look, and then it is the instance's *prototype* that
// grades — an instance has no quality of its own to report.
fn resolve_in_room(kind, key, char) {
    let room_id = "" + char.current_room_id;
    if kind == "item" {
        let found = find_item_in_room(room_id, key);
        if found == () { return ""; }
        return found.vnum;
    }
    if kind == "mobile" {
        let found = find_mobile_by_keyword(key, get_mobiles_in_room(room_id));
        if found == () { return ""; }
        return found.vnum;
    }
    return "";
}

// Rhai's expression-complexity limit bites on long if-else chains; early
// returns in a helper are the house workaround.
fn build_rest(parts, from) {
    let out = "";
    let i = from;
    while i < parts.len() {
        if i > from { out += " "; }
        out += parts[i];
        i += 1;
    }
    return out;
}

fn resolve_kind(word) {
    if word == "room" { return "room"; }
    if word == "item" || word == "object" || word == "obj" { return "item"; }
    if word == "mob" || word == "mobile" || word == "npc" { return "mobile"; }
    if word == "quest" { return "quest"; }
    return "";
}

// The key every `[area]`-taking subcommand defaults to when you give it none.
//
// The prefix, not the name. A prefix is one token that a builder chose to be
// typed; a name is prose, and an area name that picked up a stray leading space
// somewhere in the API path is a name no lookup will match. `area_matches` on
// the Rust side now trims both ends, but the prefix is still the better key
// because it is the one an operator would have typed by hand.
fn current_area_key(char) {
    let room = get_room_data(char.current_room_id);
    if room == () { return ""; }
    let area = get_area_data(room.area_id);
    if area == () { return ""; }
    if area.prefix != "" { return area.prefix; }
    return area.name;
}

fn audit_current_room(char, connection_id) {
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere the auditor can see.");
        return;
    }
    show_entity("room", "" + char.current_room_id, connection_id, char);
}

// ===== rendering =====

fn letter_colour(letter) {
    if letter == "A" { return "\x1b[1;32m"; }
    if letter == "B" { return "\x1b[0;32m"; }
    if letter == "C" { return "\x1b[1;33m"; }
    if letter == "D" { return "\x1b[0;33m"; }
    return "\x1b[1;31m";
}

fn severity_colour(severity) {
    if severity == "blocker" { return "\x1b[1;31m"; }
    if severity == "warn" { return "\x1b[1;33m"; }
    return "\x1b[0;36m";
}

// `C  62  [######----]`, coloured by the worst thing on it. Every surface that
// shows a grade uses this, so the bar and the letter can never disagree.
//
// The colour follows severity rather than the letter because those are two
// different questions and only one of them is urgent. A header that tallied "3
// blocker" in red over rows tinted by letter read as a contradiction — the
// blockers were all there, on rows the colour scheme was calling merely
// mediocre. Clean content falls back to the letter, which is the only case
// where the letter is the whole story.
fn grade_tag(score, letter, blockers, warns) {
    let colour = letter_colour(letter);
    if blockers > 0 { colour = severity_colour("blocker"); }
    else if warns > 0 { colour = severity_colour("warn"); }
    return colour + letter + "\x1b[0m " + text::pad_left("" + score, 3)
        + "  " + audit_grade_bar(score);
}

fn finding_line(f) {
    return "  " + severity_colour(f.severity) + text::pad_right(f.label, 8) + "\x1b[0m"
        + column(f.code, 32) + f.message + "\n";
}

fn findings_block(findings) {
    if findings.len() == 0 {
        return "  \x1b[0;32mNothing to fix.\x1b[0m\n";
    }
    let out = "";
    for f in findings {
        out += finding_line(f);
    }
    return out;
}

// pad_right does not truncate, so a label wider than its column runs into the
// next one. Every column that holds builder-supplied text goes through this.
fn column(value, width) {
    let s = "" + value;
    if s.len() > width - 1 {
        s = s.sub_string(0, width - 2) + "…";
    }
    return text::pad_right(s, width);
}

// The label column holds a vnum, and a truncated vnum is useless — a builder
// cannot paste `dungeon1:du…` into `redit`, and in an area full of near-identical
// names (thirty hallways) the vnum is the only thing that identifies the row.
// So it sizes itself to the widest vnum on screen rather than to a guess. The
// cap stops one pathological vnum from squeezing every other column out.
fn label_width(rows, floor) {
    let w = floor;
    for r in rows {
        let n = ("" + r.label).len() + 1; // +1 so the widest label still gets a gap
        if n > w { w = n; }
    }
    if w > 40 { w = 40; }
    return w;
}

// What the label column takes, the name column gives up, so the line length
// stays put. The name is only there for recognition — truncating it costs
// nothing a builder needs.
// `label_width` for rows keyed by `target` rather than `label` — a waiver
// target is what a builder pastes into `build waive`, so truncating it makes
// the row useless for the one thing it is there for.
fn target_width(rows, floor) {
    let w = floor;
    for r in rows {
        let n = ("" + r.target).len() + 1;
        if n > w { w = n; }
    }
    if w > 40 { w = 40; }
    return w;
}

fn name_width(budget, label_w) {
    let w = budget - label_w;
    if w < 12 { w = 12; }
    return w;
}

fn plural(n, word) {
    if n == 1 { return "" + n + " " + word; }
    return "" + n + " " + word + "s";
}

fn tally_line(blockers, warns, polish) {
    let parts = [];
    if blockers > 0 { parts.push("\x1b[1;31m" + blockers + " blocker\x1b[0m"); }
    if warns > 0 { parts.push("\x1b[1;33m" + warns + " warn\x1b[0m"); }
    if polish > 0 { parts.push("\x1b[0;36m" + polish + " polish\x1b[0m"); }
    if parts.len() == 0 { return "clean"; }
    let out = "";
    let i = 0;
    while i < parts.len() {
        if i > 0 { out += "  "; }
        out += parts[i];
        i += 1;
    }
    return out;
}

// The header tally, split by where the findings live.
//
// A container's total is its own findings plus its contents'. Printing one
// number and then two lists that each hold part of it is how "3 blocker" came
// to look like it had lost one: the area's own blocker was in the `Area` block
// and the other two were rows in `Worst first`. Naming the halves costs one
// short parenthetical and removes the whole class of question.
fn split_tally(report) {
    let out = tally_line(report.blockers, report.warns, report.polish);
    let own = report.own_blockers + report.own_warns + report.own_polish;
    let content = report.content_blockers + report.content_warns + report.content_polish;
    if own > 0 && content > 0 {
        out += "  \x1b[0;37m(" + own + " on the area, " + content + " on its contents)\x1b[0m";
    }
    if report.reviewed > 0 {
        out += "  \x1b[0;36m" + report.reviewed + " reviewed\x1b[0m";
    }
    return out;
}

// ===== one entity =====

fn show_entity(kind, key, connection_id, char) {
    let report = audit_entity(kind, key);
    if !report.found {
        let vnum = resolve_in_room(kind, key, char);
        if vnum != "" { report = audit_entity(kind, vnum); }
    }
    if !report.found {
        send_client_message(connection_id, "No " + kind + " matches '" + key + "'.");
        return;
    }

    let heading = report.name;
    if heading == "" { heading = "(unnamed)"; }

    let out = "\x1b[1;36m" + heading + "\x1b[0m  \x1b[0;37m(" + kind + " " + report.label + ")\x1b[0m\n";
    out += "  " + grade_tag(report.score, report.letter, report.blockers, report.warns) + "   "
        + tally_line(report.blockers, report.warns, report.polish);
    if report.reviewed > 0 {
        out += "  \x1b[0;36m" + report.reviewed + " reviewed\x1b[0m";
    }
    out += "\n";
    out += credit_line(report) + "\n";
    out += findings_block(report.findings);
    out += reviewed_block(report.waived);
    send_client_message(connection_id, out);
}

// What has been waived on this entity. Printed dimly and after the live
// findings, because it is a record rather than a task — but printed, because a
// suppression nobody can see is a suppression nobody re-examines.
fn reviewed_block(waived) {
    if waived.len() == 0 { return ""; }
    let out = "\n  \x1b[0;36mReviewed\x1b[0m\n";
    for f in waived {
        out += "  \x1b[0;37m" + text::pad_right("ok", 8) + column(f.code, 32) + f.message + "\x1b[0m\n";
    }
    return out;
}

// Who is responsible for this content, and whether it can be credited.
//
// `origin` answers a question the author name cannot: content shipped with the
// engine or pulled in by an importer is real content, but it is nobody's
// achievement, and saying so here is cheaper than explaining it later.
fn credit_line(report) {
    let who = report.authored_by;
    if who == "" { who = "\x1b[0;37munclaimed\x1b[0m"; }
    let out = "  \x1b[0;37mBuilt by\x1b[0m " + who;
    if report.last_edited_by != "" && report.last_edited_by != report.authored_by {
        out += "  \x1b[0;37mlast edited by\x1b[0m " + report.last_edited_by;
    }
    if !report.counts {
        out += "  \x1b[0;37m(" + report.origin_label + " — counts toward no score)\x1b[0m";
    }
    return out + "\n";
}

// ===== one area =====

fn show_area(key, full, connection_id) {
    let report = audit_area_report(key, full);
    if !report.found {
        send_client_message(connection_id, "No area matches '" + key + "'.");
        return;
    }

    let out = "\x1b[1;36m" + report.name + "\x1b[0m\n";
    out += "  " + grade_tag(report.score, report.letter, report.blockers, report.warns) + "   "
        + split_tally(report) + "\n\n";

    out += contents_line(report.counts);
    out += "\n\x1b[1;37mArea\x1b[0m\n";
    out += findings_block(report.findings);
    out += reviewed_block(report.waived);

    out += worst_block(report, key);
    send_client_message(connection_id, out);
}

// "15 rooms (11 C+)   6 mobiles (4 C+)". The C+ figure is what the area
// readiness track reads, so it is worth showing next to the raw count.
fn contents_line(counts) {
    let out = "\x1b[1;37mContents\x1b[0m\n";
    let any = false;
    for kind in counts.keys() {
        let row = counts[kind];
        out += "  " + text::pad_right(kind, 10) + text::pad_left("" + row.total, 4)
            + "   " + row.good + " graded C or better (" + row.ratio + "%)\n";
        any = true;
    }
    if !any { return "\x1b[1;37mContents\x1b[0m\n  Nothing here yet.\n"; }
    return out;
}

// Worst entities first, each with everything wrong with it.
//
// Only the first finding used to print, which is how a header could count 15
// warns while the list showed six: a room with four findings contributed one
// line. The extras are indented under their row and capped, so a single
// pathological entity cannot push the rest off the screen.
fn worst_block(report, key) {
    if report.worst.len() == 0 { return ""; }
    let heading = "\n\x1b[1;37mWorst first\x1b[0m";
    if report.truncated {
        heading += "  \x1b[0;37m(" + report.worst.len() + " of " + report.flagged + " flagged)\x1b[0m";
    }
    let out = heading + "\n";
    let lw = label_width(report.worst, 13);
    let nw = name_width(41, lw);
    for e in report.worst {
        let top = "";
        if e.findings.len() > 0 { top = e.findings[0].message; }
        out += "  " + grade_row_letter(e) + " "
            + text::pad_left("" + e.score, 3) + "  "
            + column(e.kind, 8)
            + column(e.label, lw)
            + column(e.name, nw) + top + "\n";
        out += extra_findings(e.findings);
    }
    if report.truncated {
        out += "  \x1b[0;37mFix these first, then run it again — `build audit area "
            + key + " full` for the rest.\x1b[0m\n";
    }
    return out;
}

// The letter for one worst-first row, tinted by what is actually wrong with it
// rather than by the band the score landed in.
fn grade_row_letter(e) {
    let colour = letter_colour(e.letter);
    if e.blockers > 0 { colour = severity_colour("blocker"); }
    else if e.warns > 0 { colour = severity_colour("warn"); }
    return colour + e.letter + "\x1b[0m";
}

// Findings two and beyond on a worst-first row, indented under it.
fn extra_findings(findings) {
    if findings.len() < 2 { return ""; }
    let out = "";
    let shown = findings.len();
    if shown > 4 { shown = 4; }
    let i = 1;
    while i < shown {
        let f = findings[i];
        out += "        " + severity_colour(f.severity) + text::pad_right(f.label, 8)
            + "\x1b[0m" + f.message + "\n";
        i += 1;
    }
    if findings.len() > shown {
        out += "        \x1b[0;37m+" + (findings.len() - shown) + " more\x1b[0m\n";
    }
    return out;
}

// ===== one code across many entities =====

// Every entity currently raising one finding code.
//
// The audit answers "what is wrong with this thing"; this answers "where else
// does this happen", which is the question a builder has right before deciding
// a check is wrong about their content. It is also the list `build waive all`
// acts on, so seeing it first is seeing exactly what would be silenced.
fn show_code(code, scope, connection_id) {
    let hits = audit_findings_by_code(code, scope);
    if hits.len() == 0 {
        let where_ = "the world";
        if scope != "" { where_ = scope; }
        send_client_message(connection_id, "Nothing in " + where_ + " raises `" + code + "` right now.");
        return;
    }

    let out = "\x1b[1;36m" + code + "\x1b[0m";
    if scope != "" { out += "  \x1b[0;37min " + scope + "\x1b[0m"; }
    out += "  \x1b[0;37m" + hits.len() + " found\x1b[0m\n";
    let lw = target_width(hits, 14);
    for h in hits {
        out += "  " + severity_colour(h.severity) + text::pad_right(h.label, 8) + "\x1b[0m"
            + column(h.kind, 8)
            + column(h.target, lw)
            + h.name + "\n";
    }
    out += "\n  \x1b[0;37m`build waive " + code + " <target> <reason>` for one, or\n";
    out += "  `build waive all " + code + " <area> <reason>` for all of them here.\x1b[0m";
    send_client_message(connection_id, out);
}

// ===== waivers =====

fn do_waive(char, rest, connection_id) {
    if rest.len() == 0 {
        send_client_message(connection_id, waive_usage());
        return;
    }
    let verb = rest[0].to_lower();

    if verb == "list" {
        let scope = "";
        if rest.len() > 1 { scope = build_rest(rest, 1); }
        show_waivers(scope, connection_id);
        return;
    }
    if verb == "remove" || verb == "delete" || verb == "undo" {
        if rest.len() < 3 {
            send_client_message(connection_id, "`build waive remove <code> <target>`");
            return;
        }
        do_waive_remove(char, rest[1].to_lower(), rest[2], connection_id);
        return;
    }
    if verb == "all" {
        if rest.len() < 4 {
            send_client_message(connection_id, "`build waive all <code> <area> <reason>` — a reason is required.");
            return;
        }
        do_waive_all(char, rest[1].to_lower(), rest[2], build_rest(rest, 3), connection_id);
        return;
    }

    if rest.len() < 3 {
        send_client_message(connection_id, "`build waive <code> <target> <reason>` — a reason is required.");
        return;
    }
    do_waive_one(char, verb, rest[1], build_rest(rest, 2), connection_id);
}

fn do_waive_one(char, code, target, reason, connection_id) {
    let res = waive_finding(code, target, reason, char.name);
    if !res.found {
        send_client_message(connection_id, "`" + code + "` is not firing on `" + target
            + "` right now. Nothing to review — check `build audit code " + code + "`.");
        return;
    }
    if !res.allowed {
        send_client_message(connection_id, waive_denial_text(res.reason, code, target));
        return;
    }
    let out = "\x1b[0;32mReviewed.\x1b[0m " + res.code + " on " + res.target + " no longer counts.\n";
    out += "  \x1b[0;37m" + res.message + "\x1b[0m\n";
    out += "  \x1b[0;37mIt comes back if the text it was about changes. "
        + "`build waive remove " + res.code + " " + res.target + "` to undo.\x1b[0m";
    send_client_message(connection_id, out);
}

fn do_waive_all(char, code, area, reason, connection_id) {
    let res = waive_findings_in_area(code, area, reason, char.name);
    if !res.found {
        send_client_message(connection_id, "No area matches '" + area + "'.");
        return;
    }
    if res.waived == 0 {
        if res.reason == "not_firing" {
            send_client_message(connection_id, "Nothing in " + res.area + " raises `" + code + "` right now.");
            return;
        }
        send_client_message(connection_id, waive_denial_text(res.reason, code, res.area));
        return;
    }
    let out = "\x1b[0;32mReviewed.\x1b[0m " + res.waived + " × " + code + " in " + res.area + ".\n";
    if res.denied > 0 {
        out += "  \x1b[1;33m" + res.denied + " skipped\x1b[0m \x1b[0;37m— "
            + waive_denial_text(res.reason, code, res.area) + "\x1b[0m";
    }
    send_client_message(connection_id, out);
}

fn do_waive_remove(char, code, target, connection_id) {
    let res = remove_audit_waiver(code, target, char.name);
    if !res.found {
        send_client_message(connection_id, "No waiver on `" + code + "` for `" + target + "`.");
        return;
    }
    if !res.allowed {
        send_client_message(connection_id, waive_denial_text(res.reason, code, target));
        return;
    }
    send_client_message(connection_id, "\x1b[0;32mRestored.\x1b[0m " + code + " on " + target + " counts again.");
}

// Why a waiver was refused, in words. The reasons are deliberately few: this is
// the surface where a builder learns what they are and are not trusted with.
fn waive_denial_text(reason, code, target) {
    if reason == "blocker_needs_admin" {
        return "`" + code + "` is a BLOCKER — the content is broken as shipped, not merely flagged. "
            + "An admin has to sign that off.";
    }
    if reason == "world_needs_admin" {
        return "World-level findings belong to no builder. An admin has to review those.";
    }
    if reason == "not_permitted" {
        return "You cannot edit the area `" + target + "` belongs to, so you cannot review its findings.";
    }
    return "That could not be reviewed.";
}

fn show_waivers(scope, connection_id) {
    let rows = list_audit_waivers(scope);
    if rows.len() == 0 {
        let where_ = "the world";
        if scope != "" { where_ = scope; }
        send_client_message(connection_id, "Nothing has been reviewed in " + where_ + ".");
        return;
    }

    let out = "\x1b[1;36mReviewed findings\x1b[0m";
    if scope != "" { out += "  \x1b[0;37min " + scope + "\x1b[0m"; }
    out += "\n";
    let stale = 0;
    for w in rows {
        if w.state == "stale" { stale += 1; }
        out += "  " + waiver_state_tag(w.state) + " "
            + column(w.code, 32)
            + column(w.target, 16)
            + column(w.reviewed_by, 12)
            + "\x1b[0;37m" + w.reason + "\x1b[0m\n";
    }
    if stale > 0 {
        out += "\n  \x1b[1;33m" + stale + " stale\x1b[0m \x1b[0;37m— the text changed, so the finding is "
            + "back. Review it again or fix it.\x1b[0m";
    }
    send_client_message(connection_id, out);
}

fn waiver_state_tag(state) {
    if state == "stale" { return "\x1b[1;33m" + text::pad_right("stale", 9) + "\x1b[0m"; }
    if state == "resolved" { return "\x1b[0;32m" + text::pad_right("fixed", 9) + "\x1b[0m"; }
    return "\x1b[0;36m" + text::pad_right("ok", 9) + "\x1b[0m";
}

// ===== the world =====

fn show_world(connection_id) {
    let report = audit_world_report();
    if !report.found {
        send_client_message(connection_id, "The world could not be read.");
        return;
    }

    let f = report.facts;
    let out = "\x1b[1;36mWorld Audit\x1b[0m\n";
    out += "  " + grade_tag(report.score, report.letter, report.blockers, report.warns) + "   "
        + tally_line(report.blockers, report.warns, report.polish)
        + "  \x1b[0;37m(world and area level)\x1b[0m\n";
    // The header grades areas as units, so it counts each area's own findings
    // and not its rooms'. The rows below count everything inside each area, and
    // have therefore always summed to a much larger number. Both are worth
    // knowing; only the unlabelled pair was confusing.
    // Indented to sit under the first tally: 19 characters of grade tag plus
    // the three-space gutter `grade_tag` callers use.
    out += "  " + text::pad_left("", 19) + "   "
        + tally_line(report.deep_blockers, report.deep_warns, report.deep_polish)
        + "  \x1b[0;37m(everything, all the way down)\x1b[0m\n";
    if report.reviewed > 0 {
        out += "  " + text::pad_left("", 19) + "   \x1b[0;36m" + report.reviewed + " reviewed\x1b[0m\n";
    }
    out += "\n";
    out += facts_block(f);

    out += "\n\x1b[1;37mWorld\x1b[0m\n";
    out += findings_block(report.findings);
    out += reviewed_block(report.waived);

    out += "\n\x1b[1;37mAreas\x1b[0m  \x1b[0;37m(counting contents)\x1b[0m\n";
    let lw = label_width(report.areas, 14);
    let nw = name_width(40, lw);
    for a in report.areas {
        out += "  " + grade_row_letter(a) + " "
            + text::pad_left("" + a.score, 3) + "  "
            + column(a.label, lw)
            + column(a.name, nw)
            + text::pad_left("" + a.rooms, 4) + " rooms   "
            + tally_line(a.blockers, a.warns, a.polish) + "\n";
    }
    let unfiled = f.unfiled_rooms + f.unfiled_mobiles + f.unfiled_items;
    if unfiled > 0 {
        out += "\n  \x1b[1;33mUnfiled: " + f.unfiled_rooms + " rooms, " + f.unfiled_mobiles
            + " mobiles, " + f.unfiled_items + " items belong to no area.\x1b[0m\n";
        out += "  \x1b[0;37mNo area audit reaches them, and every area they should be in reports itself empty.\x1b[0m\n";
    }
    out += "\nUse 'build audit area <name>' to go one level down.";
    send_client_message(connection_id, out);
}

fn facts_block(f) {
    let out = "  " + plural(f.areas, "area") + "   " + plural(f.rooms, "room") + "   "
        + plural(f.mobiles, "mobile") + "   " + plural(f.items, "item") + "\n";
    out += "  " + plural(f.quests, "quest") + "   " + plural(f.spawn_points, "spawn point")
        + "   " + plural(f.recipes, "recipe") + "   " + plural(f.transports, "transport") + "\n";
    out += "  " + plural(f.dialogue_trees, "dialogue tree") + "   "
        + plural(f.boards, "board") + "\n";
    return out;
}

// ===== the builder sheet =====

// The score is derived from a scan of what currently exists, never
// accumulated from edits. That is why re-saving a room earns nothing and
// deleting one costs you — there is no event here to repeat. See
// src/build_score.rs.
fn show_sheet(char, connection_id) {
    let score = get_build_score(char.name);

    let out = "\x1b[1;36mBuilder: " + char.name + "\x1b[0m\n";
    if !score.ready {
        out += "  \x1b[0;37mThe first scan has not run yet. Try again in a few minutes.\x1b[0m\n\n";
        out += usage();
        send_client_message(connection_id, out);
        return;
    }

    out += "  \x1b[1;33m" + score.total + "\x1b[0m points";
    if score.bounty_points > 0 {
        out += "  \x1b[0;37m(" + score.content_points + " built, "
            + score.bounty_points + " from bounties)\x1b[0m";
    }
    if score.placing > 0 {
        out += "   " + ordinal(score.placing) + " of " + plural(score.builders, "builder");
    }
    out += "\n";

    if score.entities == 0 {
        out += "\n  \x1b[0;37mNothing here is yours yet. `redit create`, `dig`, `oedit create`\x1b[0m\n";
        out += "  \x1b[0;37mand `medit create` all claim what they make.\x1b[0m\n";
        out += "\n" + usage();
        send_client_message(connection_id, out);
        return;
    }

    out += "\n\x1b[1;37mYour work\x1b[0m\n";
    for row in score.tallies {
        out += "  " + text::pad_right(row.kind, 9)
            + text::pad_left("" + row.count, 4) + "  "
            + text::pad_left("" + row.points, 5) + " pts   "
            + row.good + " solid";
        if row.excellent > 0 { out += ", \x1b[1;32m" + row.excellent + " excellent\x1b[0m"; }
        if row.broken > 0 { out += ", \x1b[1;31m" + row.broken + " broken\x1b[0m"; }
        out += "\n";
    }

    if score.broken > 0 {
        out += "\n  \x1b[1;31m" + plural(score.broken, "piece") + " of your work has a blocker.\x1b[0m";
        out += " \x1b[0;37m`build audit area` finds them.\x1b[0m\n";
    }

    out += "\n" + usage();
    send_client_message(connection_id, out);
}

fn show_builders(connection_id) {
    let index = get_build_score_index();
    if index.len() == 0 {
        send_client_message(connection_id, "Nobody has built anything yet.");
        return;
    }
    let out = "\x1b[1;36mBuilders\x1b[0m\n";
    for row in index {
        out += "  " + text::pad_left("" + row.rank, 3) + "  "
            + column(row.name, 20)
            + text::pad_left("" + row.total, 6) + " pts   "
            + text::pad_left("" + row.entities, 4) + " pieces";
        if row.excellent > 0 { out += "   \x1b[1;32m" + row.excellent + " A-grade\x1b[0m"; }
        out += "\n";
    }
    send_client_message(connection_id, out);
}

fn ordinal(n) {
    let suffix = "th";
    let last_two = n % 100;
    if last_two < 11 || last_two > 13 {
        let last = n % 10;
        if last == 1 { suffix = "st"; }
        if last == 2 { suffix = "nd"; }
        if last == 3 { suffix = "rd"; }
    }
    return "" + n + suffix;
}

// ===== tracks =====

// A track is a checklist rendered the way a quest log is, because a builder
// who plays this game already knows how to read one. Steps carry hints: the
// Builder's Path is a tutorial for systems most worlds never touch, and a
// checklist that names a system without saying how to reach it is a quiz.
fn track_block(track, show_all) {
    let out = "\x1b[1;37m" + track.name + "\x1b[0m  \x1b[0;37m"
        + track.done + " / " + track.total + "\x1b[0m";
    if track.complete { out += "  \x1b[1;32mcomplete\x1b[0m"; }
    out += "\n";
    if track.description != "" {
        out += "  \x1b[0;37m" + track.description + "\x1b[0m\n";
    }
    for step in track.steps {
        if step.done {
            out += "  \x1b[1;32m[x]\x1b[0m " + step.label + "\n";
        } else {
            if !show_all { continue; }
            out += "  \x1b[90m[ ]\x1b[0m " + step.label + "\n";
            if step.hint != "" {
                out += "      \x1b[0;37m" + step.hint + "\x1b[0m\n";
            }
        }
    }
    return out;
}

// ===== credits =====
//
// The one surface that shows a builder's name beside their work. Scoring
// answers "how am I doing"; this answers "whose is this", which is a different
// question and the only one a second builder on the server actually asks.

fn show_credits(char, area_key, connection_id) {
    // No argument means the area you are standing in, so walking somewhere and
    // asking who made it is one word.
    let key = area_key;
    if key == "world" || key == "all" {
        // The whole world, which the binding takes as an empty scope.
        key = "";
    } else if key == "" {
        let room = get_room_data(char.current_room_id);
        if room != () {
            let area = get_area_data(room.area_id);
            if area != () { key = area.prefix; }
        }
    }

    let credits = get_area_credits(key);
    if !credits.found {
        send_client_message(connection_id, "No area by that name. Try `build credits` where you stand, or `build credits world`.");
        return;
    }

    let out = "\x1b[1;36mCredits — " + credits.name + "\x1b[0m\n";
    if credits.authors.len() == 0 {
        out += "\x1b[90mNothing here names a builder. Content shipped with the engine or\n";
        out += "brought in by an importer credits nobody, by design.\x1b[0m";
        send_client_message(connection_id, out);
        return;
    }

    out += "\x1b[90m" + text::pad_right("builder", 18) + text::pad_left("rooms", 7)
        + text::pad_left("items", 7) + text::pad_left("mobs", 7)
        + text::pad_left("quests", 8) + text::pad_left("all", 6) + "\x1b[0m\n";
    for a in credits.authors {
        let colour = "";
        if a.name == char.name { colour = "\x1b[1;33m"; }
        out += colour + text::pad_right(a.name, 18) + "\x1b[0m"
            + text::pad_left("" + a.rooms, 7)
            + text::pad_left("" + a.items, 7)
            + text::pad_left("" + a.mobiles, 7)
            + text::pad_left("" + a.quests, 8)
            + text::pad_left("" + a.total, 6) + "\n";
    }
    if credits.uncredited > 0 {
        out += "\x1b[90m" + credits.uncredited + " unattributed — seed, import, or built before\n";
        out += "attribution existed. It counts toward nobody's score.\x1b[0m";
    }
    send_client_message(connection_id, out);
}

// `build claim [area]` — the bridge between owning an area and being credited
// for it.
//
// Two fields have always been disjoint on purpose: `owner` is an ACL, and
// `authored_by` is a credit that a create stamps once and nothing afterwards
// moves. That leaves an area built before attribution existed permanently
// uncredited, because there is no edit you can make that would claim it. This
// is the one command that can, and it is deliberately explicit rather than a
// read-time guess: credit derived from an ACL would move every time the keys
// changed hands.
//
// What it will not do: reassign anything that already names an author, and
// take seed or imported content. Those are counted back to you instead, so the
// numbers add up and nobody has to wonder what happened to the rest.
fn do_claim(char, area_key, connection_id) {
    if !char.build_mode {
        send_client_message(connection_id, "You must be in build mode to claim content.");
        return;
    }

    let key = area_key;
    if key == "" { key = current_area_key(char); }
    if key == "" {
        send_client_message(connection_id, "This room belongs to no area. Try `build claim <area>`.");
        return;
    }

    let result = claim_area_content(key, char.name);
    if !result.found {
        send_client_message(connection_id, "No area matches '" + key + "'.");
        return;
    }
    if !result.allowed {
        let out = "";
        if result.reason == "unowned" {
            out = "\x1b[1;31mThat area has no owner.\x1b[0m\n";
            out += "\x1b[90mClaiming is gated on ownership — with nobody on record as\n";
            out += "responsible for an area, there is nothing to base a claim on.\n";
            out += "Set an owner first: `aedit owner <name>`.\x1b[0m";
        } else {
            out = "\x1b[1;31mThat area belongs to " + result.owner + ".\x1b[0m\n";
            out += "\x1b[90mOnly the owner of record, or an admin, can claim its work.\x1b[0m";
        }
        send_client_message(connection_id, out);
        return;
    }

    let out = "\x1b[1;36mClaim — " + result.name + "\x1b[0m\n";
    if result.total == 0 {
        out += "\x1b[90mNothing here to claim. Everything is either already credited\n";
        out += "or came from the seed world or an importer.\x1b[0m\n";
    } else {
        out += "You are now credited for \x1b[1;33m" + result.total + "\x1b[0m pieces of content:\n";
        if result.areas > 0 { out += "  the area itself\n"; }
        if result.rooms > 0 { out += "  " + result.rooms + " rooms\n"; }
        if result.items > 0 { out += "  " + result.items + " items\n"; }
        if result.mobiles > 0 { out += "  " + result.mobiles + " mobiles\n"; }
        if result.quests > 0 { out += "  " + result.quests + " quests\n"; }
    }

    let skipped = result.already_credited + result.skipped_seed + result.skipped_import;
    if skipped > 0 {
        out += "\x1b[90m";
        if result.already_credited > 0 {
            out += result.already_credited + " already names a builder — a claim never takes that.\n";
        }
        let machine = result.skipped_seed + result.skipped_import;
        if machine > 0 {
            out += machine + " shipped with the engine or came from an importer, so it\n";
            out += "credits nobody and counts toward nobody's score.\n";
        }
        out += "\x1b[0m";
    }
    send_client_message(connection_id, out);
}

fn show_tracks(char, area_key, connection_id) {
    let out = "";
    if area_key == "" {
        let mine = get_build_tracks("builder", char.name);
        if mine.len() == 0 {
            send_client_message(connection_id, "No builder tracks are defined.");
            return;
        }
        for t in mine {
            out += track_block(t, true) + "\n";
        }
        out += "\x1b[0;37mUse 'build track <area>' for an area readiness checklist.\x1b[0m";
        send_client_message(connection_id, out);
        return;
    }

    let tracks = get_build_tracks("area", area_key);
    if tracks.len() == 0 {
        send_client_message(connection_id, "No area matches '" + area_key + "'.");
        return;
    }
    for t in tracks {
        out += track_block(t, true) + "\n";
    }
    send_client_message(connection_id, out);
}

// ===== what to do next =====

// One thing, not a wall. A list of everything outstanding is the blank page
// again, which is the problem this is here to solve.
fn show_next(char, connection_id) {
    let out = "\x1b[1;36mNext\x1b[0m\n";
    let said = false;

    // 1. Anything of yours that is outright broken. A blocker beats a
    //    checklist step every time — it is already in the world, and a player
    //    can already walk into it.
    let score = get_build_score(char.name);
    if score.ready && score.broken > 0 {
        out += "  \x1b[1;31m" + plural(score.broken, "piece") + " of your work has a blocker.\x1b[0m\n";
        out += "  \x1b[0;37m`build audit area` in the area you are standing in names them.\x1b[0m\n";
        said = true;
    }

    // 2. The area you are standing in, if it is unfinished.
    let area_key = current_area_key(char);
    if area_key != "" {
        for t in get_build_tracks("area", area_key) {
            if !t.complete && t.next_label != "" {
                out += "\n  \x1b[1;37m" + area_key + "\x1b[0m — " + t.name
                    + " \x1b[0;37m(" + t.done + "/" + t.total + ")\x1b[0m\n";
                out += "  " + t.next_label + "\n";
                if t.next_hint != "" {
                    out += "      \x1b[0;37m" + t.next_hint + "\x1b[0m\n";
                }
                said = true;
            }
        }
    }

    // 3. A system you have never used. This is the part that teaches the
    //    engine rather than just grading it.
    for t in get_build_tracks("builder", char.name) {
        if !t.complete && t.next_label != "" {
            out += "\n  \x1b[1;37m" + t.name + "\x1b[0m \x1b[0;37m("
                + t.done + "/" + t.total + ")\x1b[0m\n";
            out += "  " + t.next_label + "\n";
            if t.next_hint != "" {
                out += "      \x1b[0;37m" + t.next_hint + "\x1b[0m\n";
            }
            said = true;
        }
    }

    // 4. Whatever is holding the world back.
    let w = get_world_rating();
    if w.ready && w.capped {
        out += "\n  \x1b[1;33mThe world: " + w.next_step + "\x1b[0m\n";
        said = true;
    }

    if !said {
        out += "  \x1b[0;32mNothing outstanding. Start an area, or find something to polish.\x1b[0m\n";
    }
    send_client_message(connection_id, out);
}
