//! IronMUD legacy-content importer.
//!
//! Translates world data from older MUD engines (CircleMUD, ...) into
//! IronMUD's room/area model. Default mode is dry-run: no DB writes are made
//! unless `--apply` is passed. The destination Sled DB must not be in use by
//! a running server (Sled holds an exclusive file lock).
//!
//! Exit codes:
//!   0  clean dry-run or successful apply
//!   1  parse / I/O error
//!   2  dry-run finished with blocking warnings (use --apply only after fixing them)
//!   3  apply failed mid-write

#![recursion_limit = "512"]

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use ironmud::db::Db;
use ironmud::import::{
    MappingOptions, MudEngine, Severity, engines::circle::CircleEngine, engines::ranvier, engines::tba::TbaEngine,
    mapping, writer,
};

#[derive(Parser)]
#[command(name = "ironmud-import")]
#[command(about = "Import legacy MUD content (rooms/areas) into an IronMUD database")]
struct Cli {
    /// IronMUD database path. The server must not be running against it.
    #[arg(short, long, default_value = "ironmud.db", env = "IRONMUD_DATABASE")]
    database: String,

    #[command(subcommand)]
    engine: Engine,
}

#[derive(Subcommand)]
enum Engine {
    /// Import CircleMUD 3.x world data (.wld + .zon).
    Circle {
        /// Path to a CircleMUD installation. Auto-detects whether you point
        /// at the repo root, `lib/`, or `lib/world/`.
        #[arg(short, long)]
        source: PathBuf,

        /// Commit changes to the DB. Without this flag the importer runs as
        /// a dry-run and never writes.
        #[arg(long)]
        apply: bool,

        /// Restrict to a single zone vnum (debug aid).
        #[arg(long)]
        zone: Option<i32>,

        /// Override the default mapping JSON. See docs/import-guide.md.
        #[arg(long)]
        mapping: Option<PathBuf>,

        /// Also write a JSON report of warnings + summary to this path.
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Import a Ranvier (Node.js MUD) bundle. Walks
    /// `<bundle>/areas/<area>/{manifest,rooms,npcs,items,quests,loot-pools}.yml`.
    /// JS scripts under `<area>/scripts/` are skipped with warnings.
    Ranvier {
        /// Path to a Ranvier bundle directory (the one containing `areas/`),
        /// or directly to its `areas/` subdirectory.
        #[arg(short, long)]
        source: PathBuf,

        /// Commit changes to the DB. Without this flag the importer runs as
        /// a dry-run and never writes.
        #[arg(long)]
        apply: bool,

        /// Override the bundle name used for the vnum-map sidecar
        /// (`imports/<bundle-name>.vnum-map.json`). Defaults to the source
        /// directory's basename.
        #[arg(long)]
        bundle_name: Option<String>,

        /// Base vnum for the first area's window. Each area gets a
        /// 1000-vnum window starting here.
        #[arg(long, default_value_t = 60000)]
        vnum_base: i32,

        /// Base vnum for synthesized quests in this bundle.
        #[arg(long, default_value_t = 9000)]
        quest_vnum_base: i32,

        /// Also write a JSON report of warnings + summary to this path.
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Import tbaMUD world data (extends CircleMUD format with 128-bit ascii
    /// flag fields, DG Scripts trigger attachments, and quests). DG Script
    /// bodies and quests are warn-only — re-author behavior in Rhai.
    Tba {
        /// Path to a tbaMUD installation. Auto-detects whether you point
        /// at the repo root, `lib/`, or `lib/world/`.
        #[arg(short, long)]
        source: PathBuf,

        /// Commit changes to the DB.
        #[arg(long)]
        apply: bool,

        /// Restrict to a single zone vnum (debug aid).
        #[arg(long)]
        zone: Option<i32>,

        /// Override the default mapping JSON. See docs/import-guide.md.
        #[arg(long)]
        mapping: Option<PathBuf>,

        /// Also write a JSON report of warnings + summary to this path.
        #[arg(long)]
        report: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt().with_max_level(tracing::Level::WARN).init();
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.engine {
        Engine::Circle {
            source,
            apply,
            zone,
            mapping: mapping_path,
            report,
        } => run_engine(&CircleEngine, &cli.database, source, apply, zone, mapping_path, report),
        Engine::Tba {
            source,
            apply,
            zone,
            mapping: mapping_path,
            report,
        } => run_engine(&TbaEngine, &cli.database, source, apply, zone, mapping_path, report),
        Engine::Ranvier {
            source,
            apply,
            bundle_name,
            vnum_base,
            quest_vnum_base,
            report,
        } => run_ranvier(
            &cli.database,
            source,
            apply,
            bundle_name,
            vnum_base,
            quest_vnum_base,
            report,
        ),
    }
}

fn run_ranvier(
    database: &str,
    source: PathBuf,
    apply: bool,
    bundle_name: Option<String>,
    vnum_base: i32,
    quest_vnum_base: i32,
    report_path: Option<PathBuf>,
) -> Result<ExitCode> {
    let bundle_name = bundle_name.unwrap_or_else(|| {
        source
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("ranvier-bundle")
            .to_string()
    });
    println!(
        "ironmud-import: engine=ranvier source={} bundle={} mode={}",
        source.display(),
        bundle_name,
        if apply { "apply" } else { "dry-run" }
    );

    let db = open_db(database)?;
    let existing_area_prefixes: Vec<String> = db
        .list_all_areas()?
        .into_iter()
        .map(|a| a.prefix.to_lowercase())
        .collect();
    let existing_room_vnums: Vec<String> = db.list_all_rooms()?.into_iter().filter_map(|r| r.vnum).collect();
    let existing_mobile_vnums: Vec<String> = db
        .list_all_mobiles()?
        .into_iter()
        .filter(|m| m.is_prototype)
        .map(|m| m.vnum.to_lowercase())
        .filter(|v| !v.is_empty())
        .collect();
    let existing_item_vnums: Vec<String> = db
        .list_all_items()?
        .into_iter()
        .filter(|i| i.is_prototype)
        .filter_map(|i| i.vnum.map(|v| v.to_lowercase()))
        .filter(|v| !v.is_empty())
        .collect();

    let import = ranvier::import_bundle(
        &source,
        &bundle_name,
        vnum_base,
        quest_vnum_base,
        &existing_room_vnums,
        &existing_mobile_vnums,
        &existing_item_vnums,
        &existing_area_prefixes,
    )?;
    let ranvier::RanvierImport {
        plan,
        warnings,
        post_patches,
    } = import;

    let blocking = warnings.iter().filter(|w| w.severity == Severity::Block).count();

    if !apply {
        let summary = writer::print_dry_run(&plan, &warnings);
        if let Some(p) = report_path {
            writer::write_report_file(&p, &plan, &warnings, &summary)?;
            println!("report: {}", p.display());
        }
        if blocking > 0 {
            return Ok(ExitCode::from(2));
        }
        return Ok(ExitCode::from(0));
    }

    if blocking > 0 {
        eprintln!("refusing to --apply: {blocking} blocking warning(s) — re-run without --apply to review.");
        writer::print_warnings(&warnings);
        return Ok(ExitCode::from(2));
    }
    match writer::apply(&db, &plan, &warnings) {
        Ok(summary) => {
            ranvier::post::apply_post_patches(&db, &post_patches)?;
            writer::print_warnings(&warnings);
            if !post_patches.room_coordinates.is_empty() || !post_patches.replace_on_respawn.is_empty() {
                println!(
                    "  ranvier post-pass: {} coordinate stamps, {} replace_on_respawn flags",
                    post_patches.room_coordinates.len(),
                    post_patches.replace_on_respawn.len()
                );
            }
            if let Some(p) = report_path {
                writer::write_report_file(&p, &plan, &warnings, &summary)?;
                println!("report: {}", p.display());
            }
            Ok(ExitCode::from(0))
        }
        Err(e) => {
            eprintln!("apply failed: {e:#}");
            Ok(ExitCode::from(3))
        }
    }
}

fn run_engine<E: MudEngine>(
    engine: &E,
    database: &str,
    source: PathBuf,
    apply: bool,
    zone_filter: Option<i32>,
    mapping_path: Option<PathBuf>,
    report_path: Option<PathBuf>,
) -> Result<ExitCode> {
    println!(
        "ironmud-import: engine={} source={} mode={}",
        engine.name(),
        source.display(),
        if apply { "apply" } else { "dry-run" }
    );

    let (mut ir, parse_warnings) = engine.parse(&source)?;
    if let Some(zv) = zone_filter {
        ir.zones.retain(|z| z.vnum == zv);
        if ir.zones.is_empty() {
            bail!("--zone {zv} matched no zones in {}", source.display());
        }
    }

    let mapping_table = match mapping_path {
        Some(p) => mapping::CircleMappingTable::load_from_path(&p)?,
        None => mapping::CircleMappingTable::load_default(),
    };

    // Pull existing area prefixes / room vnums from the DB so collision
    // warnings reflect the actual target. Opening the DB read-only would be
    // ideal but Sled doesn't expose that cleanly; we open it here for both
    // dry-run and apply. If the server holds the lock the open will fail
    // with a clear error and we abort.
    let db = open_db(database)?;
    let existing_area_prefixes: Vec<String> = db
        .list_all_areas()?
        .into_iter()
        .map(|a| a.prefix.to_lowercase())
        .collect();
    let existing_room_vnums: Vec<String> = db.list_all_rooms()?.into_iter().filter_map(|r| r.vnum).collect();
    // Only prototype mobile vnums collide; live spawned instances share the
    // prototype's vnum but are not authored content. Filter to prototypes
    // so we don't false-flag every spawned NPC as a collision source.
    let existing_mobile_vnums: Vec<String> = db
        .list_all_mobiles()?
        .into_iter()
        .filter(|m| m.is_prototype)
        .map(|m| m.vnum.to_lowercase())
        .filter(|v| !v.is_empty())
        .collect();
    // Item vnums: only prototypes count for collision purposes — spawned
    // instances share the prototype vnum but aren't authored content.
    let existing_item_vnums: Vec<String> = db
        .list_all_items()?
        .into_iter()
        .filter(|i| i.is_prototype)
        .filter_map(|i| i.vnum.map(|v| v.to_lowercase()))
        .filter(|v| !v.is_empty())
        .collect();

    let opts = MappingOptions {
        circle: mapping_table,
        existing_area_prefixes,
        existing_room_vnums,
        existing_mobile_vnums,
        existing_item_vnums,
    };

    let (plan, mut warnings) = mapping::ir_to_plan(&ir, &opts);
    let mut all_warnings = parse_warnings;
    all_warnings.append(&mut warnings);

    let blocking = all_warnings.iter().filter(|w| w.severity == Severity::Block).count();

    if !apply {
        let summary = writer::print_dry_run(&plan, &all_warnings);
        if let Some(p) = report_path {
            writer::write_report_file(&p, &plan, &all_warnings, &summary)?;
            println!("report: {}", p.display());
        }
        if blocking > 0 {
            return Ok(ExitCode::from(2));
        }
        return Ok(ExitCode::from(0));
    }

    // Apply path.
    if blocking > 0 {
        eprintln!("refusing to --apply: {blocking} blocking warning(s) — re-run without --apply to review.");
        writer::print_warnings(&all_warnings);
        return Ok(ExitCode::from(2));
    }
    match writer::apply(&db, &plan, &all_warnings) {
        Ok(summary) => {
            writer::print_warnings(&all_warnings);
            if let Some(p) = report_path {
                writer::write_report_file(&p, &plan, &all_warnings, &summary)?;
                println!("report: {}", p.display());
            }
            Ok(ExitCode::from(0))
        }
        Err(e) => {
            eprintln!("apply failed: {e:#}");
            Ok(ExitCode::from(3))
        }
    }
}

fn open_db(path: &str) -> Result<Db> {
    Db::open(path).map_err(|e| {
        anyhow::anyhow!(
            "could not open IronMUD database at {path}: {e}\n\
             (is the server running? Sled holds an exclusive lock; stop the server before importing)"
        )
    })
}
