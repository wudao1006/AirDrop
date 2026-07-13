use super::{
    cache::{CachedText, ClipboardCache},
    identity::Identity,
    storage::{CachedSlotMetadata, Store, StoredRuntime, TrustedDevice},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path, sync::Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

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
pub struct NearbyDevice {
    pub(crate) instance_id: String,
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) platform: String,
    pub(crate) addresses: Vec<String>,
    pub(crate) port: u16,
    pub(crate) last_seen_at: String,
    pub(crate) paired: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedDeviceView {
    device_id: String,
    device_name: String,
    platform: String,
    paired_at: String,
    online: bool,
    sync_enabled: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPairing {
    pub(crate) pairing_id: String,
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) platform: String,
    pub(crate) sas: String,
    pub(crate) direction: String,
    pub(crate) expires_at: String,
    pub(crate) status: String,
}

#[derive(Clone)]
enum RemoteClipboardBody {
    Text(String),
    Image {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
}

pub(crate) struct RemoteImage {
    pub(crate) sequence: u64,
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) captured_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub(crate) theme: String,
    pub(crate) accent_color: String,
    pub(crate) window_opacity: f64,
    pub(crate) blur_strength: u8,
    pub(crate) glass_saturation: f64,
    pub(crate) corner_radius: u8,
    pub(crate) highlight_strength: f64,
    pub(crate) floating_orb_enabled: bool,
    pub(crate) preview_text: bool,
    pub(crate) preview_images: bool,
    pub(crate) preview_file_names: bool,
    pub(crate) allow_text: bool,
    pub(crate) allow_html: bool,
    pub(crate) allow_images: bool,
    pub(crate) allow_urls: bool,
    pub(crate) allow_files: bool,
    pub(crate) allow_private: bool,
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
    nearby_devices: Vec<NearbyDevice>,
    trusted_devices: Vec<TrustedDeviceView>,
    pending_pairings: Vec<PendingPairing>,
    cache_persistent: bool,
    imports: Vec<ImportOperation>,
    settings: AppSettings,
}

impl UiSnapshot {
    fn initial(
        settings: AppSettings,
        runtime: Option<StoredRuntime>,
        trusted_devices: Vec<TrustedDevice>,
    ) -> Self {
        let runtime = runtime.unwrap_or(StoredRuntime {
            publish_paused: false,
            subscribe_paused: false,
        });
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
            publish_paused: runtime.publish_paused,
            subscribe_paused: runtime.subscribe_paused,
            current_clipboard: CurrentClipboard {
                source: "unknown".into(),
                source_label: "正在监听本机剪贴板".into(),
                preview: "复制文本后会自动显示在这里。".into(),
                types: Vec::new(),
                changed_at: "1970-01-01T00:00:00.000Z".into(),
            },
            last_published_preview: "等待本机剪贴板变化".into(),
            slots: Vec::new(),
            nearby_devices: Vec::new(),
            trusted_devices: trusted_devices
                .into_iter()
                .map(|device| TrustedDeviceView {
                    device_id: device.device_id,
                    device_name: device.device_name,
                    platform: device.platform,
                    paired_at: device.paired_at,
                    online: false,
                    sync_enabled: device.sync_enabled,
                })
                .collect(),
            pending_pairings: Vec::new(),
            cache_persistent: false,
            imports: Vec::new(),
            settings,
        }
    }

    fn bump(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

pub struct ServiceState {
    snapshot: Mutex<UiSnapshot>,
    store: Store,
    identity: Identity,
    remote_bodies: Mutex<HashMap<String, RemoteClipboardBody>>,
    suppress_next_capture: Mutex<Option<String>>,
    suppress_next_image: Mutex<Option<[u8; 32]>>,
    clipboard_cache: ClipboardCache,
}

impl ServiceState {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        let store = Store::open(data_dir)?;
        let identity = Identity::load_or_create(data_dir)?;
        let settings = store.load_settings()?.unwrap_or_default();
        let runtime = store.load_runtime()?;
        let trusted_devices = store.trusted_devices()?;
        let clipboard_cache = ClipboardCache::open(data_dir);
        let mut snapshot = UiSnapshot::initial(settings, runtime, trusted_devices.clone());
        snapshot.cache_persistent = clipboard_cache.available();
        let mut remote_bodies = HashMap::new();
        if clipboard_cache.available() {
            let cached_slots = store.cached_slots(unix_seconds())?;
            clipboard_cache.prune_except(
                &cached_slots
                    .iter()
                    .map(|metadata| metadata.object_name.clone())
                    .collect(),
            );
            for metadata in cached_slots {
                let Some(device) = trusted_devices
                    .iter()
                    .find(|device| device.device_id == metadata.device_id && device.sync_enabled)
                else {
                    continue;
                };
                match clipboard_cache.load(&metadata.device_id, &metadata.object_name) {
                    Ok(cached) if cached.sequence == metadata.sequence => {
                        let slot = text_slot(
                            device,
                            cached.sequence,
                            &cached.text,
                            cached.captured_at,
                            false,
                            "stale",
                        );
                        remote_bodies
                            .insert(slot.id.clone(), RemoteClipboardBody::Text(cached.text));
                        snapshot.slots.push(slot);
                    }
                    Ok(_) => {
                        tracing::warn!(device_id = %metadata.device_id, "cached clipboard sequence mismatch")
                    }
                    Err(error) => {
                        tracing::warn!(device_id = %metadata.device_id, error = %error, "cached clipboard rejected")
                    }
                }
            }
        }
        Ok(Self {
            snapshot: Mutex::new(snapshot),
            store,
            identity,
            remote_bodies: Mutex::new(remote_bodies),
            suppress_next_capture: Mutex::new(None),
            suppress_next_image: Mutex::new(None),
            clipboard_cache,
        })
    }

    pub(crate) fn device_id(&self) -> &str {
        self.identity.device_id()
    }

    pub(crate) fn device_name(&self) -> &str {
        self.identity.device_name()
    }

    pub(crate) fn identity(&self) -> &Identity {
        &self.identity
    }

    pub(crate) fn trusted_device(&self, device_id: &str) -> Result<Option<TrustedDevice>, String> {
        self.store.trusted_device(device_id)
    }

    pub(crate) fn save_pending_pairing(
        &self,
        pairing_id: &str,
        device: &TrustedDevice,
        expires_at: &str,
    ) -> Result<(), String> {
        self.store
            .save_pending_pairing(pairing_id, device, expires_at)
    }

    pub(crate) fn promote_trusted_device(
        &self,
        pairing_id: &str,
        paired_at: &str,
    ) -> Result<TrustedDevice, String> {
        self.store.promote_trusted_device(pairing_id, paired_at)
    }

    pub(crate) fn nearby_device(&self, device_id: &str) -> Option<NearbyDevice> {
        self.snapshot.lock().ok().and_then(|snapshot| {
            snapshot
                .nearby_devices
                .iter()
                .find(|device| device.device_id == device_id)
                .cloned()
        })
    }

    pub(crate) fn enabled_device_ids(&self) -> Result<std::collections::HashSet<String>, String> {
        Ok(self
            .store
            .trusted_devices()?
            .into_iter()
            .filter(|device| device.sync_enabled)
            .map(|device| device.device_id)
            .collect())
    }
}

fn emit_snapshot(app: &AppHandle, snapshot: &UiSnapshot) -> Result<(), String> {
    app.emit(SNAPSHOT_EVENT, snapshot.clone())
        .map_err(|error| error.to_string())
}

fn update<F>(state: &ServiceState, app: &AppHandle, operation: F) -> Result<UiSnapshot, String>
where
    F: FnOnce(&mut UiSnapshot) -> Result<(), String>,
{
    let snapshot = {
        let mut snapshot = state
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        operation(&mut snapshot)?;
        snapshot.bump();
        snapshot.clone()
    };
    emit_snapshot(app, &snapshot)?;
    Ok(snapshot)
}

fn truncate_text(text: &str, maximum_chars: usize) -> String {
    let mut characters = text.chars();
    let preview: String = characters.by_ref().take(maximum_chars).collect();
    if characters.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn text_slot(
    device: &TrustedDevice,
    sequence: u64,
    text: &str,
    captured_at: String,
    online: bool,
    availability: &str,
) -> DeviceSlot {
    DeviceSlot {
        id: format!("device:{}", device.device_id),
        revision: sequence,
        device_id: device.device_id.clone(),
        device_name: device.device_name.clone(),
        platform: device.platform.clone(),
        online,
        pinned: None,
        availability: availability.into(),
        preview: truncate_text(text, 4096),
        captured_at,
        age_label: if online {
            "刚刚".into()
        } else {
            "本机缓存".into()
        },
        groups: vec!["直接配对".into()],
        sequence,
        size: text.len() as u64,
        representations: vec![ClipboardRepresentation {
            id: "text/plain".into(),
            kind: "text".into(),
            label: "纯文本".into(),
            mime: "text/plain;charset=utf-8".into(),
            size: text.len() as u64,
            status: "ready".into(),
            enabled: true,
        }],
        blocked_reason: None,
        progress: None,
    }
}

fn image_slot(
    device: &TrustedDevice,
    sequence: u64,
    rgba_len: usize,
    width: u32,
    height: u32,
    captured_at: String,
) -> DeviceSlot {
    DeviceSlot {
        id: format!("device:{}", device.device_id),
        revision: sequence,
        device_id: device.device_id.clone(),
        device_name: device.device_name.clone(),
        platform: device.platform.clone(),
        online: true,
        pinned: None,
        availability: "ready".into(),
        preview: format!("图片 · {width} × {height}"),
        captured_at,
        age_label: "刚刚".into(),
        groups: vec!["直接配对".into()],
        sequence,
        size: rgba_len as u64,
        representations: vec![ClipboardRepresentation {
            id: "image/rgba".into(),
            kind: "image".into(),
            label: "图片".into(),
            mime: "image/png".into(),
            size: rgba_len as u64,
            status: "ready".into(),
            enabled: true,
        }],
        blocked_reason: None,
        progress: None,
    }
}

pub(crate) fn image_hash(rgba: &[u8], width: u32, height: u32) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"localdrop-clipboard-image-v1\0");
    hash.update(width.to_be_bytes());
    hash.update(height.to_be_bytes());
    hash.update(rgba);
    hash.finalize().into()
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
    if text.len() > 1024 * 1024 {
        return report_clipboard_failure(state, app, "文本剪贴板超过 1 MiB，已跳过同步".into());
    }
    let suppress_publish = {
        let mut suppressed = state
            .suppress_next_capture
            .lock()
            .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())?;
        if suppressed.as_deref() == Some(text.as_str()) {
            *suppressed = None;
            true
        } else {
            false
        }
    };
    let snapshot = update(state, app, |snapshot| {
        if !suppress_publish {
            snapshot.current_clipboard = CurrentClipboard {
                source: "local".into(),
                source_label: "来自本机系统剪贴板".into(),
                preview: truncate_text(&text, 4096),
                types: vec!["纯文本".into()],
                changed_at: now.clone(),
            };
        }
        if !snapshot.publish_paused && snapshot.settings.allow_text && !suppress_publish {
            let preview = truncate_text(&text, 80);
            snapshot.last_published_preview = format!("本机最近捕获：{preview}");
        }
        snapshot.last_synchronized_at = now.clone();
        snapshot.clipboard_capability.can_read_text = true;
        snapshot.clipboard_capability.foreground_capture = true;
        snapshot.clipboard_capability.limitation = None;
        Ok(())
    })?;
    if !snapshot.publish_paused && snapshot.settings.allow_text && !suppress_publish {
        let sequence = state.store.next_origin_sequence()?;
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            let enabled_devices = state.enabled_device_ids()?;
            transport.broadcast_text(sequence, text, now, &enabled_devices);
        }
    }
    Ok(())
}

pub fn capture_local_image(
    state: &ServiceState,
    app: &AppHandle,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    now: String,
) -> Result<(), String> {
    let expected_length = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "图片尺寸溢出".to_string())?;
    if width == 0 || height == 0 || rgba.len() != expected_length {
        return Err("系统剪贴板图片格式无效".into());
    }
    if rgba.len() > 64 * 1024 * 1024 {
        return Err("剪贴板图片超过 64 MiB，已跳过同步".into());
    }
    let hash = image_hash(&rgba, width, height);
    let suppress_publish = {
        let mut suppressed = state
            .suppress_next_image
            .lock()
            .map_err(|_| "图片剪贴板回环抑制锁已损坏".to_string())?;
        if suppressed.as_ref() == Some(&hash) {
            *suppressed = None;
            true
        } else {
            false
        }
    };
    let snapshot = update(state, app, |snapshot| {
        if !suppress_publish {
            snapshot.current_clipboard = CurrentClipboard {
                source: "local".into(),
                source_label: "来自本机系统剪贴板".into(),
                preview: format!("图片 · {width} × {height}"),
                types: vec!["图片".into()],
                changed_at: now.clone(),
            };
        }
        if !snapshot.publish_paused && snapshot.settings.allow_images && !suppress_publish {
            snapshot.last_published_preview = format!("本机最近捕获：图片 {width} × {height}");
        }
        snapshot.last_synchronized_at = now.clone();
        Ok(())
    })?;
    if !snapshot.publish_paused && snapshot.settings.allow_images && !suppress_publish {
        let sequence = state.store.next_origin_sequence()?;
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            let enabled_devices = state.enabled_device_ids()?;
            transport.broadcast_image(sequence, rgba, width, height, now, &enabled_devices);
        }
    }
    Ok(())
}

pub fn report_clipboard_failure(
    state: &ServiceState,
    app: &AppHandle,
    message: String,
) -> Result<(), String> {
    tracing::warn!(error = %message, "clipboard capture unavailable");
    update(state, app, |snapshot| {
        snapshot.clipboard_capability.can_read_text = false;
        snapshot.clipboard_capability.foreground_capture = false;
        snapshot.clipboard_capability.limitation = Some(message);
        Ok(())
    })?;
    Ok(())
}

pub fn report_clipboard_recovered(state: &ServiceState, app: &AppHandle) -> Result<(), String> {
    update(state, app, |snapshot| {
        snapshot.clipboard_capability.can_read_text = true;
        snapshot.clipboard_capability.foreground_capture = true;
        snapshot.clipboard_capability.limitation = None;
        Ok(())
    })?;
    Ok(())
}

pub fn upsert_nearby_device(
    state: &ServiceState,
    app: &AppHandle,
    mut nearby: NearbyDevice,
) -> Result<(), String> {
    nearby.paired = state.trusted_device(&nearby.device_id)?.is_some();
    update(state, app, |snapshot| {
        if let Some(existing) = snapshot
            .nearby_devices
            .iter_mut()
            .find(|device| device.instance_id == nearby.instance_id)
        {
            *existing = nearby;
        } else {
            snapshot.nearby_devices.push(nearby);
            snapshot
                .nearby_devices
                .sort_by(|left, right| left.device_name.cmp(&right.device_name));
        }
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn show_pending_pairing(
    state: &ServiceState,
    app: &AppHandle,
    pairing: PendingPairing,
) -> Result<(), String> {
    update(state, app, |snapshot| {
        snapshot
            .pending_pairings
            .retain(|item| item.pairing_id != pairing.pairing_id);
        snapshot.pending_pairings.push(pairing);
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn pairing_status(
    state: &ServiceState,
    app: &AppHandle,
    pairing_id: &str,
    status: &str,
) -> Result<(), String> {
    update(state, app, |snapshot| {
        if let Some(pairing) = snapshot
            .pending_pairings
            .iter_mut()
            .find(|item| item.pairing_id == pairing_id)
        {
            pairing.status = status.to_string();
        }
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn pairing_completed(
    state: &ServiceState,
    app: &AppHandle,
    pairing_id: &str,
    device: TrustedDevice,
) -> Result<(), String> {
    update(state, app, |snapshot| {
        snapshot
            .pending_pairings
            .retain(|item| item.pairing_id != pairing_id);
        snapshot
            .trusted_devices
            .retain(|item| item.device_id != device.device_id);
        snapshot.trusted_devices.push(TrustedDeviceView {
            device_id: device.device_id.clone(),
            device_name: device.device_name.clone(),
            platform: device.platform.clone(),
            paired_at: device.paired_at.clone(),
            online: true,
            sync_enabled: device.sync_enabled,
        });
        if let Some(nearby) = snapshot
            .nearby_devices
            .iter_mut()
            .find(|item| item.device_id == device.device_id)
        {
            nearby.paired = true;
        }
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn set_trusted_online(
    state: &ServiceState,
    app: &AppHandle,
    device_id: &str,
    online: bool,
) -> Result<(), String> {
    update(state, app, |snapshot| {
        if let Some(device) = snapshot
            .trusted_devices
            .iter_mut()
            .find(|item| item.device_id == device_id)
        {
            device.online = online;
        }
        for slot in snapshot
            .slots
            .iter_mut()
            .filter(|item| item.device_id == device_id)
        {
            slot.online = online;
        }
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn receive_remote_text(
    state: &ServiceState,
    app: &AppHandle,
    device: &TrustedDevice,
    sequence: u64,
    text: String,
    captured_at: String,
) -> Result<(), String> {
    if text.len() > 1024 * 1024 {
        return Err("远端文本超过 1 MiB".into());
    }
    if !state
        .trusted_device(&device.device_id)?
        .is_some_and(|trusted| trusted.sync_enabled)
    {
        return Err("该设备的剪贴板同步已停用".into());
    }
    if !state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?
        .settings
        .allow_text
    {
        return Err("本机策略已停用纯文本同步".into());
    }
    if state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?
        .subscribe_paused
    {
        return Ok(());
    }
    let slot_id = format!("device:{}", device.device_id);
    {
        let mut bodies = state
            .remote_bodies
            .lock()
            .map_err(|_| "远端正文缓存锁已损坏".to_string())?;
        bodies.insert(slot_id.clone(), RemoteClipboardBody::Text(text.clone()));
    }
    match state.clipboard_cache.store(&CachedText {
        device_id: device.device_id.clone(),
        sequence,
        text: text.clone(),
        captured_at: captured_at.clone(),
    }) {
        Ok(Some(object_name)) => {
            let metadata = CachedSlotMetadata {
                device_id: device.device_id.clone(),
                sequence,
                object_name,
                expires_at_unix: unix_seconds().saturating_add(24 * 60 * 60),
            };
            match state.store.save_cached_slot(&metadata) {
                Ok(Some(previous)) if previous != metadata.object_name => {
                    state.clipboard_cache.remove(&previous);
                }
                Ok(_) => {}
                Err(error) => {
                    state.clipboard_cache.remove(&metadata.object_name);
                    tracing::warn!(device_id = %device.device_id, error = %error, "clipboard cache metadata unavailable")
                }
            }
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(device_id = %device.device_id, error = %error, "clipboard cache unavailable")
        }
    }
    update(state, app, |snapshot| {
        if let Some(existing) = snapshot
            .slots
            .iter()
            .find(|slot| slot.device_id == device.device_id)
        {
            if sequence <= existing.sequence {
                return Ok(());
            }
        }
        let slot = text_slot(device, sequence, &text, captured_at.clone(), true, "ready");
        snapshot
            .slots
            .retain(|item| item.device_id != device.device_id);
        snapshot.slots.push(slot);
        snapshot.last_synchronized_at = captured_at;
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn receive_remote_image(
    state: &ServiceState,
    app: &AppHandle,
    device: &TrustedDevice,
    image: RemoteImage,
) -> Result<(), String> {
    let expected_length = (image.width as usize)
        .checked_mul(image.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "远端图片尺寸溢出".to_string())?;
    if image.width == 0
        || image.height == 0
        || image.rgba.len() != expected_length
        || image.rgba.len() > 64 * 1024 * 1024
    {
        return Err("远端图片格式或大小无效".into());
    }
    if !state
        .trusted_device(&device.device_id)?
        .is_some_and(|trusted| trusted.sync_enabled)
    {
        return Err("该设备的剪贴板同步已停用".into());
    }
    {
        let snapshot = state
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        if snapshot.subscribe_paused || !snapshot.settings.allow_images {
            return Err("本机策略已停用图片同步".into());
        }
        if snapshot
            .slots
            .iter()
            .find(|slot| slot.device_id == device.device_id)
            .is_some_and(|slot| image.sequence <= slot.sequence)
        {
            return Ok(());
        }
    }
    let slot_id = format!("device:{}", device.device_id);
    state
        .remote_bodies
        .lock()
        .map_err(|_| "远端正文缓存锁已损坏".to_string())?
        .insert(
            slot_id,
            RemoteClipboardBody::Image {
                rgba: image.rgba.clone(),
                width: image.width,
                height: image.height,
            },
        );
    if let Some(object_name) = state.store.remove_cached_slot(&device.device_id)? {
        state.clipboard_cache.remove(&object_name);
    }
    update(state, app, |snapshot| {
        snapshot
            .slots
            .retain(|slot| slot.device_id != device.device_id);
        snapshot.slots.push(image_slot(
            device,
            image.sequence,
            image.rgba.len(),
            image.width,
            image.height,
            image.captured_at.clone(),
        ));
        snapshot.last_synchronized_at = image.captured_at;
        Ok(())
    })?;
    Ok(())
}

pub fn remove_nearby_device(
    state: &ServiceState,
    app: &AppHandle,
    instance_id: &str,
) -> Result<(), String> {
    update(state, app, |snapshot| {
        snapshot
            .nearby_devices
            .retain(|device| device.instance_id != instance_id);
        Ok(())
    })?;
    Ok(())
}

#[tauri::command]
pub fn allow_pairing(
    transport: State<'_, super::transport::TransportHandle>,
) -> Result<(), String> {
    transport.allow_pairing(120);
    Ok(())
}

#[tauri::command]
pub fn begin_pairing(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    instance_id: String,
) -> Result<(), String> {
    let nearby = state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?
        .nearby_devices
        .iter()
        .find(|device| device.instance_id == instance_id)
        .cloned()
        .ok_or_else(|| "附近设备已离线".to_string())?;
    if nearby.paired {
        return Err("该设备已经配对".into());
    }
    transport.connect_pairing(app, nearby);
    Ok(())
}

#[tauri::command]
pub fn confirm_pairing(
    transport: State<'_, super::transport::TransportHandle>,
    pairing_id: String,
    accepted: bool,
) -> Result<(), String> {
    transport.confirm_pairing(&pairing_id, accepted)
}

#[tauri::command]
pub fn set_device_sync_enabled(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    device_id: String,
    enabled: bool,
) -> Result<(), String> {
    state.store.set_device_sync_enabled(&device_id, enabled)?;
    update(&state, &app, |snapshot| {
        let device = snapshot
            .trusted_devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
            .ok_or_else(|| "可信设备不存在".to_string())?;
        device.sync_enabled = enabled;
        if !enabled {
            device.online = false;
            snapshot.slots.retain(|slot| slot.device_id != device_id);
        }
        Ok(())
    })?;
    if enabled {
        if let Some(nearby) = state.nearby_device(&device_id) {
            transport.connect_trusted(app, nearby);
        }
    } else {
        transport.disable_peer(&device_id);
        state
            .remote_bodies
            .lock()
            .map_err(|_| "远端正文缓存锁已损坏".to_string())?
            .remove(&format!("device:{device_id}"));
        if let Some(object_name) = state.store.remove_cached_slot(&device_id)? {
            state.clipboard_cache.remove(&object_name);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn revoke_device(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    device_id: String,
) -> Result<(), String> {
    let revoked_at = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
    if let Some(object_name) = state.store.remove_cached_slot(&device_id)? {
        state.clipboard_cache.remove(&object_name);
    }
    state.store.revoke_device(&device_id, &revoked_at)?;
    transport.disable_peer(&device_id);
    state
        .remote_bodies
        .lock()
        .map_err(|_| "远端正文缓存锁已损坏".to_string())?
        .remove(&format!("device:{device_id}"));
    update(&state, &app, |snapshot| {
        snapshot
            .trusted_devices
            .retain(|device| device.device_id != device_id);
        snapshot.slots.retain(|slot| slot.device_id != device_id);
        snapshot
            .pending_pairings
            .retain(|pairing| pairing.device_id != device_id);
        if let Some(nearby) = snapshot
            .nearby_devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        {
            nearby.paired = false;
        }
        Ok(())
    })?;
    Ok(())
}

#[tauri::command]
pub fn get_snapshot(
    state: State<'_, ServiceState>,
    platform: String,
    now: String,
) -> Result<UiSnapshot, String> {
    let mut snapshot = state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
    snapshot.platform = if platform == "android" {
        "android".into()
    } else {
        "desktop".into()
    };
    snapshot
        .pending_pairings
        .retain(|pairing| pairing.expires_at.as_str() > now.as_str());
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
    let snapshot = update(&state, &app, |snapshot| match kind.as_str() {
        "publish" => {
            snapshot.publish_paused = paused;
            Ok(())
        }
        "subscribe" => {
            snapshot.subscribe_paused = paused;
            Ok(())
        }
        _ => Err("未知暂停类型".into()),
    })?;
    state
        .store
        .save_runtime(snapshot.publish_paused, snapshot.subscribe_paused)
}

#[tauri::command]
pub fn set_synchronization_paused(
    state: State<'_, ServiceState>,
    app: AppHandle,
    paused: bool,
) -> Result<(), String> {
    let snapshot = update(&state, &app, |snapshot| {
        snapshot.publish_paused = paused;
        snapshot.subscribe_paused = paused;
        Ok(())
    })?;
    state
        .store
        .save_runtime(snapshot.publish_paused, snapshot.subscribe_paused)
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
    })?;
    Ok(())
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
    let snapshot = update(&state, &app, |snapshot| {
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
        if !snapshot.settings.allow_text {
            snapshot.slots.retain(|slot| {
                !slot
                    .representations
                    .iter()
                    .any(|representation| representation.kind == "text")
            });
            snapshot.imports.clear();
        }
        if !snapshot.settings.allow_images {
            snapshot.slots.retain(|slot| {
                !slot
                    .representations
                    .iter()
                    .any(|representation| representation.kind == "image")
            });
            snapshot.imports.clear();
        }
        Ok(())
    })?;
    state.store.save_settings(&snapshot.settings)?;
    if !snapshot.settings.allow_text {
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            transport.clear_latest_text();
        }
        for device in state.store.trusted_devices()? {
            if let Some(object_name) = state.store.remove_cached_slot(&device.device_id)? {
                state.clipboard_cache.remove(&object_name);
            }
        }
    }
    if !snapshot.settings.allow_images {
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            transport.clear_latest_image();
        }
    }
    if !snapshot.settings.allow_text || !snapshot.settings.allow_images {
        state
            .remote_bodies
            .lock()
            .map_err(|_| "远端正文缓存锁已损坏".to_string())?
            .retain(|_, body| match body {
                RemoteClipboardBody::Text(_) => snapshot.settings.allow_text,
                RemoteClipboardBody::Image { .. } => snapshot.settings.allow_images,
            });
    }
    Ok(())
}

#[tauri::command]
pub fn create_import_intent(
    state: State<'_, ServiceState>,
    app: AppHandle,
    slot_id: String,
    revision: u64,
) -> Result<String, String> {
    let mut snapshot = state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
    let slot = snapshot
        .slots
        .iter()
        .find(|slot| slot.id == slot_id && slot.revision == revision)
        .cloned()
        .ok_or_else(|| "设备槽位不存在或已经更新".to_string())?;
    let is_image = slot
        .representations
        .iter()
        .any(|representation| representation.kind == "image");
    if (is_image && !snapshot.settings.allow_images) || (!is_image && !snapshot.settings.allow_text)
    {
        return Err("本机策略已停用此内容类型的取用".into());
    }
    let bodies = state
        .remote_bodies
        .lock()
        .map_err(|_| "远端正文缓存锁已损坏".to_string())?;
    if !bodies.contains_key(&slot.id) {
        return Err(format!("{} 的远端正文当前不可用", slot.device_name));
    }
    let import_id = uuid::Uuid::new_v4().simple().to_string();
    snapshot.imports.push(ImportOperation {
        id: import_id.clone(),
        slot_id: slot.id.clone(),
        device_name: slot.device_name.clone(),
        source_summary: truncate_text(&slot.preview, 80),
        status: "awaiting_confirmation".into(),
        progress: 100,
        message: Some("确认后才会写入本机系统剪贴板".into()),
    });
    snapshot.bump();
    let emitted = snapshot.clone();
    drop(bodies);
    drop(snapshot);
    emit_snapshot(&app, &emitted)?;
    Ok(import_id)
}

#[tauri::command]
pub fn confirm_import(
    state: State<'_, ServiceState>,
    app: AppHandle,
    import_id: String,
) -> Result<(), String> {
    let (slot_id, body) = {
        let snapshot = state
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        let operation = snapshot
            .imports
            .iter()
            .find(|item| item.id == import_id && item.status == "awaiting_confirmation")
            .ok_or_else(|| "没有可确认的远端剪贴板导入".to_string())?;
        let bodies = state
            .remote_bodies
            .lock()
            .map_err(|_| "远端正文缓存锁已损坏".to_string())?;
        let body = bodies
            .get(&operation.slot_id)
            .cloned()
            .ok_or_else(|| "远端正文已经不可用".to_string())?;
        (operation.slot_id.clone(), body)
    };
    let (preview, types) = match &body {
        RemoteClipboardBody::Text(text) => {
            *state
                .suppress_next_capture
                .lock()
                .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())? = Some(text.clone());
            if let Err(error) = app.clipboard().write_text(text) {
                *state
                    .suppress_next_capture
                    .lock()
                    .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())? = None;
                return Err(format!("无法写入本机系统剪贴板：{error}"));
            }
            (truncate_text(text, 4096), vec!["纯文本".into()])
        }
        RemoteClipboardBody::Image {
            rgba,
            width,
            height,
        } => {
            *state
                .suppress_next_image
                .lock()
                .map_err(|_| "图片剪贴板回环抑制锁已损坏".to_string())? =
                Some(image_hash(rgba, *width, *height));
            let image = tauri::image::Image::new_owned(rgba.clone(), *width, *height);
            if let Err(error) = app.clipboard().write_image(&image) {
                *state
                    .suppress_next_image
                    .lock()
                    .map_err(|_| "图片剪贴板回环抑制锁已损坏".to_string())? = None;
                return Err(format!("无法写入本机图片剪贴板：{error}"));
            }
            (format!("图片 · {width} × {height}"), vec!["图片".into()])
        }
    };
    update(&state, &app, |snapshot| {
        if let Some(operation) = snapshot
            .imports
            .iter_mut()
            .find(|item| item.id == import_id && item.slot_id == slot_id)
        {
            operation.status = "imported".into();
            operation.message = Some("已写入本机剪贴板".into());
            snapshot.current_clipboard = CurrentClipboard {
                source: "remote".into(),
                source_label: format!("取自 {}", operation.device_name),
                preview: preview.clone(),
                types: types.clone(),
                changed_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into()),
            };
        }
        Ok(())
    })?;
    Ok(())
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
    })?;
    Ok(())
}
