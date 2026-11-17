use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Emitter;

use crate::{config, logfile, models, runtime_install};

#[must_use]
pub fn essential_model_ids() -> Vec<String> {
    let cfg = config::Config::load().unwrap_or_default();
    let configured = [
        Some(cfg.model.as_str()),
        cfg.diarize.then(|| cfg.diarizer.as_str()),
    ];
    let mut ids: Vec<String> = [
        models::Family::Asr,
        models::Family::Diarizer,
        models::Family::Llm,
        models::Family::LangId,
    ]
    .iter()
    .filter_map(|f| models::default_id(*f))
    .chain(configured.into_iter().flatten())
    .filter(|id| models::by_id(id).is_some())
    .map(String::from)
    .collect();
    ids.sort();
    ids.dedup();
    ids
}

static ESSENTIALS_STARTED: AtomicBool = AtomicBool::new(false);

pub fn auto_install_essentials(app: tauri::AppHandle) {
    if ESSENTIALS_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        runtime_install::ensure_runtimes(&app).await;
        let manager = models::manager();
        let pending: Vec<String> = essential_model_ids()
            .into_iter()
            .filter(|id| matches!(manager.status(id), Ok(models::ModelStatus::NotInstalled)))
            .collect();
        if pending.is_empty() {
            let _ = app.emit("model:essentials_done", true);
            return;
        }
        let mut handles = Vec::with_capacity(pending.len());
        for id in pending {
            let app = app.clone();
            handles.push(tauri::async_runtime::spawn(async move {
                logfile::info(&format!("auto_install {id} starting"));
                let app_for_cb = app.clone();
                let mut on_progress = move |p: models::FileProgress| {
                    let _ = app_for_cb.emit("model:progress", &p);
                };
                let manager = models::manager();
                match manager.install(&id, &mut on_progress).await {
                    Ok(()) => {
                        logfile::info(&format!("auto_install {id} ok"));
                        crate::android_backup_model(&id);
                        let _ = app.emit("model:done", &id);
                        true
                    }
                    Err(e) => {
                        logfile::error(&format!("auto_install {id}: {e}"));
                        let _ = app.emit("model:error", &id);
                        false
                    }
                }
            }));
        }
        let mut all_ok = true;
        for h in handles {
            match h.await {
                Ok(true) => {}
                _ => all_ok = false,
            }
        }
        #[cfg(target_os = "android")]
        crate::android::maybe_backup_after_install();
        let _ = app.emit("model:essentials_done", all_ok);
    });
}
