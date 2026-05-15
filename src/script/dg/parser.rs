//! Line-oriented parser for DG Scripts. Produces a [`super::ast::Block`].
//!
//! DG is whitespace-insensitive between tokens but **line-significant**:
//! every statement starts at the beginning of a line. Indentation is
//! cosmetic. Block structure comes from explicit terminators — `end` for
//! `if`, `done` for `while`/`switch`.
//!
//! Errors carry a line number for diagnostics. The parser is forgiving:
//! unknown leading tokens become [`Stmt::Cmd`] so that the command
//! dispatcher (which knows about `mecho`, `%send%`, `dg_cast`, etc.) gets
//! a chance to handle them. Unmatched `end`/`done`/`break` outside a
//! block are reported but don't abort.

use super::ast::{Block, Stmt, SwitchCase};

/// Canonical list of DG language keywords. Surfaced for the editor's
/// syntax highlighter and tab-completion source. Keep this in sync with
/// the dispatcher in `parse_block` below.
pub const KEYWORDS: &[&str] = &[
    "if", "elseif", "else", "end",
    "while", "done",
    "switch", "case", "default", "break",
    "set", "eval", "halt", "return",
    "wait", "context", "global", "unset",
    "remote", "rdelete",
];

/// Common DG variable substitutions / fields. Used by tab completion
/// after the user types `%`. The list is intentionally a curated subset
/// — DG variables are open-ended (any UUID prefix works), so completion
/// only knows the common ones.
pub const VARIABLES: &[&str] = &[
    "actor", "victim", "self", "arg", "cmd", "speech",
    "actor.name", "actor.level", "actor.hitp", "actor.maxhp",
    "actor.gold", "actor.move", "actor.vnum", "actor.is_pc",
    "actor.room", "actor.fighting", "actor.heshe", "actor.himher",
    "actor.hisher", "actor.str", "actor.dex", "actor.con",
    "actor.int", "actor.wis", "actor.cha", "actor.class", "actor.race",
    "self.name", "self.vnum", "self.room", "self.hitp", "self.maxhp",
    "self.fighting", "self.is_pc",
    "victim.name", "victim.vnum", "victim.hitp",
    "random.10", "random.100", "random.6",
];

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DG parse error at line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for ParseError {}

pub fn parse(body: &str) -> Result<Block, ParseError> {
    let lines: Vec<&str> = body.lines().collect();
    let mut p = Parser { lines: &lines, idx: 0 };
    let block = p.parse_block(BlockEnd::Eof)?;
    Ok(block)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockEnd {
    Eof,
    EndKw,    // `end`
    DoneKw,   // `done`
    /// Switch case body — terminates on `case`, `default`, `break`,
    /// or `done`. The terminator stays unconsumed so the outer loop
    /// can decide what to do with it.
    SwitchCaseBody,
    /// Inside an `if` after the `then` block has been consumed —
    /// terminates on `elseif`, `else`, or `end`. Terminator unconsumed.
    IfClause,
}

struct Parser<'a> {
    lines: &'a [&'a str],
    idx: usize,
}

impl<'a> Parser<'a> {
    fn peek_raw(&self) -> Option<&'a str> {
        self.lines.get(self.idx).copied()
    }

    fn line_no(&self) -> usize {
        self.idx + 1
    }

    fn parse_block(&mut self, end: BlockEnd) -> Result<Block, ParseError> {
        let mut out: Block = Vec::new();
        while let Some(raw) = self.peek_raw() {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                self.idx += 1;
                continue;
            }

            // Comment: `*` at start.
            if let Some(rest) = trimmed.strip_prefix('*') {
                out.push(Stmt::Comment(rest.trim_start().to_string()));
                self.idx += 1;
                continue;
            }

            // Pull the leading keyword — case-insensitive — to decide.
            let (kw, _rest) = split_kw(trimmed);
            let lk = kw.to_ascii_lowercase();

            // Block-terminator handling depends on context.
            match (lk.as_str(), end) {
                ("end", BlockEnd::EndKw) | ("end", BlockEnd::IfClause) => {
                    return Ok(out);
                }
                ("done", BlockEnd::DoneKw) | ("done", BlockEnd::SwitchCaseBody) => {
                    return Ok(out);
                }
                ("elseif", BlockEnd::IfClause) | ("else", BlockEnd::IfClause) => {
                    return Ok(out);
                }
                ("case", BlockEnd::SwitchCaseBody) | ("default", BlockEnd::SwitchCaseBody) => {
                    return Ok(out);
                }
                ("break", BlockEnd::SwitchCaseBody) => {
                    self.idx += 1;
                    out.push(Stmt::Break);
                    return Ok(out);
                }
                // Top-level stray `end`/`done` — treat as silent end-of-block
                // when we're at EOF expectation, otherwise surface an error.
                ("end", BlockEnd::Eof) | ("done", BlockEnd::Eof) | ("break", BlockEnd::Eof) => {
                    // Permissive: skip and warn via Comment.
                    out.push(Stmt::Comment(format!("(stray '{}')", lk)));
                    self.idx += 1;
                    continue;
                }
                _ => {}
            }

            // Statement dispatch.
            match lk.as_str() {
                "if" => out.push(self.parse_if()?),
                "while" => out.push(self.parse_while()?),
                "switch" => out.push(self.parse_switch()?),
                "set" => out.push(self.parse_set()?),
                "eval" => out.push(self.parse_eval()?),
                "halt" => {
                    self.idx += 1;
                    out.push(Stmt::Halt);
                }
                "return" => {
                    let rest = trimmed["return".len()..].trim();
                    let n = rest.parse::<i32>().unwrap_or(1);
                    self.idx += 1;
                    out.push(Stmt::Return(n));
                }
                "break" => {
                    // `break` outside switch — treat as stmt; eval ignores
                    // it where it can't unwind.
                    self.idx += 1;
                    out.push(Stmt::Break);
                }
                "wait" => {
                    let rest = trimmed["wait".len()..].trim().to_string();
                    self.idx += 1;
                    out.push(Stmt::Wait(rest));
                }
                "context" => {
                    let rest = trimmed["context".len()..].trim().to_string();
                    self.idx += 1;
                    out.push(Stmt::Context(rest));
                }
                "global" => {
                    let rest = trimmed["global".len()..].trim().to_string();
                    self.idx += 1;
                    out.push(Stmt::Global(rest));
                }
                "unset" => {
                    let rest = trimmed["unset".len()..].trim().to_string();
                    self.idx += 1;
                    out.push(Stmt::Unset(rest));
                }
                "nop" => {
                    let rest = trimmed["nop".len()..].trim().to_string();
                    self.idx += 1;
                    out.push(Stmt::Nop(rest));
                }
                "attach" => {
                    let rest = trimmed["attach".len()..].trim();
                    let mut tok = rest.split_whitespace();
                    let trig = tok.next().unwrap_or("").to_string();
                    let target = tok.next().unwrap_or("").to_string();
                    self.idx += 1;
                    out.push(Stmt::Attach { trig_vnum: trig, target });
                }
                "detach" => {
                    let rest = trimmed["detach".len()..].trim();
                    let mut tok = rest.split_whitespace();
                    let trig = tok.next().unwrap_or("").to_string();
                    let target = tok.next().unwrap_or("").to_string();
                    self.idx += 1;
                    out.push(Stmt::Detach { trig_vnum: trig, target });
                }
                "remote" => {
                    let rest = trimmed["remote".len()..].trim();
                    // Take var + target as whitespace tokens; the remainder
                    // (if any) is the optional explicit value, preserved as
                    // a single string so multi-word values like
                    // `remote greeting %actor.id% Welcome back` round-trip.
                    let var_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                    let var = rest[..var_end].to_string();
                    let after_var = rest[var_end..].trim_start();
                    let target_end = after_var.find(char::is_whitespace).unwrap_or(after_var.len());
                    let target = after_var[..target_end].to_string();
                    let after_target = after_var[target_end..].trim_start();
                    let value = if after_target.is_empty() {
                        None
                    } else {
                        Some(after_target.to_string())
                    };
                    self.idx += 1;
                    out.push(Stmt::Remote { var, target, value });
                }
                "rdelete" => {
                    let rest = trimmed["rdelete".len()..].trim();
                    let mut tok = rest.split_whitespace();
                    let var = tok.next().unwrap_or("").to_string();
                    let target = tok.next().unwrap_or("").to_string();
                    self.idx += 1;
                    out.push(Stmt::Rdelete { var, target });
                }
                // Anything else: treat the entire line as a command.
                // The dispatcher in cmds::dispatch knows about `%send%`,
                // `mecho`, `mload`, etc.
                _ => {
                    self.idx += 1;
                    out.push(Stmt::Cmd(trimmed.to_string()));
                }
            }
        }

        match end {
            BlockEnd::Eof => Ok(out),
            BlockEnd::EndKw => Err(ParseError {
                line: self.line_no(),
                msg: "expected `end` before end of script".into(),
            }),
            BlockEnd::DoneKw | BlockEnd::SwitchCaseBody => Err(ParseError {
                line: self.line_no(),
                msg: "expected `done` before end of script".into(),
            }),
            BlockEnd::IfClause => Err(ParseError {
                line: self.line_no(),
                msg: "expected `else`/`elseif`/`end` before end of script".into(),
            }),
        }
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let line = self.peek_raw().unwrap_or("");
        let cond = line.trim()["if".len()..].trim().to_string();
        self.idx += 1;
        let then_body = self.parse_block(BlockEnd::IfClause)?;

        let mut elif_branches: Vec<(String, Block)> = Vec::new();
        let mut else_body: Option<Block> = None;

        while let Some(raw) = self.peek_raw() {
            let t = raw.trim();
            let (kw, _) = split_kw(t);
            match kw.to_ascii_lowercase().as_str() {
                "elseif" => {
                    let c = t["elseif".len()..].trim().to_string();
                    self.idx += 1;
                    let body = self.parse_block(BlockEnd::IfClause)?;
                    elif_branches.push((c, body));
                }
                "else" => {
                    self.idx += 1;
                    let body = self.parse_block(BlockEnd::EndKw)?;
                    else_body = Some(body);
                    break;
                }
                "end" => {
                    self.idx += 1;
                    return Ok(Stmt::If { cond, then_body, elif_branches, else_body });
                }
                _ => {
                    return Err(ParseError {
                        line: self.line_no(),
                        msg: format!("expected `elseif`/`else`/`end`, got `{}`", kw),
                    });
                }
            }
        }

        // Already consumed `else`; expect `end` next.
        if let Some(raw) = self.peek_raw() {
            let t = raw.trim();
            if t.eq_ignore_ascii_case("end") {
                self.idx += 1;
                return Ok(Stmt::If { cond, then_body, elif_branches, else_body });
            }
        }
        Err(ParseError {
            line: self.line_no(),
            msg: "expected `end` after `else` body".into(),
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let line = self.peek_raw().unwrap_or("");
        let cond = line.trim()["while".len()..].trim().to_string();
        self.idx += 1;
        let body = self.parse_block(BlockEnd::DoneKw)?;
        // Consume the `done`.
        if let Some(raw) = self.peek_raw() {
            if raw.trim().eq_ignore_ascii_case("done") {
                self.idx += 1;
            }
        }
        Ok(Stmt::While { cond, body })
    }

    fn parse_switch(&mut self) -> Result<Stmt, ParseError> {
        let line = self.peek_raw().unwrap_or("");
        let value = line.trim()["switch".len()..].trim().to_string();
        self.idx += 1;

        let mut cases: Vec<SwitchCase> = Vec::new();
        let mut default: Option<Block> = None;

        // Loop over case/default clauses until `done`.
        loop {
            // Collect patterns: consecutive `case ...` lines stack.
            let mut patterns: Vec<String> = Vec::new();
            while let Some(raw) = self.peek_raw() {
                let t = raw.trim();
                if t.is_empty() {
                    self.idx += 1;
                    continue;
                }
                let (kw, _) = split_kw(t);
                match kw.to_ascii_lowercase().as_str() {
                    "case" => {
                        let p = t["case".len()..].trim().to_string();
                        patterns.push(p);
                        self.idx += 1;
                    }
                    "default" => {
                        if !patterns.is_empty() {
                            // Patterns without a body before `default`:
                            // they're an empty fall-through case. Push a
                            // case with empty body + fall_through=true.
                            cases.push(SwitchCase { patterns: std::mem::take(&mut patterns), body: Vec::new(), fall_through: true });
                        }
                        self.idx += 1;
                        let body = self.parse_block(BlockEnd::DoneKw)?;
                        default = Some(body);
                        // Consume `done`.
                        if let Some(raw2) = self.peek_raw() {
                            if raw2.trim().eq_ignore_ascii_case("done") {
                                self.idx += 1;
                            }
                        }
                        return Ok(Stmt::Switch { value, cases, default });
                    }
                    "done" => {
                        if !patterns.is_empty() {
                            cases.push(SwitchCase { patterns: std::mem::take(&mut patterns), body: Vec::new(), fall_through: false });
                        }
                        self.idx += 1;
                        return Ok(Stmt::Switch { value, cases, default });
                    }
                    _ => break,
                }
            }
            if patterns.is_empty() {
                // No case/default/done found — must have hit eof.
                return Err(ParseError {
                    line: self.line_no(),
                    msg: "expected `case`, `default`, or `done` inside switch".into(),
                });
            }

            // Body for these patterns. Terminates on case/default/break/done.
            let body = self.parse_block(BlockEnd::SwitchCaseBody)?;

            // Did the body end with an explicit Break?
            let fall_through = !matches!(body.last(), Some(Stmt::Break));
            // Strip the trailing Break since we encode it in fall_through.
            let mut body_trimmed = body;
            if matches!(body_trimmed.last(), Some(Stmt::Break)) {
                body_trimmed.pop();
            }
            cases.push(SwitchCase { patterns, body: body_trimmed, fall_through });
        }
    }

    fn parse_set(&mut self) -> Result<Stmt, ParseError> {
        let line = self.peek_raw().unwrap_or("").trim();
        // `set <var> <value...>`
        let rest = line["set".len()..].trim_start();
        let (var, value) = split_kw(rest);
        self.idx += 1;
        Ok(Stmt::Set { var: var.to_string(), value: value.trim_start().to_string() })
    }

    fn parse_eval(&mut self) -> Result<Stmt, ParseError> {
        let line = self.peek_raw().unwrap_or("").trim();
        let rest = line["eval".len()..].trim_start();
        let (var, expr) = split_kw(rest);
        self.idx += 1;
        Ok(Stmt::Eval { var: var.to_string(), expr: expr.trim_start().to_string() })
    }
}

/// Split a line at the first whitespace gap into (head, tail). `tail` is
/// returned with leading whitespace preserved (callers that want it
/// trimmed should `.trim_start()` themselves).
fn split_kw(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(i) => (&line[..i], &line[i..]),
        None => (line, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_body() {
        let b = parse("").unwrap();
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn parses_simple_if() {
        let body = "\
if %actor.level% > 5
  %send% %actor% Strong.
else
  %send% %actor% Weak.
end";
        let b = parse(body).unwrap();
        assert_eq!(b.len(), 1);
        match &b[0] {
            Stmt::If { cond, then_body, else_body, .. } => {
                assert_eq!(cond, "%actor.level% > 5");
                assert_eq!(then_body.len(), 1);
                assert!(matches!(&then_body[0], Stmt::Cmd(s) if s.starts_with("%send%")));
                assert!(else_body.is_some());
            }
            other => panic!("not an if: {other:?}"),
        }
    }

    #[test]
    fn parses_elseif_chain() {
        let body = "\
if %x% == 1
  %send% %actor% one
elseif %x% == 2
  %send% %actor% two
elseif %x% == 3
  %send% %actor% three
else
  %send% %actor% other
end";
        let b = parse(body).unwrap();
        match &b[0] {
            Stmt::If { elif_branches, else_body, .. } => {
                assert_eq!(elif_branches.len(), 2);
                assert!(else_body.is_some());
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn parses_switch_with_fallthrough() {
        // From tbamud 52.trg #5201 (magic user) — case 1/2/3 fall together.
        let body = "\
switch %actor.level%
  case 1
  case 2
  case 3
  break
  case 4
    dg_cast 'magic missile' %actor%
  break
  default
    dg_cast 'fireball' %actor%
  break
done";
        let b = parse(body).unwrap();
        match &b[0] {
            Stmt::Switch { value, cases, default } => {
                assert_eq!(value, "%actor.level%");
                // Stacked patterns: cases[0] should have patterns 1/2/3.
                assert_eq!(cases[0].patterns, vec!["1", "2", "3"]);
                assert!(!cases[0].fall_through, "explicit break should clear fall_through");
                assert_eq!(cases[1].patterns, vec!["4"]);
                assert!(default.is_some());
            }
            _ => panic!("expected switch"),
        }
    }

    #[test]
    fn parses_while_loop() {
        let body = "\
while %i% < 10
  eval i %i% + 1
done";
        let b = parse(body).unwrap();
        match &b[0] {
            Stmt::While { cond, body } => {
                assert_eq!(cond, "%i% < 10");
                assert_eq!(body.len(), 1);
            }
            _ => panic!("expected while"),
        }
    }

    #[test]
    fn parses_command_lines_as_cmd() {
        let body = "\
%send% %actor% Hello.
mecho The room shakes.
mload mob 1234
dg_cast 'fireball' %actor%";
        let b = parse(body).unwrap();
        assert_eq!(b.len(), 4);
        for s in &b {
            assert!(matches!(s, Stmt::Cmd(_)));
        }
    }

    #[test]
    fn parses_set_and_eval() {
        let body = "\
set foo 42
eval bar %foo% + 1";
        let b = parse(body).unwrap();
        assert_eq!(b.len(), 2);
        match &b[0] {
            Stmt::Set { var, value } => {
                assert_eq!(var, "foo");
                assert_eq!(value, "42");
            }
            _ => panic!("expected set"),
        }
        match &b[1] {
            Stmt::Eval { var, expr } => {
                assert_eq!(var, "bar");
                assert_eq!(expr, "%foo% + 1");
            }
            _ => panic!("expected eval"),
        }
    }

    #[test]
    fn parses_halt_and_return() {
        let b = parse("halt").unwrap();
        assert!(matches!(b[0], Stmt::Halt));

        let b = parse("return 0").unwrap();
        assert!(matches!(b[0], Stmt::Return(0)));

        let b = parse("return 1").unwrap();
        assert!(matches!(b[0], Stmt::Return(1)));
    }

    #[test]
    fn parses_real_panning_trigger() {
        // tbamud 7.trg #700 panning for gold.
        let body = "\
if %actor.move% <= 10
  %send% %actor% You are too exhausted.
  halt
end
if %cmd% /= pan && %arg% /= gold
  eval heldobj %actor.eq(hold)%
  if %heldobj.vnum% == 717
    %send% %actor% You dip your pan into the river...
    nop %actor.move(-10)%
    wait 3 sec
    if %random.10% == 1
      %load% obj 718 %actor% inv
    end
  end
end";
        let b = parse(body).unwrap();
        assert_eq!(b.len(), 2);
        // Both top-level stmts are If.
        assert!(matches!(b[0], Stmt::If { .. }));
        assert!(matches!(b[1], Stmt::If { .. }));
    }
}
