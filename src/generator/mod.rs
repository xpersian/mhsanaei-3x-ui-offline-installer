pub mod script;
pub mod bundle;

use anyhow::Result;
use crate::wizard::state::{BuildConfig, OutputKind};

pub async fn build(config: &BuildConfig, resolved_version: &str) -> Result<()> {
    // 1. First render the install.sh into the bundle folder
    script::render(config, resolved_version)?;

    // 2. If SFX is requested, wrap the folder into a single .sh file
    if config.output_kind == OutputKind::Sfx {
        let sfx_path = format!("{}.sh", config.output_dir.trim_end_matches('/'));
        bundle::create_sfx(&config.output_dir, &sfx_path)?;
    }

    Ok(())
}
