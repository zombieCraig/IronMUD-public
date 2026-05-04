//! Per-engine parsers. Each subdirectory is one source MUD engine.
//!
//! Adding a new engine:
//! 1. Create `engines/<name>/mod.rs` and implement [`crate::import::MudEngine`].
//! 2. Register it on the CLI in `src/bin/ironmud-import.rs`.
//! 3. Document its coverage matrix in `docs/import-guide.md`.

pub mod circle;
