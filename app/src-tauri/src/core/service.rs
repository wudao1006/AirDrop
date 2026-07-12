use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

const SNAPSHOT_EVENT: &str = "airdrop://snapshot";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardCapability {
    can_read_text: bool,
    can_write_text: bool,
    foreground_capture: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    limitation: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardRepresentation {
    id: String,
    kind: String,
    label: String,
    mime: String,
    size: u64,
    status: String,
    enabled: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSlot {
    id: String,
    revision: u64,
    device_id: String,
    device_name: String,
    platform: String,
    online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pinned: Option<bool>,
    availability: String,
    preview: String,
    captured_at: String,
    age_label: String,
    groups: Vec<String>,
    sequence: u64,
    size: u64,
    representations: Vec<ClipboardRepresentation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentClipboard {
    source: String,
    source_label: String,
    preview: String,
    types: Vec<String>,
    changed_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOperation {
    id: String,
    slot_id: String,
    device_name: String,
    source_summary: String,
    status: String,
    progress: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    theme: String,
    accent_color: String,
    window_opacity: f64,
    blur_strength: u8,
    glass_saturation: f64,
    corner_radius: u8,
    highlight_strength: f64,
    floating_orb_enabled: bool,
    preview_text: bool,
    preview_images: bool,
    preview_file_names: bool,
    allow_text: bool,
    allow_html: bool,
    allow_images: bool,
    allow_urls: bool,
    allow_files: bool,
    allow_private: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            accent_color: "#168fae".into(),
            window_opacity: 0.94,
            blur_strength: 30,
            glass_saturation: 1.3,
            corner_radius: 22,
            highlight_strength: 0.28,
            floating_orb_enabled: false,
            preview_text: true,
            preview_images: false,
            preview_file_names: false,
            allow_text: true,
            allow_html: true,
            allow_images: true,
            allow_urls: true,
            allow_files: false,
            allow_private: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSnapshot {
    revision: u64,
    platform: String,
    activity: String,
    last_synchronized_at: String,
    clipboard_capability: ClipboardCapability,
    demo_mode: bool,
    daemon_connected: bool,
    publish_paused: bool,
    subscribe_paused: bool,
    current_clipboard: CurrentClipboard,
    last_published_preview: String,
    slots: Vec<DeviceSlot>,
    imports: Vec<ImportOperation>,
    settings: AppSettings,
}

impl UiSnapshot {
    fn initial() -> Self {
        Self {
            revision: 1,
            platform: "desktop".into(),
            activity: "foreground_live".into(),
            last_synchronized_at: "1970-01-01T00:00:00.000Z".into(),
            clipboard_capability: ClipboardCapability {
                can_read_text: true,
                can_write_text: true,
                foreground_capture: true,
                limitation: None,
            },
            demo_mode: false,
            daemon_connected: true,
            publish_paused: false,
            subscribe_paused: false,
            current_clipboard: CurrentClipboard {
                source: "unknown".into(),
                source_label: "正在监听本机剪贴板".into(),
                preview: "复制文本后会自动显示在这里。".into(),
                types: Vec::new(),
                changed_at: "1970-01-01T00:00:00.000Z".into(),
            },
            last_published_preview: "等待本机剪贴板变化".into(),
            slots: Vec::new(),
            imports: Vec::new(),
            settings: AppSettings::default(),
        }
    }

    fn bump(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

pub struct ServiceState(Mutex<UiSnapshot>);

impl Default for ServiceState {
    fn default() -> Self {
        Self(Mutex::new(UiSnapshot::initial()))
    }
}

fn emit_snapshot(app: &AppHandle, snapshot: &UiSnapshot) -> Result<(), String> {
    app.emit(SNAPSHOT_EVENT, snapshot.clone())
        .map_err(|error| error.to_string())
}

fn update<F>(state: &ServiceState, app: &AppHandle, operation: F) -> Result<(), String>
where
    F: FnOnce(&mut UiSnapshot) -> Result<(), String>,
{
    let snapshot = {
        let mut snapshot = state
            .0
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        operation(&mut snapshot)?;
        snapshot.bump();
        snapshot.clone()
    };
    emit_snapshot(app, &snapshot)
}

pub fn capture_local_clipboard(
    state: &ServiceState,
    app: &AppHandle,
    text: String,
    now: String,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    update(state, app, |snapshot| {
        snapshot.current_clipboard = CurrentClipboard {
            source: "local".into(),
            source_label: "来自本机系统剪贴板".into(),
            preview: text.clone(),
            types: vec!["纯文本".into()],
            changed_at: now.clone(),
        };
        if !snapshot.publish_paused {
            let preview: String = text.chars().take(80).collect();
            snapshot.last_published_preview = format!("本机最近捕获：{preview}");
        }
        snapshot.last_synchronized_at = now;
        snapshot.clipboard_capability.can_read_text = true;
        snapshot.clipboard_capability.foreground_capture = true;
        snapshot.clipboard_capability.limitation = None;
        Ok(())
    })
}

#[tauri::command]
pub fn get_snapshot(
    state: State<'_, ServiceState>,
    platform: String,
    now: String,
) -> Result<UiSnapshot, String> {
    let mut snapshot = state
        .0
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
    snapshot.platform = if platform == "android" {
        "android".into()
    } else {
        "desktop".into()
    };
    if snapshot.last_synchronized_at.starts_with("1970-") {
        snapshot.last_synchronized_at = now.clone();
        snapshot.current_clipboard.changed_at = now;
    }
    Ok(snapshot.clone())
}

#[tauri::command]
pub fn set_pause(
    state: State<'_, ServiceState>,
    app: AppHandle,
    kind: String,
    paused: bool,
) -> Result<(), String> {
    update(&state, &app, |snapshot| match kind.as_str() {
        "publish" => {
            snapshot.publish_paused = paused;
            Ok(())
        }
        "subscribe" => {
            snapshot.subscribe_paused = paused;
            Ok(())
        }
        _ => Err("未知暂停类型".into()),
    })
}

#[tauri::command]
pub fn set_synchronization_paused(
    state: State<'_, ServiceState>,
    app: AppHandle,
    paused: bool,
) -> Result<(), String> {
    update(&state, &app, |snapshot| {
        snapshot.publish_paused = paused;
        snapshot.subscribe_paused = paused;
        Ok(())
    })
}

#[tauri::command]
pub fn set_app_activity(
    state: State<'_, ServiceState>,
    app: AppHandle,
    activity: String,
    now: String,
) -> Result<(), String> {
    update(&state, &app, |snapshot| {
        if snapshot.platform != "android" {
            return Ok(());
        }
        snapshot.activity = match activity.as_str() {
            "background" => "suspended",
            "foreground" => "foreground_live",
            _ => return Err("未知应用生命周期状态".into()),
        }
        .into();
        if activity == "foreground" {
            snapshot.last_synchronized_at = now;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn publish_local_clipboard(
    state: State<'_, ServiceState>,
    app: AppHandle,
    text: String,
    now: String,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("当前文本剪贴板为空".into());
    }
    capture_local_clipboard(&state, &app, text, now)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, ServiceState>,
    app: AppHandle,
    settings: Value,
) -> Result<(), String> {
    update(&state, &app, |snapshot| {
        let mut current =
            serde_json::to_value(&snapshot.settings).map_err(|error| error.to_string())?;
        let current_object = current
            .as_object_mut()
            .ok_or_else(|| "设置状态格式错误".to_string())?;
        let patch = settings
            .as_object()
            .ok_or_else(|| "设置更新必须是对象".to_string())?;
        for (key, value) in patch {
            current_object.insert(key.clone(), value.clone());
        }
        snapshot.settings =
            serde_json::from_value(current).map_err(|error| format!("设置值无效：{error}"))?;
        Ok(())
    })
}

#[tauri::command]
pub fn create_import_intent(
    state: State<'_, ServiceState>,
    slot_id: String,
    revision: u64,
) -> Result<String, String> {
    let snapshot = state
        .0
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
    let slot = snapshot
        .slots
        .iter()
        .find(|slot| slot.id == slot_id && slot.revision == revision)
        .ok_or_else(|| "设备槽位不存在或已经更新".to_string())?;
    Err(format!("{} 的远端正文传输尚未就绪", slot.device_name))
}

#[tauri::command]
pub fn confirm_import(
    _state: State<'_, ServiceState>,
    _import_id: String,
) -> Result<String, String> {
    Err("没有可确认的远端剪贴板导入".into())
}

#[tauri::command]
pub fn cancel_import(
    state: State<'_, ServiceState>,
    app: AppHandle,
    import_id: String,
) -> Result<(), String> {
    update(&state, &app, |snapshot| {
        snapshot
            .imports
            .retain(|operation| operation.id != import_id);
        Ok(())
    })
}
