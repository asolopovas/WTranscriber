#![allow(clippy::needless_pass_by_value)]

use tauri::{AppHandle, Emitter};

use crate::{
    error::{Error, Result},
    logfile, models,
    models::{FileProgress, ModelInfo, ModelStatus},
};

#[tauri::command]
pub fn list_models() -> Result<Vec<ModelInfo>> {
    models::manager().list()
}

#[tauri::command]
pub fn essential_models() -> Vec<String> {
    crate::essential_model_ids()
}

#[tauri::command]
pub fn start_essentials(app: AppHandle) {
    crate::auto_install_essentials(app);
}

#[tauri::command]
pub fn model_status(id: String) -> Result<ModelStatus> {
    models::manager().status(&id)
}

#[tauri::command]
pub async fn install_model(app: AppHandle, id: String) -> Result<()> {
    let mut on_progress = |p: FileProgress| {
        let _ = app.emit("model:progress", &p);
    };
    logfile::info(&format!("install_model {id} starting"));
    let result = models::manager().install(&id, &mut on_progress).await;
    match &result {
        Ok(()) => logfile::info(&format!("install_model {id} ok")),
        Err(e) => logfile::error(&format!("install_model {id}: {e}")),
    }
    let _ = app.emit(
        if result.is_ok() {
            "model:done"
        } else {
            "model:error"
        },
        &id,
    );
    result
}

#[tauri::command]
pub fn delete_model(id: String) -> Result<()> {
    let Some(entry) = models::by_id(&id) else {
        return Err(Error::Config(format!("unknown model id {id}")));
    };
    for p in models::paths_for(entry)? {
        if p.exists() {
            std::fs::remove_file(&p).ok();
        }
    }
    let dir = models::model_dir(&id)?;
    if dir.exists() && std::fs::read_dir(&dir).is_ok_and(|r| r.count() == 0) {
        std::fs::remove_dir(&dir).ok();
    }
    logfile::info(&format!("delete_model {id} ok"));
    Ok(())
}
