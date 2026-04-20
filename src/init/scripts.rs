//! Script loading and hot-reload functionality

use anyhow::Result;
use glob::glob;
use notify::{RecursiveMode, Watcher};
use std::path::Path;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::SharedState;

/// Load all Rhai scripts from the scripts directory
pub fn load_scripts(state: SharedState) -> Result<()> {
    let mut world = state.lock().unwrap();

    for entry in glob("scripts/**/*.rhai")? {
        match entry {
            Ok(path) => {
                let path_str = path.to_string_lossy().to_string();
                match world.engine.compile_file(path.clone()) {
                    Ok(ast) => {
                        info!("Loaded script: {}", path_str);
                        world.scripts.insert(path_str, ast);
                    }
                    Err(e) => {
                        error!("Failed to compile script {}: {}", path_str, e);
                    }
                }
            }
            Err(e) => {
                error!("Glob error: {}", e);
            }
        }
    }

    Ok(())
}

/// Watch the scripts directory for changes and hot-reload modified scripts
pub fn watch_scripts(state: SharedState) {
    let (tx, mut rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let mut watcher = notify::recommended_watcher(move |res| {
            tx.blocking_send(res).unwrap();
        })
        .unwrap();

        watcher.watch(Path::new("scripts/"), RecursiveMode::Recursive).unwrap();

        info!("Watching scripts directory for changes...");

        while let Some(res) = rx.recv().await {
            match res {
                Ok(event) => {
                    if let notify::EventKind::Modify(_) = event.kind {
                        for path in event.paths {
                            // Normalize to relative path to match how scripts are loaded and looked up
                            let path_str = path
                                .strip_prefix(std::env::current_dir().unwrap_or_default())
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .to_string();
                            if path_str.ends_with(".rhai") {
                                info!("Script changed: {}, recompiling.", path_str);
                                let mut world = state.lock().unwrap();
                                match world.engine.compile_file(path.clone()) {
                                    Ok(ast) => {
                                        world.scripts.insert(path_str.to_string(), ast);
                                        info!("Script recompiled successfully: {}", path_str);
                                    }
                                    Err(e) => {
                                        error!("Failed to recompile script {}: {}", path_str, e);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => error!("Watch error: {:?}", e),
            }
        }
    });
}
