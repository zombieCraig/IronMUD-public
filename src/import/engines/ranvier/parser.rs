//! YAML parser for Ranvier bundles. Walks the bundle directory, deserializes
//! every present `.yml` per area, and surfaces I/O / parse failures as
//! warnings rather than aborts wherever a partial import is still useful.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::ir::{IrArea, IrBundle, IrItem, IrLootEntry, IrManifest, IrNpc, IrQuest, IrRoom};
use crate::import::{Severity, SourceLoc, Warning, WarningKind};

pub fn parse_bundle(source: &Path, warnings: &mut Vec<Warning>) -> Result<IrBundle> {
    let areas_dir = resolve_areas_dir(source)?;
    let mut bundle = IrBundle::default();
    let entries = fs::read_dir(&areas_dir).with_context(|| format!("reading {}", areas_dir.display()))?;
    let mut area_dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    area_dirs.sort();
    for area_dir in area_dirs {
        match parse_area(&area_dir, warnings) {
            Ok(area) => bundle.areas.push(area),
            Err(e) => warnings.push(
                Warning::new(
                    WarningKind::Parse,
                    Severity::Block,
                    SourceLoc::file(&area_dir),
                    format!("failed to parse area: {e:#}"),
                )
                .with_suggestion("fix the YAML syntax error and re-run, or remove the area dir"),
            ),
        }
    }
    Ok(bundle)
}

/// A bundle path may be the bundle root (`<repo>/bundles/<bundle>`) or
/// already point at the `areas/` subdir. Auto-detect.
fn resolve_areas_dir(source: &Path) -> Result<PathBuf> {
    let direct = source.join("areas");
    if direct.is_dir() {
        return Ok(direct);
    }
    if source.file_name().and_then(|s| s.to_str()) == Some("areas") && source.is_dir() {
        return Ok(source.to_path_buf());
    }
    anyhow::bail!("could not find an `areas/` subdirectory under {}", source.display())
}

fn parse_area(area_dir: &Path, warnings: &mut Vec<Warning>) -> Result<IrArea> {
    let name = area_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("area dir has no name"))?;

    let manifest_path = area_dir.join("manifest.yml");
    let manifest: IrManifest = if manifest_path.exists() {
        load_yaml(&manifest_path, warnings).unwrap_or_default()
    } else {
        IrManifest::default()
    };

    let rooms: Vec<IrRoom> = load_yaml_list(&area_dir.join("rooms.yml"), warnings);
    let npcs: Vec<IrNpc> = load_yaml_list(&area_dir.join("npcs.yml"), warnings);
    let items: Vec<IrItem> = load_yaml_list(&area_dir.join("items.yml"), warnings);
    let quests: Vec<IrQuest> = load_yaml_list(&area_dir.join("quests.yml"), warnings);

    let loot_pools: HashMap<String, Vec<IrLootEntry>> = {
        let p = area_dir.join("loot-pools.yml");
        if p.exists() {
            load_yaml(&p, warnings).unwrap_or_default()
        } else {
            HashMap::new()
        }
    };

    let script_files = collect_script_files(area_dir);

    Ok(IrArea {
        name,
        manifest,
        rooms,
        npcs,
        items,
        quests,
        loot_pools,
        source_dir: area_dir.to_path_buf(),
        script_files,
    })
}

fn collect_script_files(area_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let scripts_root = area_dir.join("scripts");
    if !scripts_root.is_dir() {
        return out;
    }
    for sub in ["rooms", "npcs", "items"] {
        let d = scripts_root.join(sub);
        if !d.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("js") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Try to load a single YAML document into `T`. On failure, push a Block
/// warning and return `None` so the caller can continue with defaults.
fn load_yaml<T: serde::de::DeserializeOwned>(path: &Path, warnings: &mut Vec<Warning>) -> Option<T> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            warnings.push(Warning::new(
                WarningKind::Parse,
                Severity::Block,
                SourceLoc::file(path),
                format!("read failed: {e}"),
            ));
            return None;
        }
    };
    match serde_yaml::from_str::<T>(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            warnings.push(Warning::new(
                WarningKind::Parse,
                Severity::Block,
                SourceLoc::file(path),
                format!("yaml parse failed: {e}"),
            ));
            None
        }
    }
}

/// Like `load_yaml` but expects a YAML sequence at the top level. Missing
/// files produce an empty list (not a warning) — most areas omit several.
fn load_yaml_list<T: serde::de::DeserializeOwned>(path: &Path, warnings: &mut Vec<Warning>) -> Vec<T> {
    if !path.exists() {
        return Vec::new();
    }
    load_yaml::<Vec<T>>(path, warnings).unwrap_or_default()
}
