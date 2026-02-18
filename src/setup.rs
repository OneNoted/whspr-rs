use std::path::Path;

use dialoguer::Select;

use crate::config::{self, resolve_config_path};
use crate::error::Result;
use crate::model::{self, MODELS};

pub async fn run_setup(config_path_override: Option<&Path>) -> Result<()> {
    println!("whspr-rs setup");
    println!();

    // Build selection items
    let items: Vec<String> = MODELS
        .iter()
        .map(|m| format!("{:<22} {:>8}  {}", m.name, m.size, m.description))
        .collect();

    let selection = Select::new()
        .with_prompt("Choose a whisper model to download")
        .items(&items)
        .default(0) // large-v3-turbo
        .interact()
        .map_err(|e| crate::error::WhsprError::Config(format!("selection cancelled: {e}")))?;

    let chosen = &MODELS[selection];
    println!();
    tracing::info!("setup selected model: {}", chosen.name);

    // Download the model
    model::download_model(chosen.name).await?;
    println!();

    // Generate or update config
    let config_path = resolve_config_path(config_path_override);
    let model_path_str = model::model_path_for_config(chosen.filename);

    if config_path.exists() {
        println!("Config already exists at {}", config_path.display());
        tracing::info!("updating existing config at {}", config_path.display());
        config::update_config_model_path(&config_path, &model_path_str)?;
        println!("Updated model_path to: {}", model_path_str);
    } else {
        tracing::info!("writing new config at {}", config_path.display());
        config::write_default_config(&config_path, &model_path_str)?;
        println!("Config written to {}", config_path.display());
    }

    println!();
    println!("Setup complete! You can now use whspr-rs.");
    println!("Bind it to a key in your compositor, e.g. for Hyprland:");
    println!("  bind = SUPER ALT, D, exec, whspr-rs");

    Ok(())
}
