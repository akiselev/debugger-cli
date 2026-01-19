//! Debug adapter setup and installation
//!
//! This module provides functionality to install, manage, and verify DAP-compatible
//! debug adapters across different platforms.

pub mod adapters;
pub mod detector;
pub mod installer;
pub mod registry;
pub mod verifier;

use crate::common::Result;
use std::path::PathBuf;

/// Options for the setup command
#[derive(Debug, Clone)]
pub struct SetupOptions {
    /// Specific debugger to install
    pub debugger: Option<String>,
    /// Specific version to install
    pub version: Option<String>,
    /// List available debuggers
    pub list: bool,
    /// Check installed debuggers
    pub check: bool,
    /// Auto-detect project types and install appropriate debuggers
    pub auto_detect: bool,
    /// Uninstall instead of install
    pub uninstall: bool,
    /// Show installation path
    pub path: bool,
    /// Force reinstall
    pub force: bool,
    /// Dry run mode
    pub dry_run: bool,
    /// Output as JSON
    pub json: bool,
}

/// Result of a setup operation
#[derive(Debug, Clone, serde::Serialize)]
pub struct SetupResult {
    pub status: SetupStatus,
    pub debugger: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Status of a setup operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStatus {
    Success,
    AlreadyInstalled,
    Uninstalled,
    NotFound,
    Failed,
    DryRun,
}

/// Run the setup command
pub async fn run(opts: SetupOptions) -> Result<()> {
    if opts.list {
        return list_debuggers(opts.json).await;
    }

    if opts.check {
        return check_debuggers(opts.json).await;
    }

    if opts.auto_detect {
        return auto_setup(opts).await;
    }

    // Need a debugger name for other operations
    let debugger = match &opts.debugger {
        Some(d) => d.clone(),
        None => {
            if opts.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "message": "No debugger specified. Use --list to see available debuggers."
                    })
                );
            } else {
                println!("No debugger specified. Use --list to see available debuggers.");
                println!();
                println!("Available debuggers:");
                for info in registry::all_debuggers() {
                    println!(
                        "  {:12} - {} ({})",
                        info.id,
                        info.description,
                        info.languages.join(", ")
                    );
                }
            }
            return Ok(());
        }
    };

    if opts.path {
        return show_path(&debugger, opts.json).await;
    }

    if opts.uninstall {
        return uninstall_debugger(&debugger, opts.json).await;
    }

    // Install the debugger
    install_debugger(&debugger, opts).await
}

/// List all available debuggers and their status
async fn list_debuggers(json: bool) -> Result<()> {
    let debuggers = registry::all_debuggers();
    let mut results = Vec::new();

    for info in debuggers {
        let installer = registry::get_installer(info.id);
        let status = if let Some(inst) = &installer {
            inst.status().await.ok()
        } else {
            None
        };

        let status_str = match &status {
            Some(installer::InstallStatus::Installed { version, .. }) => {
                if let Some(v) = version {
                    format!("installed ({})", v)
                } else {
                    "installed".to_string()
                }
            }
            Some(installer::InstallStatus::Broken { reason, .. }) => {
                format!("broken: {}", reason)
            }
            Some(installer::InstallStatus::NotInstalled) | None => "not installed".to_string(),
        };

        if json {
            results.push(serde_json::json!({
                "id": info.id,
                "name": info.name,
                "description": info.description,
                "languages": info.languages,
                "platforms": info.platforms.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
                "primary": info.primary,
                "status": status_str,
                "path": status.as_ref().and_then(|s| match s {
                    installer::InstallStatus::Installed { path, .. } => Some(path.display().to_string()),
                    installer::InstallStatus::Broken { path, .. } => Some(path.display().to_string()),
                    _ => None,
                }),
            }));
        } else {
            let status_indicator = match &status {
                Some(installer::InstallStatus::Installed { .. }) => "✓",
                Some(installer::InstallStatus::Broken { .. }) => "✗",
                _ => " ",
            };
            println!(
                "  {} {:12} {:20} {}",
                status_indicator,
                info.id,
                status_str,
                info.languages.join(", ")
            );
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }

    Ok(())
}

/// Check all installed debuggers
async fn check_debuggers(json: bool) -> Result<()> {
    let debuggers = registry::all_debuggers();
    let mut results = Vec::new();
    let mut found_any = false;

    if !json {
        println!("Checking installed debuggers...\n");
    }

    for info in debuggers {
        let installer = match registry::get_installer(info.id) {
            Some(i) => i,
            None => continue,
        };

        let status = installer.status().await.ok();

        if let Some(installer::InstallStatus::Installed { path, version }) = &status {
            found_any = true;

            // Verify the installation
            let verify_result = installer.verify().await;
            let working = verify_result.as_ref().map(|v| v.success).unwrap_or(false);

            if json {
                results.push(serde_json::json!({
                    "id": info.id,
                    "path": path.display().to_string(),
                    "version": version,
                    "working": working,
                    "error": verify_result.as_ref().ok().and_then(|v| v.error.clone()),
                }));
            } else {
                let status_icon = if working { "✓" } else { "✗" };
                println!("{} {}", status_icon, info.id);
                println!("  Path: {}", path.display());
                if let Some(v) = version {
                    println!("  Version: {}", v);
                }
                if !working {
                    if let Ok(v) = &verify_result {
                        if let Some(err) = &v.error {
                            println!("  Error: {}", err);
                        }
                    }
                }
                println!();
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else if !found_any {
        println!("No debuggers installed.");
        println!("Use 'debugger setup --list' to see available debuggers.");
    }

    Ok(())
}

/// Auto-detect project types and install appropriate debuggers
async fn auto_setup(opts: SetupOptions) -> Result<()> {
    let project_types = detector::detect_project_types(std::env::current_dir()?.as_path());

    if project_types.is_empty() {
        if opts.json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "no_projects",
                    "message": "No recognized project types found in current directory."
                })
            );
        } else {
            println!("No recognized project types found in current directory.");
        }
        return Ok(());
    }

    let debuggers: Vec<&str> = project_types
        .iter()
        .flat_map(|pt| detector::debuggers_for_project(pt))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if !opts.json {
        println!(
            "Detected project types: {}",
            project_types
                .iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!(
            "Will install debuggers: {}",
            debuggers.join(", ")
        );
        println!();
    }

    let mut results = Vec::new();

    for debugger in debuggers {
        let result = install_debugger_inner(
            debugger,
            &SetupOptions {
                debugger: Some(debugger.to_string()),
                ..opts.clone()
            },
        )
        .await;

        if opts.json {
            results.push(result);
        }
    }

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }

    Ok(())
}

/// Show the installation path for a debugger
async fn show_path(debugger: &str, json: bool) -> Result<()> {
    let installer = match registry::get_installer(debugger) {
        Some(i) => i,
        None => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "not_found",
                        "debugger": debugger,
                        "message": format!("Unknown debugger: {}", debugger)
                    })
                );
            } else {
                println!("Unknown debugger: {}", debugger);
            }
            return Ok(());
        }
    };

    let status = installer.status().await?;

    match status {
        installer::InstallStatus::Installed { path, version } => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "installed",
                        "debugger": debugger,
                        "path": path.display().to_string(),
                        "version": version,
                    })
                );
            } else {
                println!("{}", path.display());
            }
        }
        installer::InstallStatus::Broken { path, reason } => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "broken",
                        "debugger": debugger,
                        "path": path.display().to_string(),
                        "reason": reason,
                    })
                );
            } else {
                println!("{} (broken: {})", path.display(), reason);
            }
        }
        installer::InstallStatus::NotInstalled => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "not_installed",
                        "debugger": debugger,
                    })
                );
            } else {
                println!("{} is not installed", debugger);
            }
        }
    }

    Ok(())
}

/// Uninstall a debugger
async fn uninstall_debugger(debugger: &str, json: bool) -> Result<()> {
    let installer = match registry::get_installer(debugger) {
        Some(i) => i,
        None => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "not_found",
                        "debugger": debugger,
                        "message": format!("Unknown debugger: {}", debugger)
                    })
                );
            } else {
                println!("Unknown debugger: {}", debugger);
            }
            return Ok(());
        }
    };

    match installer.uninstall().await {
        Ok(()) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "uninstalled",
                        "debugger": debugger,
                    })
                );
            } else {
                println!("{} uninstalled", debugger);
            }
        }
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "debugger": debugger,
                        "message": e.to_string(),
                    })
                );
            } else {
                println!("Failed to uninstall {}: {}", debugger, e);
            }
        }
    }

    Ok(())
}

/// Install a debugger
async fn install_debugger(debugger: &str, opts: SetupOptions) -> Result<()> {
    let result = install_debugger_inner(debugger, &opts).await;

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    }

    Ok(())
}

/// Inner installation logic that returns a result struct
async fn install_debugger_inner(debugger: &str, opts: &SetupOptions) -> SetupResult {
    let installer = match registry::get_installer(debugger) {
        Some(i) => i,
        None => {
            return SetupResult {
                status: SetupStatus::NotFound,
                debugger: debugger.to_string(),
                version: None,
                path: None,
                languages: None,
                message: Some(format!("Unknown debugger: {}", debugger)),
            };
        }
    };

    // Check current status
    let status = match installer.status().await {
        Ok(s) => s,
        Err(e) => {
            return SetupResult {
                status: SetupStatus::Failed,
                debugger: debugger.to_string(),
                version: None,
                path: None,
                languages: None,
                message: Some(format!("Failed to check status: {}", e)),
            };
        }
    };

    // Already installed?
    if let installer::InstallStatus::Installed { path, version } = &status {
        if !opts.force {
            if !opts.json {
                println!(
                    "{} is already installed at {}",
                    debugger,
                    path.display()
                );
                if let Some(v) = version {
                    println!("Version: {}", v);
                }
                println!("Use --force to reinstall.");
            }
            return SetupResult {
                status: SetupStatus::AlreadyInstalled,
                debugger: debugger.to_string(),
                version: version.clone(),
                path: Some(path.clone()),
                languages: Some(
                    installer
                        .info()
                        .languages
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ),
                message: None,
            };
        }
    }

    // Dry run?
    if opts.dry_run {
        let method = installer.best_method().await;
        if !opts.json {
            println!("Would install {} using:", debugger);
            match &method {
                Ok(m) => println!("  Method: {:?}", m),
                Err(e) => println!("  Error determining method: {}", e),
            }
        }
        return SetupResult {
            status: SetupStatus::DryRun,
            debugger: debugger.to_string(),
            version: opts.version.clone(),
            path: None,
            languages: Some(
                installer
                    .info()
                    .languages
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            message: Some(format!("Method: {:?}", method)),
        };
    }

    // Install
    if !opts.json {
        println!("Installing {}...", debugger);
    }

    let install_opts = installer::InstallOptions {
        version: opts.version.clone(),
        force: opts.force,
    };

    match installer.install(install_opts).await {
        Ok(result) => {
            // Update configuration
            if let Err(e) = update_config(debugger, &result.path, &result.args).await {
                if !opts.json {
                    println!("Warning: Failed to update configuration: {}", e);
                }
            }

            if !opts.json {
                println!();
                println!(
                    "✓ {} {} installed to {}",
                    installer.info().name,
                    result.version.as_deref().unwrap_or(""),
                    result.path.display()
                );
                println!();
                println!(
                    "Configuration updated. Use 'debugger start --adapter {} ./program' to debug.",
                    debugger
                );
            }

            SetupResult {
                status: SetupStatus::Success,
                debugger: debugger.to_string(),
                version: result.version,
                path: Some(result.path),
                languages: Some(
                    installer
                        .info()
                        .languages
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ),
                message: None,
            }
        }
        Err(e) => {
            if !opts.json {
                println!("✗ Failed to install {}: {}", debugger, e);
            }
            SetupResult {
                status: SetupStatus::Failed,
                debugger: debugger.to_string(),
                version: None,
                path: None,
                languages: None,
                message: Some(e.to_string()),
            }
        }
    }
}

/// Update the configuration file with the installed adapter
async fn update_config(debugger: &str, path: &std::path::Path, args: &[String]) -> Result<()> {
    use crate::common::paths::{config_path, ensure_config_dir};
    use std::io::Write;

    ensure_config_dir()?;

    let config_file = match config_path() {
        Some(p) => p,
        None => return Ok(()),
    };

    // Read existing config or create new
    let mut content = if config_file.exists() {
        std::fs::read_to_string(&config_file)?
    } else {
        String::new()
    };

    // Parse and update
    let mut config: toml::Table = if content.is_empty() {
        toml::Table::new()
    } else {
        content.parse().map_err(|e| {
            crate::common::Error::ConfigParse(format!(
                "Failed to parse {}: {}",
                config_file.display(),
                e
            ))
        })?
    };

    // Ensure adapters section exists
    if !config.contains_key("adapters") {
        config.insert("adapters".to_string(), toml::Value::Table(toml::Table::new()));
    }

    let adapters = config
        .get_mut("adapters")
        .and_then(|v| v.as_table_mut())
        .expect("Expected 'adapters' to be a TOML table");

    // Create adapter entry
    let mut adapter_table = toml::Table::new();
    adapter_table.insert(
        "path".to_string(),
        toml::Value::String(path.display().to_string()),
    );
    if !args.is_empty() {
        adapter_table.insert(
            "args".to_string(),
            toml::Value::Array(args.iter().map(|s| toml::Value::String(s.clone())).collect()),
        );
    }

    adapters.insert(debugger.to_string(), toml::Value::Table(adapter_table));

    // Write back
    content = toml::to_string_pretty(&config).unwrap_or_default();
    let mut file = std::fs::File::create(&config_file)?;
    file.write_all(content.as_bytes())?;

    Ok(())
}
