//! Mod demo pentru verificări vizuale (`DI_DEMO=1`): emite evenimente de test,
//! ca alertele, volumul și notificările să poată fi văzute fără hardware sau aplicații reale.
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Emitter};

pub fn spawn(app: AppHandle) {
    if std::env::var_os("DI_DEMO").is_none() {
        return;
    }
    std::thread::spawn(move || {
        let steps: Vec<(u64, &str, serde_json::Value)> = vec![
            (5, "alert", json!({"icon": "headphones", "title": "AirPods Pro 3", "detail": "Conectat · 80%", "color": "var(--fg)", "level": 0.8})),
            (9, "alert", json!({"icon": "bolt", "title": "Se încarcă", "detail": "64%", "color": "var(--green)", "level": 0.64})),
            (13, "volume", json!({"source": "volume", "level": 0.45, "muted": false})),
            (13, "volume", json!({"source": "volume", "level": 0.55, "muted": false})),
            (17, "notification", json!({"app": "Discord", "title": "Andrei", "body": "Intri pe server diseară? Avem nevoie de încă un om la echipă", "icon": null})),
            (23, "privacy", json!({"mic": ["Discord"], "cam": []})),
        ];
        let mut t = 0;
        for (at, event, payload) in steps {
            std::thread::sleep(Duration::from_secs(at - t));
            t = at;
            let _ = app.emit(event, payload);
        }
    });
}
