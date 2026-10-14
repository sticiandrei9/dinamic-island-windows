use std::sync::mpsc::{channel, Sender};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::implement;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationEventHandler, IUIAutomationEventHandler_Impl,
    TreeScope_Descendants, TreeScope_Subtree, UIA_EVENT_ID, UIA_TextControlTypeId, UIA_Window_WindowOpenedEventId,
};

use crate::settings::SettingsState;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Toast {
    pub app: String,
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
}

/// Texte de sistem din fereastra toast-ului (etichete, butoane), în limbile probabile.
const SYSTEM_TEXTS: &[&str] = &[
    "notificare nouă", "new notification", "новое уведомление",
    "închideți", "închide", "close", "dismiss", "закрыть",
    "setări", "settings", "параметры", "notification settings",
];

/// Ordinea textelor dintr-un toast: aplicația, titlul, apoi corpul (unul sau mai multe rânduri).
pub fn parse_toast(texts: &[String]) -> Option<Toast> {
    let mut clean: Vec<String> = Vec::new();
    for t in texts.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if SYSTEM_TEXTS.contains(&t.to_lowercase().as_str()) || clean.last().is_some_and(|l| l == t) {
            continue;
        }
        clean.push(t.to_string());
    }
    let toast = |app: &str, title: &str, body: &[String]| Toast { app: app.into(), title: title.into(), body: body.join(" · "), icon: None };
    match clean.as_slice() {
        [] => None,
        [title] => Some(toast("", title, &[])),
        [app, title] => Some(toast(app, title, &[])),
        [app, title, body @ ..] => Some(toast(app, title, body)),
    }
}

/// Toast-urile sunt ferestre ale lui ShellExperienceHost: `Windows.UI.Core.CoreWindow` pe Windows 10,
/// `FlexibleToastView` pe Windows 11.
pub fn is_toast_window(class: &str, exe: &str) -> bool {
    matches!(class, "Windows.UI.Core.CoreWindow" | "FlexibleToastView") && exe.eq_ignore_ascii_case("ShellExperienceHost.exe")
}

#[implement(IUIAutomationEventHandler)]
struct WindowOpened {
    automation: IUIAutomation,
    tx: Sender<Toast>,
}

impl IUIAutomationEventHandler_Impl for WindowOpened_Impl {
    fn HandleAutomationEvent(&self, sender: Option<&IUIAutomationElement>, _id: UIA_EVENT_ID) -> windows::core::Result<()> {
        let Some(el) = sender else { return Ok(()) };
        unsafe {
            let class = el.CurrentClassName().map(|s| s.to_string()).unwrap_or_default();
            let pid = el.CurrentProcessId().unwrap_or(0) as u32;
            let exe = crate::windows_list::exe_name(pid);
            if !is_toast_window(&class, &exe) {
                log::debug!("UIA: fereastră deschisă {class} ({exe})");
                return Ok(());
            }
            let all = el.FindAll(TreeScope_Descendants, &self.automation.CreateTrueCondition()?)?;
            let mut texts = Vec::new();
            for i in 0..all.Length()? {
                let item = all.GetElement(i)?;
                if item.CurrentControlType()? == UIA_TextControlTypeId {
                    texts.push(item.CurrentName().map(|s| s.to_string()).unwrap_or_default());
                }
            }
            if let Some(t) = parse_toast(&texts) {
                let _ = self.tx.send(t);
            }
        }
        Ok(())
    }
}

/// Ascultă deschiderea ferestrelor (UI Automation) și emite `notification` pentru fiecare toast.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let automation: IUIAutomation = match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
            Ok(a) => a,
            Err(e) => {
                log::warn!("notificări: UI Automation indisponibil, modulul e oprit: {e}");
                return;
            }
        };
        let (tx, rx) = channel::<Toast>();
        let mut registered = false;
        loop {
            let enabled = app.state::<SettingsState>().get().modules.notifications;
            if enabled && !registered {
                let handler: IUIAutomationEventHandler = WindowOpened { automation: automation.clone(), tx: tx.clone() }.into();
                let ok = unsafe {
                    automation.GetRootElement().and_then(|root| {
                        automation.AddAutomationEventHandler(UIA_Window_WindowOpenedEventId, &root, TreeScope_Subtree, None, &handler)
                    })
                };
                match ok {
                    Ok(()) => registered = true,
                    Err(e) => {
                        log::warn!("notificări: nu pot asculta ferestrele, modulul e oprit: {e}");
                        return;
                    }
                }
            } else if !enabled && registered {
                unsafe {
                    let _ = automation.RemoveAllEventHandlers();
                }
                registered = false;
            }
            if let Ok(t) = rx.recv_timeout(Duration::from_millis(500)) {
                if enabled {
                    let _ = app.emit("notification", &t);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn toast_complet() {
        let t = parse_toast(&v(&["Discord", "Andrei", "Intri pe server diseară?"])).unwrap();
        assert_eq!((t.app.as_str(), t.title.as_str(), t.body.as_str()), ("Discord", "Andrei", "Intri pe server diseară?"));
    }

    #[test]
    fn corp_pe_mai_multe_randuri_si_texte_goale_sau_duplicate() {
        let t = parse_toast(&v(&["Outlook", "", "Ședință", "Ședință", "Azi la 15:00", "Sala 2"])).unwrap();
        assert_eq!(t.title, "Ședință");
        assert_eq!(t.body, "Azi la 15:00 · Sala 2");
    }

    #[test]
    fn fara_corp_sau_doar_titlu() {
        let t = parse_toast(&v(&["Teams", "Apel ratat"])).unwrap();
        assert_eq!((t.title.as_str(), t.body.as_str()), ("Apel ratat", ""));
        let t = parse_toast(&v(&["Doar un text"])).unwrap();
        assert_eq!((t.app.as_str(), t.title.as_str()), ("", "Doar un text"));
        assert!(parse_toast(&v(&["", "  "])).is_none());
    }

    #[test]
    fn textele_de_sistem_ale_toastului_sunt_ignorate() {
        // butoanele și eticheta „Notificare nouă” nu sunt conținut
        let t = parse_toast(&v(&["Notificare nouă", "Discord", "Andrei", "Salut", "Închideți"])).unwrap();
        assert_eq!((t.app.as_str(), t.title.as_str(), t.body.as_str()), ("Discord", "Andrei", "Salut"));
    }

    #[test]
    fn fereastra_de_toast() {
        assert!(is_toast_window("Windows.UI.Core.CoreWindow", "ShellExperienceHost.exe")); // Windows 10
        assert!(is_toast_window("FlexibleToastView", "ShellExperienceHost.exe")); // Windows 11
        assert!(!is_toast_window("Windows.UI.Core.CoreWindow", "ApplicationFrameHost.exe"));
        assert!(!is_toast_window("Chrome_WidgetWin_1", "ShellExperienceHost.exe"));
    }
}
