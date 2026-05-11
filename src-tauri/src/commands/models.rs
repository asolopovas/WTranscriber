#![allow(clippy::needless_pass_by_value)]

use tauri::{AppHandle, Emitter};

use crate::{
    error::Result,
    logfile, models,
    models::{FileProgress, ModelInfo},
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
pub async fn install_model(app: AppHandle, id: String) -> Result<()> {
    let mut on_progress = |p: FileProgress| {
        let _ = app.emit("model:progress", &p);
    };
    logfile::info(&format!("install_model {id} starting"));
    let result = models::manager().install(&id, &mut on_progress).await;
    match &result {
        Ok(()) => {
            logfile::info(&format!("install_model {id} ok"));
            crate::android_mirror_after_install();
        }
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
