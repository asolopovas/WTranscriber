use crate::{config::Config, error::Result, models};

#[tauri::command]
pub fn load_config() -> Result<Config> {
    Config::load()
}

#[tauri::command]
pub fn save_config(mut config: Config) -> Result<()> {
    sync_engine(&mut config);
    config.save()
}

pub(super) fn sync_engine(config: &mut Config) {
    if let Some(model) = models::by_id(&config.model)
        && let Some(engine) = model.engine_kind()
    {
        config.engine = engine;
    }
}
