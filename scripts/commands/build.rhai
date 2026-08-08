// scripts/commands/build.rhai
//
//   build                     - your builder sheet: score, output, standing
//   build who                 - every builder, best first
//   build next                - the one thing worth doing next
//   build track [area]        - your path, or an area's readiness checklist
//   build credits [area]      - who built what, here or in a named area
//   build audit               - grade the room you are standing in
//   build audit room [<vnum>] - grade one room, or the one you are in
//   build audit item  <what>  - grade one item prototype
//   build audit mob   <what>  - grade one mobile prototype
//   build audit quest <vnum>  - grade one quest
//   build audit area [<key>]  - grade an area and everything in it
//   build audit world         - grade the whole world
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
    if verb == "audit" {
        let rest = [];
        let i = 1;
        while i < parts.len() {
            rest.push(parts[i]);
            i += 1;
        }
        do_audit(char, rest, connection_id);
        return;
    }

    send_client_message(connection_id, usage());
}

fn usage() {
    let out = "\x1b[1;36mbuild\x1b[0m — builder tools\n";
    out += "  build                     your builder sheet\n";
    out += "  build who                 every builder, best first\n";
    out += "  build next                the one thing worth doing next\n";
    out += "  build track [area]        your path, or an area readiness checklist\n";
    out += "  build credits [area]      who built what, here or in an area\n";
    out += "  build audit               grade the room you are in\n";
    out += "  build audit room [<vnum>] grade one room, or the one you are in\n";
    out += "  build audit item <what>   grade one item prototype\n";
    out += "  build audit mob <what>    grade one mobile prototype\n";
    out += "  build audit quest <vnum>  grade one quest\n";
    out += "  build audit area [<key>]  grade an area and its contents\n";
    out += "  build audit world         grade the whole world\n";
    out += "  \x1b[0;37m<what> is a vnum, or the keyword of something in this room.\x1b[0m";
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
    if what == "area" || what == "zone" {
        if key == "" { key = current_area_key(char); }
        if key == "" {
            send_client_message(connection_id, "This room belongs to no area. Try `build audit area <name>`.");
            return;
        }
        show_area(key, connection_id);
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

fn current_area_key(char) {
    let room = get_room_data(char.current_room_id);
    if room == () { return ""; }
    let area = get_area_data(room.area_id);
    if area == () { return ""; }
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

// `C  62  [######----]`, coloured by letter. Every surface that shows a grade
// uses this, so the bar and the letter can never disagree.
fn grade_tag(score, letter) {
    return letter_colour(letter) + letter + "\x1b[0m " + text::pad_left("" + score, 3)
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
    out += "  " + grade_tag(report.score, report.letter) + "   "
        + tally_line(report.blockers, report.warns, report.polish) + "\n";
    out += credit_line(report) + "\n";
    out += findings_block(report.findings);
    send_client_message(connection_id, out);
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

fn show_area(key, connection_id) {
    let report = audit_area_report(key);
    if !report.found {
        send_client_message(connection_id, "No area matches '" + key + "'.");
        return;
    }

    let out = "\x1b[1;36m" + report.name + "\x1b[0m\n";
    out += "  " + grade_tag(report.score, report.letter) + "   "
        + tally_line(report.blockers, report.warns, report.polish) + "\n\n";

    out += contents_line(report.counts);
    out += "\n\x1b[1;37mArea\x1b[0m\n";
    out += findings_block(report.findings);

    out += worst_block(report);
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

fn worst_block(report) {
    if report.worst.len() == 0 { return ""; }
    let out = "\n\x1b[1;37mWorst first\x1b[0m\n";
    for e in report.worst {
        let top = "";
        if e.findings.len() > 0 { top = e.findings[0].message; }
        out += "  " + letter_colour(e.letter) + e.letter + "\x1b[0m "
            + text::pad_left("" + e.score, 3) + "  "
            + column(e.kind, 8)
            + column(e.label, 13)
            + column(e.name, 28) + top + "\n";
    }
    if report.truncated {
        out += "  \x1b[0;37m...and more. Fix these first, then run it again.\x1b[0m\n";
    }
    return out;
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
    out += "  " + grade_tag(report.score, report.letter) + "   "
        + tally_line(report.blockers, report.warns, report.polish) + "\n\n";
    out += facts_block(f);

    out += "\n\x1b[1;37mWorld\x1b[0m\n";
    out += findings_block(report.findings);

    out += "\n\x1b[1;37mAreas\x1b[0m\n";
    for a in report.areas {
        out += "  " + letter_colour(a.letter) + a.letter + "\x1b[0m "
            + text::pad_left("" + a.score, 3) + "  "
            + column(a.label, 14)
            + column(a.name, 26)
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
