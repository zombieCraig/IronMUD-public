// scripts/lib/text.rhai
// Shared string helpers. Import: import "text" as text;
//
// Rhai 1.x has no `trim` on strings — calling it returns () and breaks the
// binding with a confusing error — so every command that takes a free-form
// argument has to strip padding by hand. Doing that once here is the point:
// the alternative is each command growing its own copy, which is how `top`
// ended up able to handle `top  renown` while `standing` could not handle
// `standing  watch`.

// Strip leading and trailing spaces.
fn trim_spaces(s) {
    let out = "" + s;
    while out.starts_with(" ") { out = out.sub_string(1); }
    while out.ends_with(" ") { out = out.sub_string(0, out.len() - 1); }
    return out;
}

// Trim and lowercase — what a command wants before matching a player's word
// against a key.
fn normalise(s) {
    return trim_spaces(s).to_lower();
}

// Pad to `width` with trailing spaces. Longer text is left alone rather than
// truncated: a clipped name is worse than a ragged column.
fn pad_right(text, width) {
    let out = "" + text;
    while out.len() < width { out += " "; }
    return out;
}

// Pad to `width` with leading spaces, for right-aligned numbers.
fn pad_left(text, width) {
    let out = "" + text;
    while out.len() < width { out = " " + out; }
    return out;
}
