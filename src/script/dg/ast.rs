//! AST for DG Scripts. The parser produces [`Block`] (a flat `Vec<Stmt>`);
//! evaluation walks each statement in order.
//!
//! Conditions and command lines are stored as raw strings — variable
//! interpolation (`%actor.level%`) happens at evaluation time, not parse
//! time, because `%self.id%` and friends bind to the runtime ctx.

pub type Block = Vec<Stmt>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `if <cond>` ... [`elseif <cond>` ...] ... [`else` ...] `end`
    If {
        cond: String,
        then_body: Block,
        elif_branches: Vec<(String, Block)>,
        else_body: Option<Block>,
    },

    /// `while <cond>` ... `done`
    While { cond: String, body: Block },

    /// `switch <value>` ... [`case <pat>` ... [`break`]] ... [`default` ...] `done`
    Switch {
        value: String,
        cases: Vec<SwitchCase>,
        default: Option<Block>,
    },

    /// `set <var> <value>` — value is interpolated at eval time.
    Set { var: String, value: String },

    /// `eval <var> <expr>` — string-substitute, then store result.
    Eval { var: String, expr: String },

    /// `halt` — stop executing this script immediately.
    Halt,

    /// `return <n>` — stop with a status. `return 0` from a COMMAND
    /// trigger cancels the host action; non-zero lets it proceed.
    Return(i32),

    /// `break` — exit innermost switch/while.
    Break,

    /// A command line: `%send% %actor% Hello` or `mecho Hi all` or
    /// `mload mob 1234`. Stored as a raw line; `cmds::dispatch` parses
    /// and routes it after substitution.
    Cmd(String),

    /// Comment line (`* ...`). Kept in the AST for round-tripping but
    /// the evaluator skips it.
    Comment(String),

    /// `wait <duration>` — Phase 2 cooperative suspension.
    /// `<duration>` is the raw arglist (e.g. "3 sec", "1 min", "until 6:00",
    /// or just "30" for game pulses); parsed at eval time by [`super::eval::parse_wait_secs`].
    Wait(String),
    /// `context <id>` — switches subsequent `global`/`unset` to write to
    /// the entity with that id's `dg_vars`. Empty/0 clears.
    Context(String),
    /// `global <var>` — promote the named local variable to a durable
    /// store. With no `context` set, writes to world `dg_globals`. With
    /// a `context` set, writes to that entity's `dg_vars`.
    Global(String),
    /// `unset <var>` — remove from locals + world globals + context
    /// entity's vars. Broad clear matches tbamud's permissive intent.
    Unset(String),
    /// `nop <expr>` — evaluate an expression for side effects, discard
    /// result. Treated as a comment.
    Nop(String),
    /// `attach <trig_vnum> <target>` — Phase 3.
    Attach { trig_vnum: String, target: String },
    /// `detach <trig_vnum> <target>` — Phase 3.
    Detach { trig_vnum: String, target: String },
    /// `remote <var> <uuid>` — write `var`'s current value (resolved
    /// from locals → globals) into the entity's `dg_vars`. IronMUD also
    /// accepts a third-arg form `remote <var> <uuid> <value>` that
    /// writes the substituted `<value>` directly without consulting
    /// locals — handy when the source value isn't bound to a local of
    /// the same name as the destination.
    Remote {
        var: String,
        target: String,
        value: Option<String>,
    },
    /// `rdelete <var> <uuid>` — remove `var` from the entity's `dg_vars`.
    Rdelete { var: String, target: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchCase {
    /// One or more `case <pat>` patterns. Multiple consecutive `case`
    /// lines before any code share the same body (DG fall-through).
    pub patterns: Vec<String>,
    pub body: Block,
    /// `true` when the body ends without a `break` and execution should
    /// fall through to the next case clause.
    pub fall_through: bool,
}
