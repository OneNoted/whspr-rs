use dialoguer::Select;

use crate::config::{self, default_config_path};
use crate::error::Result;
use crate::model::{self, MODELS};

pub async fn run_setup() -> Result<()> {
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

    // Download the model
    let dest = model::download_model(chosen.name).await?;
    println!();

    // Generate or update config
    let config_path = default_config_path();
    let model_path_str = format!("~/.local/share/whspr-rs/{}", chosen.filename);

    if config_path.exists() {
        println!("Config already exists at {}", config_path.display());
        config::update_config_model_path(&config_path, &model_path_str)?;
        println!("Updated model_path to: {}", model_path_str);
    } else {
        config::write_default_config(&config_path, &model_path_str)?;
        println!("Config written to {}", config_path.display());
    }

    println!();
    println!("Setup complete! You can now use whspr-rs.");
    println!("Bind it to a key in your compositor, e.g. for Hyprland:");
    println!("  bind = SUPER ALT, D, exec, whspr-rs");

    let _ = dest;
    Ok(())
}
