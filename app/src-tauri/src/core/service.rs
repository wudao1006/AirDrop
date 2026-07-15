use super::{
    cache::{CachedText, ClipboardCache},
    files::{prepare_file_cache, stage_file_bundle, ReceivedFileBundle},
    group::{
        GroupManifest, GroupMember, GroupPolicy, GroupTombstone, MemberState, SignedGroupManifest,
        SignedGroupTombstone, SyncDirection, GROUP_ENCODING_VERSION, MAX_GROUP_MEMBERS,
    },
    identity::Identity,
    storage::{
        CachedSlotMetadata, Store, StoredGroupInvite, StoredGroupLeave, StoredRuntime,
        TrustedDevice,
    },
};
use data_encoding::BASE64;
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder, RgbaImage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
#[cfg(desktop)]
use tauri_plugin_global_shortcut::GlobalShortcutExt;

const SNAPSHOT_EVENT: &str = "airdrop://snapshot";
const ANDROID_CLIPBOARD_LIMITATION: &str = "Android 首版仅支持前台纯文本和 URL 剪贴板";

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
    #[serde(skip_serializing_if = "Option::is_none")]
    image_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_names: Option<Vec<String>>,
    captured_at: String,
    age_label: String,
    groups: Vec<String>,
    group_ids: Vec<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    image_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_names: Option<Vec<String>>,
    types: Vec<String>,
    changed_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOperation {
    id: String,
    slot_id: String,
    revision: u64,
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
    advertised_name: String,
    local_alias: Option<String>,
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncGroupView {
    group_id: String,
    name: String,
    owner_device_id: String,
    revision: u64,
    membership_epoch: u64,
    is_owner: bool,
    policy: GroupPolicy,
    members: Vec<GroupMember>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingGroupInviteView {
    invite_id: String,
    group_id: String,
    group_name: String,
    owner_device_id: String,
    owner_name: String,
    expires_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSyncGroupInput {
    name: String,
    member_device_ids: Vec<String>,
    allow_text: bool,
    allow_images: bool,
    allow_html: bool,
    allow_files: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupPolicyInput {
    group_id: String,
    allow_text: bool,
    allow_images: bool,
    allow_html: bool,
    allow_files: bool,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum PreparedDragData {
    Fixed(String),
    Map(HashMap<String, String>),
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum PreparedDragItem {
    Files(Vec<String>),
    Data {
        data: PreparedDragData,
        types: Vec<String>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedSlotDrag {
    item: PreparedDragItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    lease_id: Option<String>,
}

#[derive(Clone)]
enum RemoteClipboardBody {
    Text {
        sequence: u64,
        text: String,
    },
    Rich {
        sequence: u64,
        text: String,
        html: Option<String>,
        rtf: Option<String>,
    },
    Files {
        sequence: u64,
        bundle: Arc<ReceivedFileBundle>,
    },
    Image {
        sequence: u64,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
}

impl RemoteClipboardBody {
    fn sequence(&self) -> u64 {
        match self {
            Self::Text { sequence, .. }
            | Self::Rich { sequence, .. }
            | Self::Files { sequence, .. }
            | Self::Image { sequence, .. } => *sequence,
        }
    }
}

pub(crate) struct RemoteImage {
    pub(crate) sequence: u64,
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) captured_at: String,
    pub(crate) group_ids: Vec<String>,
}

pub(crate) struct RemoteRich {
    pub(crate) sequence: u64,
    pub(crate) text: String,
    pub(crate) html: Option<String>,
    pub(crate) rtf: Option<String>,
    pub(crate) captured_at: String,
    pub(crate) group_ids: Vec<String>,
}

pub(crate) struct RemoteFiles {
    pub(crate) sequence: u64,
    pub(crate) bundle: Arc<ReceivedFileBundle>,
    pub(crate) captured_at: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) total_size: u64,
}

struct SlotGroups {
    names: Vec<String>,
    ids: Vec<String>,
}

struct SlotPresentation {
    preview_content: bool,
    online: bool,
    availability: &'static str,
}

enum SlotPreviewUpdate {
    Text(String),
    Image(Option<String>),
    Files {
        preview: String,
        file_names: Option<Vec<String>>,
    },
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
    #[serde(default = "default_global_shortcut")]
    pub(crate) global_shortcut: String,
    pub(crate) preview_text: bool,
    pub(crate) preview_images: bool,
    pub(crate) preview_file_names: bool,
    pub(crate) allow_text: bool,
    pub(crate) allow_html: bool,
    pub(crate) allow_images: bool,
    pub(crate) allow_urls: bool,
    pub(crate) allow_files: bool,
    pub(crate) allow_private: bool,
    #[serde(default)]
    pub(crate) content_policy_version: u8,
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
            global_shortcut: default_global_shortcut(),
            preview_text: true,
            preview_images: false,
            preview_file_names: false,
            allow_text: true,
            allow_html: true,
            allow_images: true,
            allow_urls: true,
            allow_files: true,
            allow_private: false,
            content_policy_version: 1,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSnapshot {
    revision: u64,
    platform: String,
    local_device_name: String,
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
    pairing_allowed_until: Option<u64>,
    cache_persistent: bool,
    sync_groups: Vec<SyncGroupView>,
    pending_group_invites: Vec<PendingGroupInviteView>,
    imports: Vec<ImportOperation>,
    settings: AppSettings,
}

impl UiSnapshot {
    fn initial(
        local_device_name: String,
        settings: AppSettings,
        runtime: Option<StoredRuntime>,
        trusted_devices: Vec<TrustedDevice>,
    ) -> Self {
        let mut settings = settings;
        if cfg!(target_os = "android") {
            settings.allow_html = false;
            settings.allow_images = false;
            settings.allow_files = false;
            settings.allow_private = false;
        }
        let runtime = runtime.unwrap_or(StoredRuntime {
            publish_paused: false,
            subscribe_paused: false,
        });
        Self {
            revision: 1,
            platform: if cfg!(target_os = "android") {
                "android".into()
            } else {
                "desktop".into()
            },
            local_device_name,
            activity: "foreground_live".into(),
            last_synchronized_at: "1970-01-01T00:00:00.000Z".into(),
            clipboard_capability: ClipboardCapability {
                can_read_text: true,
                can_write_text: true,
                foreground_capture: true,
                limitation: cfg!(target_os = "android")
                    .then(|| ANDROID_CLIPBOARD_LIMITATION.into()),
            },
            demo_mode: false,
            daemon_connected: true,
            publish_paused: runtime.publish_paused,
            subscribe_paused: runtime.subscribe_paused,
            current_clipboard: CurrentClipboard {
                source: "unknown".into(),
                source_label: "正在监听本机剪贴板".into(),
                preview: "复制文本、图片、富文本或文件后会自动显示在这里。".into(),
                image_preview: None,
                file_names: None,
                types: Vec::new(),
                changed_at: "1970-01-01T00:00:00.000Z".into(),
            },
            last_published_preview: "等待本机剪贴板变化".into(),
            slots: Vec::new(),
            nearby_devices: Vec::new(),
            trusted_devices: trusted_devices
                .into_iter()
                .map(|device| {
                    let device_name = device.display_name().to_string();
                    TrustedDeviceView {
                        device_id: device.device_id,
                        device_name,
                        advertised_name: device.device_name,
                        local_alias: device.local_alias,
                        platform: device.platform,
                        paired_at: device.paired_at,
                        online: false,
                        sync_enabled: device.sync_enabled,
                    }
                })
                .collect(),
            pending_pairings: Vec::new(),
            pairing_allowed_until: None,
            cache_persistent: false,
            sync_groups: Vec::new(),
            pending_group_invites: Vec::new(),
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
    device_name: Mutex<String>,
    remote_bodies: Mutex<HashMap<String, RemoteClipboardBody>>,
    incoming_sequences: Mutex<HashMap<String, u64>>,
    recoverable_sequences: Mutex<HashSet<String>>,
    suppress_next_capture: Mutex<Option<String>>,
    suppress_next_rich: Mutex<Option<[u8; 32]>>,
    suppress_next_image: Mutex<Option<[u8; 32]>>,
    suppress_next_files: Mutex<Option<[u8; 32]>>,
    clipboard_cache: ClipboardCache,
    file_cache_root: PathBuf,
    imported_files: Mutex<Option<Arc<ReceivedFileBundle>>>,
    accepted_file_transfers: Mutex<HashMap<String, String>>,
    drag_file_leases: Mutex<HashMap<String, (Arc<ReceivedFileBundle>, u64)>>,
}

impl ServiceState {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        let store = Store::open(data_dir)?;
        let identity = Identity::load_or_create(data_dir)?;
        let device_name = match store.load_device_name()? {
            Some(device_name) => device_name,
            None => {
                let device_name = identity.device_name().to_string();
                store.save_device_name(&device_name)?;
                device_name
            }
        };
        let mut settings = store.load_settings()?.unwrap_or_default();
        if settings.content_policy_version < 1 {
            settings.allow_files = true;
            settings.content_policy_version = 1;
            store.save_settings(&settings)?;
        }
        let runtime = store.load_runtime()?;
        let trusted_devices = store.trusted_devices()?;
        let clipboard_cache = ClipboardCache::open(data_dir);
        let file_cache_root = data_dir.join("cache").join("files");
        prepare_file_cache(&file_cache_root);
        let mut snapshot = UiSnapshot::initial(
            device_name.clone(),
            settings,
            runtime,
            trusted_devices.clone(),
        );
        snapshot.cache_persistent = clipboard_cache.available();
        let manifests = store.group_manifests()?;
        let display_names =
            device_display_names(identity.device_id(), &device_name, trusted_devices.iter());
        snapshot.sync_groups = manifests
            .iter()
            .map(|manifest| {
                let mut view = group_view(manifest, identity.device_id());
                apply_group_display_names(&mut view, &display_names);
                view
            })
            .collect();
        snapshot.pending_group_invites = store
            .group_invites(&current_time())?
            .iter()
            .map(|invite| {
                let mut view = pending_group_invite_view(invite);
                if let Some(name) = display_names.get(&view.owner_device_id) {
                    view.owner_name = name.clone();
                }
                view
            })
            .collect();
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
                if store.is_device_revoked(&metadata.device_id)? {
                    continue;
                }
                let device = trusted_devices
                    .iter()
                    .find(|device| device.device_id == metadata.device_id && device.sync_enabled)
                    .cloned()
                    .or_else(|| {
                        manifests.iter().find_map(|signed| {
                            let manifest = &signed.manifest;
                            manifest.active_member(identity.device_id())?;
                            manifest
                                .active_member(&metadata.device_id)
                                .and_then(|member| trusted_from_group_member(member).ok())
                        })
                    });
                let Some(device) = device else { continue };
                match clipboard_cache.load(&metadata.device_id, &metadata.object_name) {
                    Ok(cached) if cached.sequence == metadata.sequence => {
                        let content_type = text_content_type(&cached.text);
                        let locally_allowed = if content_type == "url" {
                            snapshot.settings.allow_urls
                        } else {
                            snapshot.settings.allow_text
                        };
                        if !locally_allowed {
                            continue;
                        }
                        let Ok(groups) = validate_group_delivery_from_manifests(
                            &manifests,
                            identity.device_id(),
                            &device.device_id,
                            &cached.group_ids,
                            content_type,
                        ) else {
                            continue;
                        };
                        let slot = text_slot(
                            &device,
                            cached.sequence,
                            &cached.text,
                            cached.captured_at,
                            SlotGroups {
                                names: groups,
                                ids: cached.group_ids.clone(),
                            },
                            SlotPresentation {
                                preview_content: snapshot.settings.preview_text,
                                online: false,
                                availability: "stale",
                            },
                        );
                        remote_bodies.insert(
                            slot.id.clone(),
                            RemoteClipboardBody::Text {
                                sequence: cached.sequence,
                                text: cached.text,
                            },
                        );
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
        let mut incoming_sequences = store.remote_sequences()?;
        for slot in &snapshot.slots {
            incoming_sequences
                .entry(slot.device_id.clone())
                .and_modify(|sequence| *sequence = (*sequence).max(slot.sequence))
                .or_insert(slot.sequence);
        }
        let recoverable_sequences = incoming_sequences
            .iter()
            .filter(|(device_id, sequence)| {
                !snapshot
                    .slots
                    .iter()
                    .any(|slot| slot.device_id == **device_id && slot.sequence == **sequence)
            })
            .map(|(device_id, _)| device_id.clone())
            .collect();
        Ok(Self {
            snapshot: Mutex::new(snapshot),
            store,
            identity,
            device_name: Mutex::new(device_name),
            remote_bodies: Mutex::new(remote_bodies),
            incoming_sequences: Mutex::new(incoming_sequences),
            recoverable_sequences: Mutex::new(recoverable_sequences),
            suppress_next_capture: Mutex::new(None),
            suppress_next_rich: Mutex::new(None),
            suppress_next_image: Mutex::new(None),
            suppress_next_files: Mutex::new(None),
            clipboard_cache,
            file_cache_root,
            imported_files: Mutex::new(None),
            accepted_file_transfers: Mutex::new(HashMap::new()),
            drag_file_leases: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn device_id(&self) -> &str {
        self.identity.device_id()
    }

    pub(crate) fn store(&self) -> Store {
        self.store.clone()
    }

    pub(crate) fn device_name(&self) -> Result<String, String> {
        self.device_name
            .lock()
            .map(|name| name.clone())
            .map_err(|_| "本机设备名称状态锁已损坏".to_string())
    }

    #[cfg(desktop)]
    pub(crate) fn configured_global_shortcut(&self) -> Result<String, String> {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.settings.global_shortcut.clone())
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())
    }

    pub(crate) fn identity(&self) -> &Identity {
        &self.identity
    }

    pub(crate) fn incoming_files_root(&self) -> PathBuf {
        self.file_cache_root.join("incoming")
    }

    pub(crate) fn has_accepted_file_transfer(&self, device_id: &str, transfer_id: &str) -> bool {
        self.accepted_file_transfers
            .lock()
            .ok()
            .and_then(|transfers| transfers.get(device_id).cloned())
            .is_some_and(|accepted| accepted == transfer_id)
    }

    pub(crate) fn mark_accepted_file_transfer(&self, device_id: &str, transfer_id: String) {
        if let Ok(mut transfers) = self.accepted_file_transfers.lock() {
            transfers.insert(device_id.into(), transfer_id);
        }
    }

    fn clear_accepted_file_transfer(&self, device_id: &str) {
        if let Ok(mut transfers) = self.accepted_file_transfers.lock() {
            transfers.remove(device_id);
        }
    }

    fn clear_cached_text_best_effort(&self, device_id: &str) {
        match self.store.remove_cached_slot(device_id) {
            Ok(Some(object_name)) => self.clipboard_cache.remove(&object_name),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                device_id,
                error = %error,
                "stale encrypted text cache cleanup deferred"
            ),
        }
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

    fn displayed_device_names(&self) -> Result<HashMap<String, String>, String> {
        let snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        let mut names = snapshot
            .trusted_devices
            .iter()
            .map(|device| (device.device_id.clone(), device.device_name.clone()))
            .collect::<HashMap<_, _>>();
        names.insert(
            self.device_id().to_string(),
            snapshot.local_device_name.clone(),
        );
        Ok(names)
    }

    pub(crate) fn authorized_device(
        &self,
        device_id: &str,
    ) -> Result<Option<TrustedDevice>, String> {
        if self.store.is_device_revoked(device_id)? {
            return Ok(None);
        }
        if let Some(device) = self.store.trusted_device(device_id)? {
            return Ok(device.sync_enabled.then_some(device));
        }
        for signed in self.store.group_manifests()? {
            let manifest = &signed.manifest;
            if manifest.active_member(self.device_id()).is_none() {
                continue;
            }
            if let Some(member) = manifest.active_member(device_id) {
                return Ok(Some(trusted_from_group_member(member)?));
            }
        }
        Ok(None)
    }

    pub(crate) fn delivery_targets(
        &self,
        content_type: &str,
    ) -> Result<HashMap<String, Vec<String>>, String> {
        let mut targets = HashMap::<String, Vec<String>>::new();
        for signed in self.store.group_manifests()? {
            let manifest = &signed.manifest;
            if !group_type_allowed(&manifest.policy, content_type) {
                continue;
            }
            let Some(local) = manifest.active_member(self.device_id()) else {
                continue;
            };
            if !local.direction.can_publish() {
                continue;
            }
            for member in &manifest.members {
                if member.device_id != self.device_id()
                    && member.state == MemberState::Active
                    && member.direction.can_subscribe()
                    && !self.store.is_device_revoked(&member.device_id)?
                    && self.store.is_device_sync_allowed(&member.device_id)?
                {
                    targets
                        .entry(member.device_id.clone())
                        .or_default()
                        .push(manifest.group_id.clone());
                }
            }
        }
        Ok(targets)
    }

    pub(crate) fn validate_group_delivery(
        &self,
        origin_device_id: &str,
        group_ids: &[String],
        content_type: &str,
    ) -> Result<Vec<String>, String> {
        validate_group_delivery_from_manifests(
            &self.store.group_manifests()?,
            self.device_id(),
            origin_device_id,
            group_ids,
            content_type,
        )
    }

    pub(crate) fn validate_incoming_offer(
        &self,
        origin_device_id: &str,
        group_ids: &[String],
        content_type: &str,
    ) -> Result<(), String> {
        self.validate_group_delivery(origin_device_id, group_ids, content_type)?;
        let snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        let enabled = match content_type {
            "text" => snapshot.settings.allow_text,
            "url" => snapshot.settings.allow_urls,
            "image" => snapshot.settings.allow_images,
            "html" => snapshot.settings.allow_html,
            "files" => snapshot.settings.allow_files,
            _ => false,
        };
        if snapshot.subscribe_paused || !enabled {
            return Err("本机策略已停用此内容类型的接收".into());
        }
        Ok(())
    }

    pub(crate) fn validate_incoming_sequence(
        &self,
        origin_device_id: &str,
        sequence: u64,
    ) -> Result<(), String> {
        let sequences = self
            .incoming_sequences
            .lock()
            .map_err(|_| "远端剪贴板序号锁已损坏".to_string())?;
        if let Some(current) = sequences.get(origin_device_id) {
            let recoverable = sequence == *current
                && self
                    .recoverable_sequences
                    .lock()
                    .map_err(|_| "远端剪贴板恢复状态锁已损坏".to_string())?
                    .contains(origin_device_id);
            if sequence < *current || (sequence == *current && !recoverable) {
                return Err("远端剪贴板数据流已经过期".into());
            }
        }
        Ok(())
    }

    pub(crate) fn reconciliation_sequence(&self, origin_device_id: &str) -> Option<u64> {
        let sequence = self
            .incoming_sequences
            .lock()
            .ok()
            .and_then(|sequences| sequences.get(origin_device_id).copied())?;
        let recoverable = self
            .recoverable_sequences
            .lock()
            .ok()
            .is_some_and(|sequences| sequences.contains(origin_device_id));
        (!recoverable).then_some(sequence)
    }

    fn accept_incoming_sequence(
        &self,
        origin_device_id: &str,
        sequence: u64,
        content_hash: &[u8; 32],
    ) -> Result<bool, String> {
        let mut sequences = self
            .incoming_sequences
            .lock()
            .map_err(|_| "远端剪贴板序号锁已损坏".to_string())?;
        if let Some(current) = sequences.get(origin_device_id).copied() {
            if sequence < current {
                return Ok(false);
            }
            if sequence == current {
                let mut recoverable = self
                    .recoverable_sequences
                    .lock()
                    .map_err(|_| "远端剪贴板恢复状态锁已损坏".to_string())?;
                if !recoverable.contains(origin_device_id)
                    || !self.store.remote_sequence_matches(
                        origin_device_id,
                        sequence,
                        content_hash,
                    )?
                {
                    return Ok(false);
                }
                recoverable.remove(origin_device_id);
                return Ok(true);
            }
        }
        if !self
            .store
            .advance_remote_sequence(origin_device_id, sequence, content_hash)?
        {
            return Ok(false);
        }
        sequences.insert(origin_device_id.to_string(), sequence);
        if let Ok(mut recoverable) = self.recoverable_sequences.lock() {
            recoverable.remove(origin_device_id);
        }
        Ok(true)
    }

    pub(crate) fn can_publish_content(&self, content_type: &str) -> bool {
        self.snapshot.lock().ok().is_some_and(|snapshot| {
            !snapshot.publish_paused
                && match content_type {
                    "text" => snapshot.settings.allow_text,
                    "url" => snapshot.settings.allow_urls,
                    "image" => snapshot.settings.allow_images,
                    "html" => snapshot.settings.allow_html,
                    "files" => snapshot.settings.allow_files,
                    _ => false,
                }
        })
    }

    pub(crate) fn replay_group_state(
        &self,
        transport: &super::transport::TransportHandle,
        device_id: &str,
    ) {
        let replayed_at = current_time();
        if let Ok(invites) = self.store.group_invites_for_target(device_id, &replayed_at) {
            for invite in invites {
                let invite_id = invite.invite_id.clone();
                if transport
                    .send_group_invite(
                        device_id,
                        invite.invite_id,
                        invite.expires_at,
                        invite.manifest,
                    )
                    .is_ok()
                {
                    let _ = self.store.set_group_invite_status(&invite_id, "sent");
                }
            }
        }
        if let Ok(invites) = self.store.group_invite_responses_for_owner(device_id) {
            for invite in invites {
                let _ = transport.send_group_accept(
                    device_id,
                    invite.invite_id,
                    invite.manifest.manifest.group_id,
                    invite.status == "accepted",
                );
            }
        }
        if let Ok(groups) = self.store.group_manifests() {
            for group in groups {
                let invited = group.manifest.owner_device_id == self.device_id()
                    && group.manifest.members.iter().any(|member| {
                        member.device_id == device_id && member.state == MemberState::Invited
                    });
                if invited
                    && self
                        .store
                        .should_retry_group_invite(
                            &group.manifest.group_id,
                            device_id,
                            &replayed_at,
                        )
                        .unwrap_or(false)
                {
                    let expires_at = (time::OffsetDateTime::now_utc()
                        + time::Duration::minutes(10))
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| replayed_at.clone());
                    let invite = StoredGroupInvite {
                        invite_id: uuid::Uuid::new_v4().to_string(),
                        target_device_id: device_id.to_string(),
                        expires_at,
                        status: "queued".into(),
                        manifest: group.clone(),
                    };
                    if self.store.save_group_invite(&invite).is_ok() {
                        let invite_id = invite.invite_id.clone();
                        if transport
                            .send_group_invite(
                                device_id,
                                invite.invite_id,
                                invite.expires_at,
                                invite.manifest,
                            )
                            .is_ok()
                        {
                            let _ = self.store.set_group_invite_status(&invite_id, "sent");
                        }
                    }
                }
                if group.manifest.active_member(device_id).is_some() {
                    let _ = transport.send_group_manifest(device_id, group);
                }
            }
        }
        if let Ok(leaves) = self.store.group_leaves_for_owner(device_id) {
            for leave in leaves {
                let _ = transport.send_group_leave(device_id, leave.group_id, leave.leave_id);
            }
        }
        if let Ok(tombstones) = self.store.group_tombstones_for_member(device_id) {
            for tombstone in tombstones {
                let _ = transport.send_group_tombstone(device_id, tombstone);
            }
        }
    }
}

fn trusted_from_group_member(member: &GroupMember) -> Result<TrustedDevice, String> {
    Ok(TrustedDevice {
        device_id: member.device_id.clone(),
        device_name: member.device_name.clone(),
        local_alias: None,
        platform: member.platform.clone(),
        public_key: BASE64
            .decode(member.public_key.as_bytes())
            .map_err(|_| "同步组成员公钥编码无效".to_string())?,
        certificate_der: BASE64
            .decode(member.certificate.as_bytes())
            .map_err(|_| "同步组成员证书编码无效".to_string())?,
        paired_at: member.joined_at.clone(),
        sync_enabled: true,
    })
}

fn group_type_allowed(policy: &GroupPolicy, content_type: &str) -> bool {
    match content_type {
        "text" => policy.allow_text,
        "url" => policy.allow_text,
        "image" => policy.allow_images,
        "html" => policy.allow_html,
        "files" => policy.allow_files,
        _ => false,
    }
}

fn validate_group_delivery_from_manifests(
    manifests: &[SignedGroupManifest],
    local_device_id: &str,
    origin_device_id: &str,
    group_ids: &[String],
    content_type: &str,
) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for signed in manifests {
        let manifest = &signed.manifest;
        if !group_ids.contains(&manifest.group_id)
            || !group_type_allowed(&manifest.policy, content_type)
        {
            continue;
        }
        let Some(origin) = manifest.active_member(origin_device_id) else {
            continue;
        };
        let Some(local) = manifest.active_member(local_device_id) else {
            continue;
        };
        if origin.direction.can_publish() && local.direction.can_subscribe() {
            names.push(manifest.name.clone());
        }
    }
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Err("远端槽位不属于当前允许的同步组".into());
    }
    Ok(names)
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

fn default_global_shortcut() -> String {
    "Ctrl+Alt+KeyZ".into()
}

fn image_preview_data_url(rgba: &[u8], width: u32, height: u32) -> Option<String> {
    let source = RgbaImage::from_raw(width, height, rgba.to_vec())?;
    let thumbnail = image::imageops::thumbnail(&source, 360, 240);
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            thumbnail.as_raw(),
            thumbnail.width(),
            thumbnail.height(),
            ColorType::Rgba8.into(),
        )
        .ok()?;
    Some(format!("data:image/png;base64,{}", BASE64.encode(&png)))
}

fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "图片尺寸溢出".to_string())?;
    if rgba.len() != expected {
        return Err("图片像素数据长度无效".into());
    }
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(rgba, width, height, ColorType::Rgba8.into())
        .map_err(|error| format!("无法编码拖放图片：{error}"))?;
    Ok(png)
}

fn prepare_drag_image_file(
    state: &ServiceState,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<String, String> {
    let root = state.file_cache_root.join("drag");
    fs::create_dir_all(&root).map_err(|error| format!("无法创建拖放图片缓存：{error}"))?;
    let hash = data_encoding::HEXLOWER.encode(&image_hash(rgba, width, height));
    let path = root.join(format!("image-{hash}.png"));
    if !path.exists() {
        let temporary = root.join(format!(
            "image-{hash}-{}.tmp",
            uuid::Uuid::new_v4().simple()
        ));
        fs::write(&temporary, encode_png(rgba, width, height)?)
            .map_err(|error| format!("无法写入拖放图片缓存：{error}"))?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            if !path.exists() {
                return Err(format!("无法提交拖放图片缓存：{error}"));
            }
        }
    }
    Ok(path.to_string_lossy().into_owned())
}

fn copied_file_names(files: &[String]) -> Vec<String> {
    files
        .iter()
        .map(|file| {
            Path::new(file)
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or(file)
                .to_string()
        })
        .collect()
}

fn retry_clipboard_write(mut operation: impl FnMut() -> Result<(), String>) -> Result<(), String> {
    let mut last_error = None;
    for attempt in 0..4 {
        match operation() {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        if attempt < 3 {
            std::thread::sleep(Duration::from_millis(25 * (attempt + 1)));
        }
    }
    Err(last_error.unwrap_or_else(|| "系统剪贴板写入失败".into()))
}

fn redact_disallowed_previews(snapshot: &mut UiSnapshot) {
    if !snapshot.settings.preview_text {
        for slot in &mut snapshot.slots {
            if slot.representations.iter().any(|representation| {
                matches!(representation.kind.as_str(), "text" | "url" | "html")
            }) {
                slot.preview = "文本预览已隐藏".into();
            }
        }
        let current_is_text = snapshot
            .current_clipboard
            .types
            .iter()
            .any(|kind| kind == "URL" || kind.contains("文本") || kind.contains("HTML"));
        if current_is_text {
            snapshot.current_clipboard.preview = "文本预览已隐藏".into();
            snapshot.last_published_preview = "本机最近捕获：文本预览已隐藏".into();
        }
    }
    if !snapshot.settings.preview_images {
        snapshot.current_clipboard.image_preview = None;
        for slot in &mut snapshot.slots {
            slot.image_preview = None;
        }
    }
    if !snapshot.settings.preview_file_names {
        snapshot.current_clipboard.file_names = None;
        for slot in &mut snapshot.slots {
            slot.file_names = None;
            if let Some(files) = slot
                .representations
                .iter()
                .find(|representation| representation.kind == "files")
            {
                slot.preview = files.label.clone();
            }
        }
    }
}

fn refresh_preview_policy(state: &ServiceState, app: &AppHandle) -> Result<(), String> {
    let settings = state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?
        .settings
        .clone();
    let previews = {
        let bodies = state
            .remote_bodies
            .lock()
            .map_err(|_| "远端正文缓存锁已损坏".to_string())?;
        bodies
            .iter()
            .map(|(slot_id, body)| {
                let preview = match body {
                    RemoteClipboardBody::Text { text, .. }
                    | RemoteClipboardBody::Rich { text, .. } => {
                        SlotPreviewUpdate::Text(if settings.preview_text {
                            truncate_text(text, 4096)
                        } else {
                            "文本预览已隐藏".into()
                        })
                    }
                    RemoteClipboardBody::Image {
                        rgba,
                        width,
                        height,
                        ..
                    } => SlotPreviewUpdate::Image(
                        settings
                            .preview_images
                            .then(|| image_preview_data_url(rgba, *width, *height))
                            .flatten(),
                    ),
                    RemoteClipboardBody::Files { bundle, .. } => {
                        let names = bundle.display_names();
                        let count = names.len();
                        SlotPreviewUpdate::Files {
                            preview: if settings.preview_file_names && !names.is_empty() {
                                truncate_text(&names.join("、"), 4096)
                            } else {
                                format!("{count} 个文件或目录")
                            },
                            file_names: settings.preview_file_names.then_some(names),
                        }
                    }
                };
                (slot_id.clone(), preview)
            })
            .collect::<HashMap<_, _>>()
    };
    update(state, app, |snapshot| {
        for slot in &mut snapshot.slots {
            match previews.get(&slot.id) {
                Some(SlotPreviewUpdate::Text(preview)) => slot.preview = preview.clone(),
                Some(SlotPreviewUpdate::Image(image_preview)) => {
                    slot.image_preview = image_preview.clone()
                }
                Some(SlotPreviewUpdate::Files {
                    preview,
                    file_names,
                }) => {
                    slot.preview = preview.clone();
                    slot.file_names = file_names.clone();
                }
                None => {}
            }
        }
        redact_disallowed_previews(snapshot);
        Ok(())
    })?;
    Ok(())
}

fn text_slot(
    device: &TrustedDevice,
    sequence: u64,
    text: &str,
    captured_at: String,
    groups: SlotGroups,
    presentation: SlotPresentation,
) -> DeviceSlot {
    let content_type = text_content_type(text);
    let (kind, label, mime) = if content_type == "url" {
        ("url", "URL", "text/uri-list;charset=utf-8")
    } else {
        ("text", "纯文本", "text/plain;charset=utf-8")
    };
    DeviceSlot {
        id: format!("device:{}", device.device_id),
        revision: sequence,
        device_id: device.device_id.clone(),
        device_name: device.display_name().to_string(),
        platform: device.platform.clone(),
        online: presentation.online,
        pinned: None,
        availability: presentation.availability.into(),
        preview: if presentation.preview_content {
            truncate_text(text, 4096)
        } else {
            "文本预览已隐藏".into()
        },
        image_preview: None,
        file_names: None,
        captured_at,
        age_label: if presentation.online {
            "刚刚".into()
        } else {
            "本机缓存".into()
        },
        groups: groups.names,
        group_ids: groups.ids,
        sequence,
        size: text.len() as u64,
        representations: vec![ClipboardRepresentation {
            id: mime.into(),
            kind: kind.into(),
            label: label.into(),
            mime: mime.into(),
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
    rgba: &[u8],
    dimensions: (u32, u32),
    preview_images: bool,
    captured_at: String,
    groups: SlotGroups,
) -> DeviceSlot {
    let (width, height) = dimensions;
    DeviceSlot {
        id: format!("device:{}", device.device_id),
        revision: sequence,
        device_id: device.device_id.clone(),
        device_name: device.display_name().to_string(),
        platform: device.platform.clone(),
        online: true,
        pinned: None,
        availability: "ready".into(),
        preview: format!("图片 · {width} × {height}"),
        image_preview: preview_images
            .then(|| image_preview_data_url(rgba, width, height))
            .flatten(),
        file_names: None,
        captured_at,
        age_label: "刚刚".into(),
        groups: groups.names,
        group_ids: groups.ids,
        sequence,
        size: rgba.len() as u64,
        representations: vec![ClipboardRepresentation {
            id: "image/rgba".into(),
            kind: "image".into(),
            label: "图片".into(),
            mime: "image/png".into(),
            size: rgba.len() as u64,
            status: "ready".into(),
            enabled: true,
        }],
        blocked_reason: None,
        progress: None,
    }
}

fn rich_slot(
    device: &TrustedDevice,
    rich: &RemoteRich,
    preview_text: bool,
    groups: SlotGroups,
) -> DeviceSlot {
    let fallback_type = text_content_type(&rich.text);
    let (fallback_kind, fallback_label, fallback_mime) = if fallback_type == "url" {
        ("url", "URL 降级", "text/uri-list;charset=utf-8")
    } else {
        ("text", "纯文本降级", "text/plain;charset=utf-8")
    };
    let mut representations = vec![ClipboardRepresentation {
        id: fallback_mime.into(),
        kind: fallback_kind.into(),
        label: fallback_label.into(),
        mime: fallback_mime.into(),
        size: rich.text.len() as u64,
        status: "ready".into(),
        enabled: true,
    }];
    if let Some(html) = &rich.html {
        representations.push(ClipboardRepresentation {
            id: "text/html".into(),
            kind: "html".into(),
            label: "HTML".into(),
            mime: "text/html;charset=utf-8".into(),
            size: html.len() as u64,
            status: "ready".into(),
            enabled: true,
        });
    }
    if let Some(rtf) = &rich.rtf {
        representations.push(ClipboardRepresentation {
            id: "text/rtf".into(),
            kind: "html".into(),
            label: "RTF".into(),
            mime: "text/rtf".into(),
            size: rtf.len() as u64,
            status: "ready".into(),
            enabled: true,
        });
    }
    DeviceSlot {
        id: format!("device:{}", device.device_id),
        revision: rich.sequence,
        device_id: device.device_id.clone(),
        device_name: device.display_name().to_string(),
        platform: device.platform.clone(),
        online: true,
        pinned: None,
        availability: "ready".into(),
        preview: if preview_text {
            truncate_text(&rich.text, 4096)
        } else {
            "文本预览已隐藏".into()
        },
        image_preview: None,
        file_names: None,
        captured_at: rich.captured_at.clone(),
        age_label: "刚刚".into(),
        groups: groups.names,
        group_ids: groups.ids,
        sequence: rich.sequence,
        size: representations.iter().map(|item| item.size).sum(),
        representations,
        blocked_reason: None,
        progress: None,
    }
}

fn file_slot(
    device: &TrustedDevice,
    files: &RemoteFiles,
    preview_names: bool,
    groups: SlotGroups,
) -> DeviceSlot {
    let names = files.bundle.display_names();
    let count = names.len();
    let preview = if preview_names && !names.is_empty() {
        truncate_text(&names.join("、"), 4096)
    } else {
        format!("{count} 个文件或目录")
    };
    DeviceSlot {
        id: format!("device:{}", device.device_id),
        revision: files.sequence,
        device_id: device.device_id.clone(),
        device_name: device.display_name().to_string(),
        platform: device.platform.clone(),
        online: true,
        pinned: None,
        availability: "ready".into(),
        preview,
        image_preview: None,
        file_names: preview_names.then_some(names),
        captured_at: files.captured_at.clone(),
        age_label: "刚刚".into(),
        groups: groups.names,
        group_ids: groups.ids,
        sequence: files.sequence,
        size: files.total_size,
        representations: vec![ClipboardRepresentation {
            id: "application/x-localdrop-files".into(),
            kind: "files".into(),
            label: format!("{count} 个文件或目录"),
            mime: "application/x-localdrop-files".into(),
            size: files.total_size,
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

pub(crate) fn rich_hash(text: &str, html: Option<&str>, rtf: Option<&str>) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"localdrop-rich-clipboard-v1\0");
    hash.update((text.len() as u64).to_be_bytes());
    hash.update(text.as_bytes());
    for value in [html, rtf] {
        match value {
            Some(value) => {
                hash.update([1]);
                hash.update((value.len() as u64).to_be_bytes());
                hash.update(value.as_bytes());
            }
            None => hash.update([0]),
        }
    }
    hash.finalize().into()
}

fn incoming_content_hash(
    kind: &str,
    captured_at: &str,
    group_ids: &[String],
    body_hash: &[u8; 32],
) -> [u8; 32] {
    let mut normalized_group_ids = group_ids.to_vec();
    normalized_group_ids.sort();
    normalized_group_ids.dedup();

    let mut hash = Sha256::new();
    hash.update(b"localdrop-incoming-clipboard-v1\0");
    hash.update((kind.len() as u64).to_be_bytes());
    hash.update(kind.as_bytes());
    hash.update((captured_at.len() as u64).to_be_bytes());
    hash.update(captured_at.as_bytes());
    hash.update((normalized_group_ids.len() as u64).to_be_bytes());
    for group_id in normalized_group_ids {
        hash.update((group_id.len() as u64).to_be_bytes());
        hash.update(group_id.as_bytes());
    }
    hash.update(body_hash);
    hash.finalize().into()
}

pub(crate) fn file_list_hash(files: &[String]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"localdrop-file-list-v1\0");
    for file in files {
        hash.update((file.len() as u64).to_be_bytes());
        hash.update(file.as_bytes());
    }
    hash.finalize().into()
}

pub(crate) fn text_content_type(text: &str) -> &'static str {
    let value = text.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return "text";
    }
    url::Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https" | "ftp" | "mailto"))
        .map_or("text", |_| "url")
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn current_time() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn group_view(manifest: &SignedGroupManifest, local_device_id: &str) -> SyncGroupView {
    SyncGroupView {
        group_id: manifest.manifest.group_id.clone(),
        name: manifest.manifest.name.clone(),
        owner_device_id: manifest.manifest.owner_device_id.clone(),
        revision: manifest.manifest.revision,
        membership_epoch: manifest.manifest.membership_epoch,
        is_owner: manifest.manifest.owner_device_id == local_device_id,
        policy: manifest.manifest.policy.clone(),
        members: manifest.manifest.members.clone(),
    }
}

fn device_display_names<'a>(
    local_device_id: &str,
    local_device_name: &str,
    devices: impl IntoIterator<Item = &'a TrustedDevice>,
) -> HashMap<String, String> {
    let mut names = devices
        .into_iter()
        .map(|device| (device.device_id.clone(), device.display_name().to_string()))
        .collect::<HashMap<_, _>>();
    names.insert(local_device_id.to_string(), local_device_name.to_string());
    names
}

fn apply_group_display_names(group: &mut SyncGroupView, display_names: &HashMap<String, String>) {
    for member in &mut group.members {
        if let Some(name) = display_names.get(&member.device_id) {
            member.device_name = name.clone();
        }
    }
}

fn normalize_device_label(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    let length = value.chars().count();
    if length == 0 {
        return Err(format!("{field}不能为空"));
    }
    if length > 48 {
        return Err(format!("{field}不能超过 48 个字符"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field}不能包含控制字符"));
    }
    Ok(value.to_string())
}

fn pending_group_invite_view(invite: &StoredGroupInvite) -> PendingGroupInviteView {
    let owner_name = invite
        .manifest
        .manifest
        .active_member(&invite.manifest.manifest.owner_device_id)
        .map(|member| member.device_name.clone())
        .unwrap_or_else(|| "未知设备".into());
    PendingGroupInviteView {
        invite_id: invite.invite_id.clone(),
        group_id: invite.manifest.manifest.group_id.clone(),
        group_name: invite.manifest.manifest.name.clone(),
        owner_device_id: invite.manifest.manifest.owner_device_id.clone(),
        owner_name,
        expires_at: invite.expires_at.clone(),
    }
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
    let content_type = text_content_type(&text);
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
                preview: if snapshot.settings.preview_text {
                    truncate_text(&text, 4096)
                } else {
                    "文本预览已隐藏".into()
                },
                image_preview: None,
                file_names: None,
                types: vec![if content_type == "url" {
                    "URL".into()
                } else {
                    "纯文本".into()
                }],
                changed_at: now.clone(),
            };
        }
        let allowed = if content_type == "url" {
            snapshot.settings.allow_urls
        } else {
            snapshot.settings.allow_text
        };
        if !snapshot.publish_paused && allowed && !suppress_publish {
            snapshot.last_published_preview = if snapshot.settings.preview_text {
                format!("本机最近捕获：{}", truncate_text(&text, 80))
            } else {
                "本机最近捕获：文本预览已隐藏".into()
            };
        }
        snapshot.last_synchronized_at = now.clone();
        snapshot.clipboard_capability.can_read_text = true;
        snapshot.clipboard_capability.foreground_capture = true;
        snapshot.clipboard_capability.limitation =
            cfg!(target_os = "android").then(|| ANDROID_CLIPBOARD_LIMITATION.into());
        Ok(())
    })?;
    let allowed = if content_type == "url" {
        snapshot.settings.allow_urls
    } else {
        snapshot.settings.allow_text
    };
    if !snapshot.publish_paused && allowed && !suppress_publish {
        let sequence = state.store.next_origin_sequence()?;
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            let targets = state.delivery_targets(content_type)?;
            transport.broadcast_text(sequence, text, now, &targets);
        }
    }
    Ok(())
}

pub fn capture_local_rich(
    state: &ServiceState,
    app: &AppHandle,
    text: String,
    html: Option<String>,
    rtf: Option<String>,
    now: String,
) -> Result<(), String> {
    let total_size = text
        .len()
        .saturating_add(html.as_ref().map_or(0, String::len))
        .saturating_add(rtf.as_ref().map_or(0, String::len));
    if html.is_none() && rtf.is_none() {
        return capture_local_clipboard(state, app, text, now);
    }
    if total_size > 1024 * 1024 {
        return report_clipboard_failure(state, app, "富文本剪贴板超过 1 MiB，已跳过同步".into());
    }
    let fallback_type = text_content_type(&text);
    let hash = rich_hash(&text, html.as_deref(), rtf.as_deref());
    let suppress_publish = {
        let mut suppressed = state
            .suppress_next_rich
            .lock()
            .map_err(|_| "富文本剪贴板回环抑制锁已损坏".to_string())?;
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
                preview: if snapshot.settings.preview_text {
                    truncate_text(&text, 4096)
                } else {
                    "文本预览已隐藏".into()
                },
                image_preview: None,
                file_names: None,
                types: vec!["富文本 / HTML".into(), "纯文本降级".into()],
                changed_at: now.clone(),
            };
        }
        let fallback_allowed = if fallback_type == "url" {
            snapshot.settings.allow_urls
        } else {
            snapshot.settings.allow_text
        };
        if !snapshot.publish_paused
            && (snapshot.settings.allow_html || fallback_allowed)
            && !suppress_publish
        {
            snapshot.last_published_preview = if snapshot.settings.preview_text {
                format!("本机最近捕获：{}", truncate_text(&text, 80))
            } else {
                "本机最近捕获：文本预览已隐藏".into()
            };
        }
        snapshot.last_synchronized_at = now.clone();
        Ok(())
    })?;
    let fallback_allowed = if fallback_type == "url" {
        snapshot.settings.allow_urls
    } else {
        snapshot.settings.allow_text
    };
    if !snapshot.publish_paused
        && (snapshot.settings.allow_html || fallback_allowed)
        && !suppress_publish
    {
        let sequence = state.store.next_origin_sequence()?;
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            if snapshot.settings.allow_html {
                let rich_targets = state.delivery_targets("html")?;
                let text_targets = if fallback_allowed {
                    state.delivery_targets(fallback_type)?
                } else {
                    HashMap::new()
                };
                transport.broadcast_rich(
                    sequence,
                    text,
                    html,
                    rtf,
                    now,
                    super::transport::RichDeliveryTargets {
                        rich: &rich_targets,
                        text: &text_targets,
                    },
                );
            } else {
                let targets = state.delivery_targets(fallback_type)?;
                transport.broadcast_text(sequence, text, now, &targets);
            }
        }
    }
    Ok(())
}

pub fn capture_local_files(
    state: &ServiceState,
    app: &AppHandle,
    files: Vec<String>,
    now: String,
) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }
    let hash = file_list_hash(&files);
    let suppress_publish = {
        let mut suppressed = state
            .suppress_next_files
            .lock()
            .map_err(|_| "文件剪贴板回环抑制锁已损坏".to_string())?;
        if suppressed.as_ref() == Some(&hash) {
            *suppressed = None;
            true
        } else {
            false
        }
    };
    let file_names = copied_file_names(&files);
    let snapshot = update(state, app, |snapshot| {
        if !suppress_publish {
            snapshot.current_clipboard = CurrentClipboard {
                source: "local".into(),
                source_label: "来自本机系统剪贴板".into(),
                preview: format!("{} 个文件或目录", files.len()),
                image_preview: None,
                file_names: snapshot
                    .settings
                    .preview_file_names
                    .then(|| file_names.clone()),
                types: vec!["文件与目录".into()],
                changed_at: now.clone(),
            };
        }
        snapshot.last_synchronized_at = now.clone();
        Ok(())
    })?;
    if snapshot.publish_paused || !snapshot.settings.allow_files || suppress_publish {
        return Ok(());
    }
    let Some(transport) = app.try_state::<super::transport::TransportHandle>() else {
        return Ok(());
    };
    let targets = state.delivery_targets("files")?;
    if targets.is_empty() {
        return Ok(());
    }
    let sequence = state.store.next_origin_sequence()?;
    let bundle = stage_file_bundle(&files, &state.file_cache_root.join("outgoing"), sequence)?;
    update(state, app, |snapshot| {
        snapshot.last_published_preview = format!("本机最近捕获：{} 个文件或目录", files.len());
        Ok(())
    })?;
    transport.broadcast_files(sequence, bundle, now, &targets);
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
                image_preview: snapshot
                    .settings
                    .preview_images
                    .then(|| image_preview_data_url(&rgba, width, height))
                    .flatten(),
                file_names: None,
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
            let targets = state.delivery_targets("image")?;
            transport.broadcast_image(sequence, rgba, width, height, now, &targets);
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

pub fn report_clipboard_limitation(
    state: &ServiceState,
    app: &AppHandle,
    message: String,
) -> Result<(), String> {
    tracing::warn!(limitation = %message, "extended clipboard capability unavailable");
    update(state, app, |snapshot| {
        snapshot.clipboard_capability.limitation = Some(message);
        Ok(())
    })?;
    Ok(())
}

pub fn upsert_nearby_device(
    state: &ServiceState,
    app: &AppHandle,
    mut nearby: NearbyDevice,
) -> Result<(), String> {
    if let Some(trusted) = state.authorized_device(&nearby.device_id)? {
        nearby.paired = true;
        nearby.device_name = trusted.display_name().to_string();
    }
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
            device_name: device.display_name().to_string(),
            advertised_name: device.device_name.clone(),
            local_alias: device.local_alias.clone(),
            platform: device.platform.clone(),
            paired_at: device.paired_at.clone(),
            // 配对完成只代表身份已持久化；可信 QUIC 连接建立后才算真正在线。
            online: false,
            sync_enabled: device.sync_enabled,
        });
        if let Some(nearby) = snapshot
            .nearby_devices
            .iter_mut()
            .find(|item| item.device_id == device.device_id)
        {
            nearby.paired = true;
            nearby.device_name = device.display_name().to_string();
        }
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn pairing_cancelled(
    state: &ServiceState,
    app: &AppHandle,
    pairing_id: &str,
) -> Result<(), String> {
    state.store.remove_pending_pairing(pairing_id)?;
    update(state, app, |snapshot| {
        snapshot
            .pending_pairings
            .retain(|item| item.pairing_id != pairing_id);
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

pub(crate) fn update_trusted_device_profile(
    state: &ServiceState,
    app: &AppHandle,
    device_id: &str,
    device_name: &str,
    platform: &str,
) -> Result<(String, String), String> {
    let advertised_name = normalize_device_label(device_name, "对方设备名称")?;
    let platform = match platform {
        "macos" | "windows" | "linux" | "android" => platform.to_string(),
        _ => "unknown".to_string(),
    };
    if state.store.trusted_device(device_id)?.is_some() {
        state
            .store
            .update_trusted_device_profile(device_id, &advertised_name, &platform)?;
    }
    update(state, app, |snapshot| {
        let local_alias = snapshot
            .trusted_devices
            .iter()
            .find(|device| device.device_id == device_id)
            .and_then(|device| device.local_alias.clone());
        let display_name = local_alias
            .as_deref()
            .unwrap_or(&advertised_name)
            .to_string();
        if let Some(device) = snapshot
            .trusted_devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        {
            device.device_name = display_name.clone();
            device.advertised_name = advertised_name.clone();
            device.platform = platform.clone();
        }
        for nearby in snapshot
            .nearby_devices
            .iter_mut()
            .filter(|device| device.device_id == device_id && device.paired)
        {
            nearby.device_name = display_name.clone();
            nearby.platform = platform.clone();
        }
        for slot in snapshot
            .slots
            .iter_mut()
            .filter(|slot| slot.device_id == device_id)
        {
            slot.device_name = display_name.clone();
            slot.platform = platform.clone();
        }
        for group in &mut snapshot.sync_groups {
            for member in group
                .members
                .iter_mut()
                .filter(|member| member.device_id == device_id)
            {
                member.device_name = display_name.clone();
                member.platform = platform.clone();
            }
        }
        for invite in snapshot
            .pending_group_invites
            .iter_mut()
            .filter(|invite| invite.owner_device_id == device_id)
        {
            invite.owner_name = display_name.clone();
        }
        for operation in snapshot
            .imports
            .iter_mut()
            .filter(|operation| operation.slot_id == format!("device:{device_id}"))
        {
            operation.device_name = display_name.clone();
        }
        Ok(())
    })?;
    Ok((advertised_name, platform))
}

pub(crate) fn receive_remote_text(
    state: &ServiceState,
    app: &AppHandle,
    device: &TrustedDevice,
    sequence: u64,
    text: String,
    captured_at: String,
    group_ids: Vec<String>,
) -> Result<(), String> {
    if text.len() > 1024 * 1024 {
        return Err("远端文本超过 1 MiB".into());
    }
    if !state
        .authorized_device(&device.device_id)?
        .is_some_and(|trusted| trusted.sync_enabled)
    {
        return Err("该设备的剪贴板同步已停用".into());
    }
    let content_type = text_content_type(&text);
    let group_names = state.validate_group_delivery(&device.device_id, &group_ids, content_type)?;
    let preview_text = {
        let snapshot = state
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        let allowed = if content_type == "url" {
            snapshot.settings.allow_urls
        } else {
            snapshot.settings.allow_text
        };
        if !allowed {
            return Err("本机策略已停用此文本类型同步".into());
        }
        if snapshot.subscribe_paused {
            return Ok(());
        }
        snapshot.settings.preview_text
    };
    let body_hash: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    let content_hash = incoming_content_hash("text", &captured_at, &group_ids, &body_hash);
    if !state.accept_incoming_sequence(&device.device_id, sequence, &content_hash)? {
        return Ok(());
    }
    let slot_id = format!("device:{}", device.device_id);
    state.clear_accepted_file_transfer(&device.device_id);
    {
        let mut bodies = state
            .remote_bodies
            .lock()
            .map_err(|_| "远端正文缓存锁已损坏".to_string())?;
        bodies.insert(
            slot_id.clone(),
            RemoteClipboardBody::Text {
                sequence,
                text: text.clone(),
            },
        );
    }
    match state.clipboard_cache.store(&CachedText {
        device_id: device.device_id.clone(),
        sequence,
        text: text.clone(),
        captured_at: captured_at.clone(),
        group_ids: group_ids.clone(),
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
        let slot = text_slot(
            device,
            sequence,
            &text,
            captured_at.clone(),
            SlotGroups {
                names: group_names,
                ids: group_ids,
            },
            SlotPresentation {
                preview_content: preview_text,
                online: true,
                availability: "ready",
            },
        );
        snapshot
            .slots
            .retain(|item| item.device_id != device.device_id);
        snapshot.slots.push(slot);
        snapshot.last_synchronized_at = captured_at;
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn receive_remote_rich(
    state: &ServiceState,
    app: &AppHandle,
    device: &TrustedDevice,
    rich: RemoteRich,
) -> Result<(), String> {
    let total_size = rich
        .text
        .len()
        .saturating_add(rich.html.as_ref().map_or(0, String::len))
        .saturating_add(rich.rtf.as_ref().map_or(0, String::len));
    if (rich.html.is_none() && rich.rtf.is_none()) || total_size > 1024 * 1024 {
        return Err("远端富文本格式或大小无效".into());
    }
    if !state
        .authorized_device(&device.device_id)?
        .is_some_and(|trusted| trusted.sync_enabled)
    {
        return Err("该设备的剪贴板同步已停用".into());
    }
    let mut group_names =
        state.validate_group_delivery(&device.device_id, &rich.group_ids, "html")?;
    let fallback_type = text_content_type(&rich.text);
    if let Ok(text_group_names) =
        state.validate_group_delivery(&device.device_id, &rich.group_ids, fallback_type)
    {
        group_names.extend(text_group_names);
        group_names.sort();
        group_names.dedup();
    }
    let preview_text = {
        let snapshot = state
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        if snapshot.subscribe_paused || !snapshot.settings.allow_html {
            return Err("本机策略已停用富文本同步".into());
        }
        if snapshot
            .slots
            .iter()
            .find(|slot| slot.device_id == device.device_id)
            .is_some_and(|slot| rich.sequence <= slot.sequence)
        {
            return Ok(());
        }
        snapshot.settings.preview_text
    };
    let body_hash = rich_hash(&rich.text, rich.html.as_deref(), rich.rtf.as_deref());
    let content_hash =
        incoming_content_hash("rich", &rich.captured_at, &rich.group_ids, &body_hash);
    if !state.accept_incoming_sequence(&device.device_id, rich.sequence, &content_hash)? {
        return Ok(());
    }
    let slot_id = format!("device:{}", device.device_id);
    state.clear_accepted_file_transfer(&device.device_id);
    state
        .remote_bodies
        .lock()
        .map_err(|_| "远端正文缓存锁已损坏".to_string())?
        .insert(
            slot_id,
            RemoteClipboardBody::Rich {
                sequence: rich.sequence,
                text: rich.text.clone(),
                html: rich.html.clone(),
                rtf: rich.rtf.clone(),
            },
        );
    update(state, app, |snapshot| {
        snapshot
            .slots
            .retain(|slot| slot.device_id != device.device_id);
        snapshot.slots.push(rich_slot(
            device,
            &rich,
            preview_text,
            SlotGroups {
                names: group_names,
                ids: rich.group_ids.clone(),
            },
        ));
        snapshot.last_synchronized_at = rich.captured_at;
        Ok(())
    })?;
    state.clear_cached_text_best_effort(&device.device_id);
    Ok(())
}

pub(crate) fn receive_remote_files(
    state: &ServiceState,
    app: &AppHandle,
    device: &TrustedDevice,
    files: RemoteFiles,
) -> Result<(), String> {
    if files.bundle.clipboard_paths().is_empty() {
        return Err("远端文件清单为空".into());
    }
    if !state
        .authorized_device(&device.device_id)?
        .is_some_and(|trusted| trusted.sync_enabled)
    {
        return Err("该设备的剪贴板同步已停用".into());
    }
    let group_names =
        state.validate_group_delivery(&device.device_id, &files.group_ids, "files")?;
    let preview_names = {
        let snapshot = state
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        if snapshot.subscribe_paused || !snapshot.settings.allow_files {
            return Err("本机策略已停用文件同步".into());
        }
        if snapshot
            .slots
            .iter()
            .find(|slot| slot.device_id == device.device_id)
            .is_some_and(|slot| files.sequence <= slot.sequence)
        {
            return Ok(());
        }
        snapshot.settings.preview_file_names
    };
    let body_hash = files.bundle.content_hash();
    let content_hash =
        incoming_content_hash("files", &files.captured_at, &files.group_ids, &body_hash);
    if !state.accept_incoming_sequence(&device.device_id, files.sequence, &content_hash)? {
        return Ok(());
    }
    let slot_id = format!("device:{}", device.device_id);
    state.clear_accepted_file_transfer(&device.device_id);
    state
        .remote_bodies
        .lock()
        .map_err(|_| "远端正文缓存锁已损坏".to_string())?
        .insert(
            slot_id,
            RemoteClipboardBody::Files {
                sequence: files.sequence,
                bundle: files.bundle.clone(),
            },
        );
    update(state, app, |snapshot| {
        snapshot
            .slots
            .retain(|slot| slot.device_id != device.device_id);
        snapshot.slots.push(file_slot(
            device,
            &files,
            preview_names,
            SlotGroups {
                names: group_names,
                ids: files.group_ids.clone(),
            },
        ));
        snapshot.last_synchronized_at = files.captured_at;
        Ok(())
    })?;
    state.clear_cached_text_best_effort(&device.device_id);
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
        .authorized_device(&device.device_id)?
        .is_some_and(|trusted| trusted.sync_enabled)
    {
        return Err("该设备的剪贴板同步已停用".into());
    }
    let group_names =
        state.validate_group_delivery(&device.device_id, &image.group_ids, "image")?;
    let preview_images = {
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
        snapshot.settings.preview_images
    };
    let body_hash = image_hash(&image.rgba, image.width, image.height);
    let content_hash =
        incoming_content_hash("image", &image.captured_at, &image.group_ids, &body_hash);
    if !state.accept_incoming_sequence(&device.device_id, image.sequence, &content_hash)? {
        return Ok(());
    }
    let slot_id = format!("device:{}", device.device_id);
    state.clear_accepted_file_transfer(&device.device_id);
    state
        .remote_bodies
        .lock()
        .map_err(|_| "远端正文缓存锁已损坏".to_string())?
        .insert(
            slot_id,
            RemoteClipboardBody::Image {
                sequence: image.sequence,
                rgba: image.rgba.clone(),
                width: image.width,
                height: image.height,
            },
        );
    update(state, app, |snapshot| {
        snapshot
            .slots
            .retain(|slot| slot.device_id != device.device_id);
        snapshot.slots.push(image_slot(
            device,
            image.sequence,
            &image.rgba,
            (image.width, image.height),
            preview_images,
            image.captured_at.clone(),
            SlotGroups {
                names: group_names,
                ids: image.group_ids.clone(),
            },
        ));
        snapshot.last_synchronized_at = image.captured_at;
        Ok(())
    })?;
    state.clear_cached_text_best_effort(&device.device_id);
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

fn upsert_group_snapshot(
    state: &ServiceState,
    app: &AppHandle,
    manifest: &SignedGroupManifest,
) -> Result<(), String> {
    let display_names = state.displayed_device_names()?;
    let mut view = group_view(manifest, state.device_id());
    apply_group_display_names(&mut view, &display_names);
    update(state, app, |snapshot| {
        snapshot
            .sync_groups
            .retain(|group| group.group_id != view.group_id);
        snapshot.sync_groups.push(view);
        snapshot
            .sync_groups
            .sort_by(|left, right| left.name.cmp(&right.name));
        Ok(())
    })?;
    Ok(())
}

fn reconcile_group_slots(state: &ServiceState, app: &AppHandle) -> Result<(), String> {
    let slots = state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?
        .slots
        .clone();
    let mut retained = HashMap::<String, (Vec<String>, Vec<String>, bool)>::new();
    for slot in &slots {
        let is_rich = slot
            .representations
            .iter()
            .any(|representation| representation.kind == "html");
        let fallback_type = if slot
            .representations
            .iter()
            .any(|representation| representation.kind == "url")
        {
            "url"
        } else {
            "text"
        };
        let content_type = if slot
            .representations
            .iter()
            .any(|representation| representation.kind == "files")
        {
            "files"
        } else if slot
            .representations
            .iter()
            .any(|representation| representation.kind == "image")
        {
            "image"
        } else if is_rich {
            "html"
        } else {
            fallback_type
        };
        let mut valid_ids = Vec::new();
        let mut valid_names = Vec::new();
        let mut text_ids = Vec::new();
        let mut text_names = Vec::new();
        for group_id in &slot.group_ids {
            if let Ok(names) = state.validate_group_delivery(
                &slot.device_id,
                std::slice::from_ref(group_id),
                content_type,
            ) {
                valid_ids.push(group_id.clone());
                valid_names.extend(names);
            }
            if is_rich {
                if let Ok(names) = state.validate_group_delivery(
                    &slot.device_id,
                    std::slice::from_ref(group_id),
                    fallback_type,
                ) {
                    text_ids.push(group_id.clone());
                    text_names.extend(names);
                }
            }
        }
        let downgrade_to_text = is_rich && valid_ids.is_empty() && !text_ids.is_empty();
        if is_rich {
            if downgrade_to_text {
                valid_ids = text_ids;
                valid_names = text_names;
            } else {
                valid_ids.extend(text_ids);
                valid_names.extend(text_names);
                valid_ids.sort();
                valid_ids.dedup();
            }
        }
        valid_names.sort();
        valid_names.dedup();
        if !valid_ids.is_empty() {
            retained.insert(slot.id.clone(), (valid_ids, valid_names, downgrade_to_text));
        }
    }
    let removed = slots
        .iter()
        .filter(|slot| !retained.contains_key(&slot.id))
        .map(|slot| (slot.id.clone(), slot.device_id.clone()))
        .collect::<Vec<_>>();
    let mut downgraded_sizes = HashMap::<String, u64>::new();
    {
        let mut bodies = state
            .remote_bodies
            .lock()
            .map_err(|_| "远端正文缓存锁已损坏".to_string())?;
        let downgrade_ids = retained
            .iter()
            .filter(|(_, (_, _, downgrade))| *downgrade)
            .map(|(slot_id, _)| slot_id.clone())
            .collect::<Vec<_>>();
        for slot_id in downgrade_ids {
            if let Some(RemoteClipboardBody::Rich { sequence, text, .. }) =
                bodies.get(&slot_id).cloned()
            {
                downgraded_sizes.insert(slot_id.clone(), text.len() as u64);
                bodies.insert(slot_id, RemoteClipboardBody::Text { sequence, text });
            }
        }
        for (slot_id, _) in &removed {
            bodies.remove(slot_id);
        }
    }
    for (_, device_id) in &removed {
        state.clear_accepted_file_transfer(device_id);
        if let Some(object_name) = state.store.remove_cached_slot(device_id)? {
            state.clipboard_cache.remove(&object_name);
        }
    }
    update(state, app, |snapshot| {
        snapshot
            .slots
            .retain(|slot| retained.contains_key(&slot.id));
        for slot in &mut snapshot.slots {
            if let Some((group_ids, names, downgrade_to_text)) = retained.get(&slot.id) {
                slot.group_ids = group_ids.clone();
                slot.groups = names.clone();
                if *downgrade_to_text {
                    let fallback_type = if slot
                        .representations
                        .iter()
                        .any(|representation| representation.kind == "url")
                    {
                        "url"
                    } else {
                        "text"
                    };
                    let size = downgraded_sizes.get(&slot.id).copied().unwrap_or(slot.size);
                    let (id, kind, label, mime) = if fallback_type == "url" {
                        ("text/uri-list", "url", "URL", "text/uri-list;charset=utf-8")
                    } else {
                        ("text/plain", "text", "纯文本", "text/plain;charset=utf-8")
                    };
                    slot.size = size;
                    slot.representations = vec![ClipboardRepresentation {
                        id: id.into(),
                        kind: kind.into(),
                        label: label.into(),
                        mime: mime.into(),
                        size,
                        status: "ready".into(),
                        enabled: true,
                    }];
                }
            }
        }
        Ok(())
    })?;
    Ok(())
}

#[tauri::command]
pub fn create_sync_group(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    input: CreateSyncGroupInput,
) -> Result<String, String> {
    let mut member_device_ids = input.member_device_ids;
    member_device_ids.sort();
    member_device_ids.dedup();
    if member_device_ids.is_empty() || member_device_ids.len() + 1 > MAX_GROUP_MEMBERS {
        return Err("请选择 1 到 15 台可信设备".into());
    }
    let now = current_time();
    let owner = GroupMember {
        device_id: state.device_id().into(),
        device_name: state.device_name()?,
        platform: crate::platform::platform_name().into(),
        public_key: BASE64.encode(&state.identity().public_key_bytes()),
        certificate: BASE64.encode(transport.certificate_der()),
        joined_at: now.clone(),
        state: MemberState::Active,
        direction: SyncDirection::Bidirectional,
    };
    let mut members = vec![owner];
    for device_id in &member_device_ids {
        let device = state
            .trusted_device(device_id)?
            .filter(|device| device.sync_enabled)
            .ok_or_else(|| format!("设备 {device_id} 尚未直接配对或同步已停用"))?;
        members.push(GroupMember {
            device_id: device.device_id,
            device_name: device.device_name,
            platform: device.platform,
            public_key: BASE64.encode(&device.public_key),
            certificate: BASE64.encode(&device.certificate_der),
            joined_at: now.clone(),
            state: MemberState::Invited,
            direction: SyncDirection::Bidirectional,
        });
    }
    let group_id = uuid::Uuid::new_v4().to_string();
    let signed = SignedGroupManifest::sign(
        GroupManifest {
            encoding_version: GROUP_ENCODING_VERSION,
            group_id: group_id.clone(),
            owner_device_id: state.device_id().into(),
            name: input.name,
            revision: 1,
            membership_epoch: 1,
            policy: GroupPolicy {
                allow_text: input.allow_text,
                allow_images: input.allow_images,
                allow_html: input.allow_html,
                allow_files: input.allow_files,
                offline_ttl_seconds: 24 * 60 * 60,
            },
            members,
        },
        state.identity(),
    )?;
    state.store.save_group_manifest(&signed, "active", &now)?;
    upsert_group_snapshot(&state, &app, &signed)?;
    let expires_at = (time::OffsetDateTime::now_utc() + time::Duration::minutes(10))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| now.clone());
    for device_id in member_device_ids {
        let invite = StoredGroupInvite {
            invite_id: uuid::Uuid::new_v4().to_string(),
            target_device_id: device_id.clone(),
            expires_at: expires_at.clone(),
            status: "queued".into(),
            manifest: signed.clone(),
        };
        state.store.save_group_invite(&invite)?;
        match transport.send_group_invite(
            &device_id,
            invite.invite_id.clone(),
            invite.expires_at.clone(),
            signed.clone(),
        ) {
            Ok(()) => {
                if let Err(error) = state
                    .store
                    .set_group_invite_status(&invite.invite_id, "sent")
                {
                    tracing::warn!(
                        invite_id = %invite.invite_id,
                        error = %error,
                        "group invite delivery state update deferred"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    device_id,
                    error = %error,
                    "group invite queued until trusted connection is available"
                );
                if let Some(nearby) = state.nearby_device(&device_id) {
                    transport.connect_trusted(app.clone(), nearby);
                }
            }
        }
    }
    Ok(group_id)
}

pub(crate) fn receive_group_invite(
    state: &ServiceState,
    app: &AppHandle,
    sender_device_id: &str,
    invite_id: String,
    target_device_id: String,
    expires_at: String,
    manifest: SignedGroupManifest,
) -> Result<(), String> {
    if target_device_id != state.device_id()
        || manifest.manifest.owner_device_id != sender_device_id
        || expires_at <= current_time()
    {
        return Err("同步组邀请目标、来源或有效期无效".into());
    }
    let owner = state
        .trusted_device(sender_device_id)?
        .ok_or_else(|| "同步组邀请必须来自直接配对设备".to_string())?;
    manifest.verify(&owner.public_key)?;
    let target = manifest
        .manifest
        .members
        .iter()
        .find(|member| member.device_id == state.device_id())
        .ok_or_else(|| "同步组邀请清单缺少本机".to_string())?;
    if target.state != MemberState::Invited {
        return Err("同步组邀请中的本机状态无效".into());
    }
    let invite = StoredGroupInvite {
        invite_id,
        target_device_id,
        expires_at,
        status: "pending".into(),
        manifest,
    };
    let already_processed = state
        .store
        .group_invite(&invite.invite_id)?
        .is_some_and(|existing| existing.status == "accepted" || existing.status == "rejected");
    state.store.save_group_invite(&invite)?;
    if already_processed {
        return Ok(());
    }
    let mut view = pending_group_invite_view(&invite);
    if let Some(name) = state.displayed_device_names()?.get(&view.owner_device_id) {
        view.owner_name = name.clone();
    }
    update(state, app, |snapshot| {
        snapshot
            .pending_group_invites
            .retain(|item| item.invite_id != view.invite_id);
        snapshot.pending_group_invites.push(view);
        Ok(())
    })?;
    Ok(())
}

#[tauri::command]
pub fn confirm_group_invite(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    invite_id: String,
    accepted: bool,
) -> Result<(), String> {
    let invite = state
        .store
        .group_invite(&invite_id)?
        .filter(|invite| invite.status == "pending" && invite.expires_at > current_time())
        .ok_or_else(|| "同步组邀请不存在或已过期".to_string())?;
    state
        .store
        .set_group_invite_status(&invite_id, if accepted { "accepted" } else { "rejected" })?;
    update(&state, &app, |snapshot| {
        snapshot
            .pending_group_invites
            .retain(|item| item.invite_id != invite_id);
        Ok(())
    })?;
    if let Err(error) = transport.send_group_accept(
        &invite.manifest.manifest.owner_device_id,
        invite_id,
        invite.manifest.manifest.group_id.clone(),
        accepted,
    ) {
        tracing::warn!(
            owner_device_id = %invite.manifest.manifest.owner_device_id,
            error = %error,
            "group invite response queued until trusted connection is available"
        );
        if let Some(nearby) = state.nearby_device(&invite.manifest.manifest.owner_device_id) {
            transport.connect_trusted(app, nearby);
        }
    }
    Ok(())
}

pub(crate) fn receive_group_accept(
    state: &ServiceState,
    app: &AppHandle,
    transport: &super::transport::TransportHandle,
    sender_device_id: &str,
    invite_id: &str,
    group_id: &str,
    accepted: bool,
) -> Result<(), String> {
    let invite = state
        .store
        .group_invite(invite_id)?
        .filter(|invite| {
            invite.target_device_id == sender_device_id
                && invite.manifest.manifest.group_id == group_id
        })
        .ok_or_else(|| "同步组接受消息没有匹配邀请".to_string())?;
    if (invite.status == "accepted" && accepted) || (invite.status == "rejected" && !accepted) {
        if accepted {
            if let Some(current) = state.store.group_manifest(group_id)? {
                let _ = transport.send_group_manifest(sender_device_id, current);
            }
        }
        return Ok(());
    }
    if invite.status != "sent" {
        return Err("同步组邀请已经处理，不能修改选择".into());
    }
    if !accepted {
        return state.store.set_group_invite_status(invite_id, "rejected");
    }
    let current = state
        .store
        .group_manifest(group_id)?
        .ok_or_else(|| "同步组不存在".to_string())?;
    if current.manifest.owner_device_id != state.device_id() {
        return Err("只有同步组 Owner 可以处理接受消息".into());
    }
    let mut manifest = current.manifest;
    let member = manifest
        .members
        .iter_mut()
        .find(|member| member.device_id == sender_device_id)
        .ok_or_else(|| "同步组清单缺少接受设备".to_string())?;
    member.state = MemberState::Active;
    member.joined_at = current_time();
    manifest.revision = manifest.revision.saturating_add(1);
    manifest.membership_epoch = manifest.membership_epoch.saturating_add(1);
    let signed = SignedGroupManifest::sign(manifest, state.identity())?;
    state
        .store
        .save_group_manifest(&signed, "active", &current_time())?;
    state.store.set_group_invite_status(invite_id, "accepted")?;
    upsert_group_snapshot(state, app, &signed)?;
    for member in &signed.manifest.members {
        if member.state == MemberState::Active && member.device_id != state.device_id() {
            let _ = transport.send_group_manifest(&member.device_id, signed.clone());
        }
    }
    Ok(())
}

pub(crate) fn receive_group_manifest(
    state: &ServiceState,
    app: &AppHandle,
    transport: &super::transport::TransportHandle,
    manifest: SignedGroupManifest,
) -> Result<(), String> {
    let owner_key =
        if let Some(existing) = state.store.group_manifest(&manifest.manifest.group_id)? {
            existing
                .manifest
                .active_member(&existing.manifest.owner_device_id)
                .ok_or_else(|| "已有同步组缺少 Owner".to_string())?
                .public_key
                .clone()
        } else {
            if !state
                .store
                .has_accepted_group_invite(&manifest.manifest.group_id, state.device_id())?
            {
                return Err("本机尚未接受此同步组邀请".into());
            }
            state
                .trusted_device(&manifest.manifest.owner_device_id)?
                .ok_or_else(|| "未知同步组 Owner".to_string())
                .map(|device| BASE64.encode(&device.public_key))?
        };
    let owner_key = BASE64
        .decode(owner_key.as_bytes())
        .map_err(|_| "同步组 Owner 公钥编码无效".to_string())?;
    manifest.verify(&owner_key)?;
    let local_state = manifest
        .manifest
        .members
        .iter()
        .find(|member| member.device_id == state.device_id())
        .map(|member| member.state.clone())
        .ok_or_else(|| "同步组新清单缺少本机".to_string())?;
    if local_state == MemberState::Invited {
        return Err("未接受邀请的清单不能直接激活本机".into());
    }
    let state_label = if local_state == MemberState::Active {
        "active"
    } else {
        "removed"
    };
    if !state
        .store
        .save_group_manifest(&manifest, state_label, &current_time())?
    {
        return Ok(());
    }
    if local_state == MemberState::Removed {
        let removed_name = manifest.manifest.name.clone();
        update(state, app, |snapshot| {
            snapshot
                .sync_groups
                .retain(|group| group.group_id != manifest.manifest.group_id);
            for slot in &mut snapshot.slots {
                slot.groups.retain(|group| group != &removed_name);
            }
            snapshot.slots.retain(|slot| !slot.groups.is_empty());
            Ok(())
        })?;
        reconcile_group_slots(state, app)?;
        return Ok(());
    }
    upsert_group_snapshot(state, app, &manifest)?;
    reconcile_group_slots(state, app)?;
    for member in &manifest.manifest.members {
        if member.state != MemberState::Active || member.device_id == state.device_id() {
            continue;
        }
        if let Some(nearby) = state.nearby_device(&member.device_id) {
            transport.connect_trusted(app.clone(), nearby);
        }
    }
    Ok(())
}

fn publish_manifest_to_members(
    state: &ServiceState,
    transport: &super::transport::TransportHandle,
    manifest: &SignedGroupManifest,
    include_removed: Option<&str>,
) {
    for member in &manifest.manifest.members {
        if member.device_id == state.device_id() {
            continue;
        }
        if member.state == MemberState::Active
            || include_removed.is_some_and(|device_id| device_id == member.device_id)
        {
            let _ = transport.send_group_manifest(&member.device_id, manifest.clone());
        }
    }
}

#[tauri::command]
pub fn set_group_member_direction(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    group_id: String,
    device_id: String,
    direction: String,
) -> Result<(), String> {
    let direction = match direction.as_str() {
        "disabled" => SyncDirection::Disabled,
        "send_only" => SyncDirection::SendOnly,
        "receive_only" => SyncDirection::ReceiveOnly,
        "bidirectional" => SyncDirection::Bidirectional,
        _ => return Err("未知同步方向".into()),
    };
    let current = state
        .store
        .group_manifest(&group_id)?
        .ok_or_else(|| "同步组不存在".to_string())?;
    if current.manifest.owner_device_id != state.device_id() {
        return Err("只有同步组 Owner 可以修改成员方向".into());
    }
    let mut manifest = current.manifest;
    let member = manifest
        .members
        .iter_mut()
        .find(|member| member.device_id == device_id && member.state == MemberState::Active)
        .ok_or_else(|| "活动成员不存在".to_string())?;
    member.direction = direction;
    manifest.revision = manifest.revision.saturating_add(1);
    manifest.membership_epoch = manifest.membership_epoch.saturating_add(1);
    let signed = SignedGroupManifest::sign(manifest, state.identity())?;
    state
        .store
        .save_group_manifest(&signed, "active", &current_time())?;
    upsert_group_snapshot(&state, &app, &signed)?;
    reconcile_group_slots(&state, &app)?;
    publish_manifest_to_members(&state, &transport, &signed, None);
    Ok(())
}

#[tauri::command]
pub fn remove_group_member(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    group_id: String,
    device_id: String,
) -> Result<(), String> {
    let current = state
        .store
        .group_manifest(&group_id)?
        .ok_or_else(|| "同步组不存在".to_string())?;
    if current.manifest.owner_device_id != state.device_id() {
        return Err("只有同步组 Owner 可以移除成员".into());
    }
    if device_id == state.device_id() {
        return Err("Owner 不能把自己移出同步组".into());
    }
    let mut manifest = current.manifest;
    let member = manifest
        .members
        .iter_mut()
        .find(|member| member.device_id == device_id && member.state != MemberState::Removed)
        .ok_or_else(|| "同步组成员不存在".to_string())?;
    member.state = MemberState::Removed;
    member.direction = SyncDirection::Disabled;
    manifest.revision = manifest.revision.saturating_add(1);
    manifest.membership_epoch = manifest.membership_epoch.saturating_add(1);
    let signed = SignedGroupManifest::sign(manifest, state.identity())?;
    state
        .store
        .save_group_manifest(&signed, "active", &current_time())?;
    upsert_group_snapshot(&state, &app, &signed)?;
    reconcile_group_slots(&state, &app)?;
    publish_manifest_to_members(&state, &transport, &signed, Some(&device_id));
    Ok(())
}

#[tauri::command]
pub fn update_group_policy(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    input: UpdateGroupPolicyInput,
) -> Result<(), String> {
    let current = state
        .store
        .group_manifest(&input.group_id)?
        .ok_or_else(|| "同步组不存在".to_string())?;
    if current.manifest.owner_device_id != state.device_id() {
        return Err("只有同步组 Owner 可以修改组策略".into());
    }
    let mut manifest = current.manifest;
    manifest.policy.allow_text = input.allow_text;
    manifest.policy.allow_images = input.allow_images;
    manifest.policy.allow_html = input.allow_html;
    manifest.policy.allow_files = input.allow_files;
    manifest.revision = manifest.revision.saturating_add(1);
    manifest.membership_epoch = manifest.membership_epoch.saturating_add(1);
    let signed = SignedGroupManifest::sign(manifest, state.identity())?;
    state
        .store
        .save_group_manifest(&signed, "active", &current_time())?;
    upsert_group_snapshot(&state, &app, &signed)?;
    reconcile_group_slots(&state, &app)?;
    publish_manifest_to_members(&state, &transport, &signed, None);
    Ok(())
}

#[tauri::command]
pub fn leave_sync_group(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    group_id: String,
) -> Result<(), String> {
    let group = state
        .store
        .group_manifest(&group_id)?
        .ok_or_else(|| "同步组不存在".to_string())?;
    if group.manifest.owner_device_id == state.device_id() {
        return Err("Owner 需要结束同步组，不能执行普通退出".into());
    }
    let leave = StoredGroupLeave {
        group_id: group_id.clone(),
        member_device_id: state.device_id().into(),
        owner_device_id: group.manifest.owner_device_id.clone(),
        leave_id: uuid::Uuid::new_v4().to_string(),
        status: "pending".into(),
    };
    state.store.save_group_leave(&leave)?;
    state
        .store
        .set_group_local_state(&group_id, "left", &current_time())?;
    update(&state, &app, |snapshot| {
        snapshot
            .sync_groups
            .retain(|group| group.group_id != group_id);
        Ok(())
    })?;
    reconcile_group_slots(&state, &app)?;
    let _ = transport.send_group_leave(&leave.owner_device_id, leave.group_id, leave.leave_id);
    Ok(())
}

pub(crate) fn receive_group_leave(
    state: &ServiceState,
    app: &AppHandle,
    transport: &super::transport::TransportHandle,
    sender_device_id: &str,
    group_id: &str,
    leave_id: &str,
) -> Result<(), String> {
    let current = state
        .store
        .group_manifest(group_id)?
        .ok_or_else(|| "同步组不存在".to_string())?;
    if current.manifest.owner_device_id != state.device_id() {
        return Err("本机不是同步组 Owner".into());
    }
    if current.manifest.active_member(sender_device_id).is_none() {
        return Err("退出通知来源不是活动成员".into());
    }
    let leave = StoredGroupLeave {
        group_id: group_id.into(),
        member_device_id: sender_device_id.into(),
        owner_device_id: state.device_id().into(),
        leave_id: leave_id.into(),
        status: "received".into(),
    };
    if !state.store.save_group_leave(&leave)? {
        return Ok(());
    }
    let mut manifest = current.manifest;
    let member = manifest
        .members
        .iter_mut()
        .find(|member| member.device_id == sender_device_id)
        .ok_or_else(|| "同步组清单缺少退出成员".to_string())?;
    member.state = MemberState::Removed;
    member.direction = SyncDirection::Disabled;
    manifest.revision = manifest.revision.saturating_add(1);
    manifest.membership_epoch = manifest.membership_epoch.saturating_add(1);
    let signed = SignedGroupManifest::sign(manifest, state.identity())?;
    state
        .store
        .save_group_manifest(&signed, "active", &current_time())?;
    state
        .store
        .set_group_leave_status(group_id, sender_device_id, "processed")?;
    upsert_group_snapshot(state, app, &signed)?;
    reconcile_group_slots(state, app)?;
    publish_manifest_to_members(state, transport, &signed, Some(sender_device_id));
    Ok(())
}

#[tauri::command]
pub fn delete_sync_group(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
    group_id: String,
) -> Result<(), String> {
    let group = state
        .store
        .group_manifest(&group_id)?
        .ok_or_else(|| "同步组不存在".to_string())?;
    if group.manifest.owner_device_id != state.device_id() {
        return Err("只有同步组 Owner 可以结束同步组".into());
    }
    let recipients = group
        .manifest
        .members
        .iter()
        .filter(|member| {
            member.state == MemberState::Active && member.device_id != state.device_id()
        })
        .map(|member| member.device_id.clone())
        .collect::<Vec<_>>();
    let tombstone = SignedGroupTombstone::sign(
        GroupTombstone {
            encoding_version: GROUP_ENCODING_VERSION,
            group_id: group_id.clone(),
            owner_device_id: state.device_id().into(),
            revision: group.manifest.revision.saturating_add(1),
            membership_epoch: group.manifest.membership_epoch.saturating_add(1),
            deleted_at: current_time(),
        },
        state.identity(),
    )?;
    state.store.save_group_tombstone(&tombstone)?;
    update(&state, &app, |snapshot| {
        snapshot
            .sync_groups
            .retain(|group| group.group_id != group_id);
        Ok(())
    })?;
    reconcile_group_slots(&state, &app)?;
    for device_id in recipients {
        let _ = transport.send_group_tombstone(&device_id, tombstone.clone());
    }
    Ok(())
}

pub(crate) fn receive_group_tombstone(
    state: &ServiceState,
    app: &AppHandle,
    tombstone: SignedGroupTombstone,
) -> Result<(), String> {
    let group = state
        .store
        .group_manifest_any(&tombstone.tombstone.group_id)?
        .ok_or_else(|| "同步组删除声明没有可信历史清单".to_string())?;
    if group.manifest.owner_device_id != tombstone.tombstone.owner_device_id
        || tombstone.tombstone.revision <= group.manifest.revision
    {
        return Err("同步组删除声明 Owner 或版本无效".into());
    }
    let owner_key = group
        .manifest
        .members
        .iter()
        .find(|member| member.device_id == group.manifest.owner_device_id)
        .ok_or_else(|| "同步组历史清单缺少 Owner".to_string())?
        .public_key
        .clone();
    let owner_key = BASE64
        .decode(owner_key.as_bytes())
        .map_err(|_| "同步组 Owner 公钥编码无效".to_string())?;
    tombstone.verify(&owner_key)?;
    if !state.store.save_group_tombstone(&tombstone)? {
        return Ok(());
    }
    update(state, app, |snapshot| {
        snapshot
            .sync_groups
            .retain(|group| group.group_id != tombstone.tombstone.group_id);
        Ok(())
    })?;
    reconcile_group_slots(state, app)?;
    Ok(())
}

#[tauri::command]
pub fn allow_pairing(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    app: AppHandle,
) -> Result<(), String> {
    let expiry = transport.allow_pairing(120)?;
    update(&state, &app, |snapshot| {
        snapshot.pairing_allowed_until = Some(expiry);
        Ok(())
    })?;
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
    transport.connect_pairing(app, nearby)
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
        state.clear_accepted_file_transfer(&device_id);
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
pub fn set_local_device_name(
    state: State<'_, ServiceState>,
    transport: State<'_, super::transport::TransportHandle>,
    discovery: State<'_, super::discovery::DiscoveryHandle>,
    app: AppHandle,
    device_name: String,
) -> Result<(), String> {
    let device_name = normalize_device_label(&device_name, "本机名称")?;
    if state.device_name()? == device_name {
        return Ok(());
    }
    state.store.save_device_name(&device_name)?;
    *state
        .device_name
        .lock()
        .map_err(|_| "本机设备名称状态锁已损坏".to_string())? = device_name.clone();
    update(&state, &app, |snapshot| {
        snapshot.local_device_name = device_name.clone();
        for group in &mut snapshot.sync_groups {
            for member in group
                .members
                .iter_mut()
                .filter(|member| member.device_id == state.device_id())
            {
                member.device_name = device_name.clone();
            }
        }
        Ok(())
    })?;
    transport.refresh_local_profile();
    discovery.refresh(app)?;
    Ok(())
}

#[tauri::command]
pub fn set_device_alias(
    state: State<'_, ServiceState>,
    app: AppHandle,
    device_id: String,
    local_alias: Option<String>,
) -> Result<(), String> {
    let local_alias = local_alias
        .as_deref()
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .map(|alias| normalize_device_label(alias, "设备备注名"))
        .transpose()?;
    state
        .store
        .set_device_local_alias(&device_id, local_alias.as_deref())?;
    update(&state, &app, |snapshot| {
        let advertised_name = snapshot
            .trusted_devices
            .iter()
            .find(|device| device.device_id == device_id)
            .map(|device| device.advertised_name.clone())
            .ok_or_else(|| "可信设备不存在".to_string())?;
        let display_name = local_alias
            .as_deref()
            .unwrap_or(&advertised_name)
            .to_string();
        if let Some(device) = snapshot
            .trusted_devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        {
            device.device_name = display_name.clone();
            device.local_alias = local_alias.clone();
        }
        for nearby in snapshot
            .nearby_devices
            .iter_mut()
            .filter(|device| device.device_id == device_id && device.paired)
        {
            nearby.device_name = display_name.clone();
        }
        for slot in snapshot
            .slots
            .iter_mut()
            .filter(|slot| slot.device_id == device_id)
        {
            slot.device_name = display_name.clone();
        }
        for group in &mut snapshot.sync_groups {
            for member in group
                .members
                .iter_mut()
                .filter(|member| member.device_id == device_id)
            {
                member.device_name = display_name.clone();
            }
        }
        for invite in snapshot
            .pending_group_invites
            .iter_mut()
            .filter(|invite| invite.owner_device_id == device_id)
        {
            invite.owner_name = display_name.clone();
        }
        for operation in snapshot
            .imports
            .iter_mut()
            .filter(|operation| operation.slot_id == format!("device:{device_id}"))
        {
            operation.device_name = display_name.clone();
        }
        snapshot
            .trusted_devices
            .sort_by(|left, right| left.device_name.cmp(&right.device_name));
        Ok(())
    })?;
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
    let owner_group_ids = state
        .store
        .group_manifests()?
        .into_iter()
        .filter(|group| {
            group.manifest.owner_device_id == device_id
                && group.manifest.owner_device_id != state.device_id()
        })
        .map(|group| group.manifest.group_id)
        .collect::<Vec<_>>();
    if let Some(object_name) = state.store.remove_cached_slot(&device_id)? {
        state.clipboard_cache.remove(&object_name);
    }
    state.store.revoke_device(&device_id, &revoked_at)?;
    for group_id in &owner_group_ids {
        state
            .store
            .set_group_local_state(group_id, "left", &revoked_at)?;
    }
    transport.disable_peer(&device_id);
    state.clear_accepted_file_transfer(&device_id);
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
        snapshot
            .sync_groups
            .retain(|group| !owner_group_ids.contains(&group.group_id));
        if let Some(nearby) = snapshot
            .nearby_devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        {
            nearby.paired = false;
        }
        Ok(())
    })?;
    reconcile_group_slots(&state, &app)?;
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
    let _ = platform;
    snapshot.platform = if cfg!(target_os = "android") {
        "android".into()
    } else {
        "desktop".into()
    };
    snapshot
        .pending_pairings
        .retain(|pairing| pairing.expires_at.as_str() > now.as_str());
    if snapshot
        .pairing_allowed_until
        .is_some_and(|expiry| expiry <= unix_seconds())
    {
        snapshot.pairing_allowed_until = None;
    }
    if snapshot.last_synchronized_at.starts_with("1970-") {
        snapshot.last_synchronized_at = now.clone();
        snapshot.current_clipboard.changed_at = now;
    }
    Ok(snapshot.clone())
}

#[tauri::command]
pub fn copy_diagnostic_report(
    state: State<'_, ServiceState>,
    app: AppHandle,
    report: String,
) -> Result<(), String> {
    if report.trim().is_empty() || report.len() > 64 * 1024 {
        return Err("诊断摘要为空或超过 64 KiB".into());
    }
    *state
        .suppress_next_capture
        .lock()
        .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())? = Some(report.clone());
    if let Err(error) = retry_clipboard_write(|| {
        app.clipboard()
            .write_text(&report)
            .map_err(|error| error.to_string())
    }) {
        *state
            .suppress_next_capture
            .lock()
            .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())? = None;
        return Err(format!("无法复制诊断摘要：{error}"));
    }
    Ok(())
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
    if cfg!(target_os = "android") {
        return Ok(());
    }
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

#[cfg(mobile)]
pub(crate) fn set_mobile_activity(
    state: &ServiceState,
    app: &AppHandle,
    activity: &str,
) -> Result<(), String> {
    if !matches!(activity, "foreground_live" | "reconnecting" | "suspended") {
        return Err("未知移动端运行状态".into());
    }
    update(state, app, |snapshot| {
        snapshot.platform = "android".into();
        snapshot.activity = activity.into();
        if activity == "suspended" {
            snapshot.pairing_allowed_until = None;
            for device in &mut snapshot.trusted_devices {
                device.online = false;
            }
            for slot in &mut snapshot.slots {
                slot.online = false;
            }
        } else if activity == "foreground_live" {
            snapshot.last_synchronized_at = current_time();
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
pub async fn publish_current_clipboard(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let content = crate::platform::read_system_clipboard(&app)?;
        let now = current_time();
        let state = app.state::<ServiceState>();
        let mut captured = false;

        if !content.files.is_empty() {
            capture_local_files(&state, &app, content.files, now.clone())?;
            captured = true;
        } else {
            if let Some((text, html, rtf)) = content.rich {
                capture_local_rich(&state, &app, text, html, rtf, now.clone())?;
                captured = true;
            } else if let Some(text) = content.text {
                capture_local_clipboard(&state, &app, text, now.clone())?;
                captured = true;
            }
            if let Some((rgba, width, height)) = content.image {
                capture_local_image(&state, &app, rgba, width, height, now)?;
                captured = true;
            }
        }

        if captured {
            Ok(())
        } else {
            Err("当前剪贴板没有可同步的文本、图片、富文本或文件".into())
        }
    })
    .await
    .map_err(|error| format!("剪贴板读取任务失败：{error}"))?
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, ServiceState>,
    app: AppHandle,
    settings: Value,
) -> Result<(), String> {
    let preview_policy_changed = settings.as_object().is_some_and(|patch| {
        ["previewText", "previewImages", "previewFileNames"]
            .iter()
            .any(|key| patch.contains_key(*key))
    });
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
            if key == "globalShortcut" {
                continue;
            }
            current_object.insert(key.clone(), value.clone());
        }
        snapshot.settings =
            serde_json::from_value(current).map_err(|error| format!("设置值无效：{error}"))?;
        if cfg!(target_os = "android") {
            snapshot.settings.allow_html = false;
            snapshot.settings.allow_images = false;
            snapshot.settings.allow_files = false;
            snapshot.settings.allow_private = false;
        }
        if !snapshot.settings.allow_text {
            snapshot.slots.retain(|slot| {
                let has_text = slot
                    .representations
                    .iter()
                    .any(|representation| representation.kind == "text");
                let has_rich = slot
                    .representations
                    .iter()
                    .any(|representation| representation.kind == "html");
                !has_text || has_rich
            });
            snapshot.imports.clear();
        }
        if !snapshot.settings.allow_urls {
            snapshot.slots.retain(|slot| {
                let has_url = slot
                    .representations
                    .iter()
                    .any(|representation| representation.kind == "url");
                let has_rich = slot
                    .representations
                    .iter()
                    .any(|representation| representation.kind == "html");
                !has_url || has_rich
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
        if !snapshot.settings.allow_html {
            snapshot.slots.retain(|slot| {
                !slot
                    .representations
                    .iter()
                    .any(|representation| representation.kind == "html")
            });
            snapshot.imports.clear();
        }
        if !snapshot.settings.allow_files {
            snapshot.slots.retain(|slot| {
                !slot
                    .representations
                    .iter()
                    .any(|representation| representation.kind == "files")
            });
            snapshot.imports.clear();
        }
        redact_disallowed_previews(snapshot);
        Ok(())
    })?;
    state.store.save_settings(&snapshot.settings)?;
    if preview_policy_changed {
        refresh_preview_policy(&state, &app)?;
    }
    if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
        transport
            .retain_enabled_latest_text(snapshot.settings.allow_text, snapshot.settings.allow_urls);
    }
    if !snapshot.settings.allow_text || !snapshot.settings.allow_urls {
        for metadata in state.store.cached_slots(unix_seconds())? {
            let remove = state
                .clipboard_cache
                .load(&metadata.device_id, &metadata.object_name)
                .map(|cached| {
                    if text_content_type(&cached.text) == "url" {
                        !snapshot.settings.allow_urls
                    } else {
                        !snapshot.settings.allow_text
                    }
                })
                .unwrap_or(true);
            if remove {
                if let Some(object_name) = state.store.remove_cached_slot(&metadata.device_id)? {
                    state.clipboard_cache.remove(&object_name);
                }
            }
        }
    }
    if !snapshot.settings.allow_images {
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            transport.clear_latest_image();
        }
    }
    if !snapshot.settings.allow_html {
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            transport
                .downgrade_latest_rich(snapshot.settings.allow_text, snapshot.settings.allow_urls);
        }
    }
    if !snapshot.settings.allow_files {
        if let Some(transport) = app.try_state::<super::transport::TransportHandle>() {
            transport.clear_latest_files();
        }
        if let Ok(mut transfers) = state.accepted_file_transfers.lock() {
            transfers.clear();
        }
    }
    if !snapshot.settings.allow_text
        || !snapshot.settings.allow_urls
        || !snapshot.settings.allow_images
        || !snapshot.settings.allow_html
        || !snapshot.settings.allow_files
    {
        state
            .remote_bodies
            .lock()
            .map_err(|_| "远端正文缓存锁已损坏".to_string())?
            .retain(|_, body| match body {
                RemoteClipboardBody::Text { text, .. } => {
                    if text_content_type(text) == "url" {
                        snapshot.settings.allow_urls
                    } else {
                        snapshot.settings.allow_text
                    }
                }
                RemoteClipboardBody::Rich { .. } => snapshot.settings.allow_html,
                RemoteClipboardBody::Files { .. } => snapshot.settings.allow_files,
                RemoteClipboardBody::Image { .. } => snapshot.settings.allow_images,
            });
    }
    Ok(())
}

#[tauri::command]
pub fn set_global_shortcut(
    state: State<'_, ServiceState>,
    app: AppHandle,
    shortcut: String,
) -> Result<(), String> {
    #[cfg(not(desktop))]
    {
        let _ = (state, app, shortcut);
        Err("移动端不支持桌面全局快捷键".into())
    }
    #[cfg(desktop)]
    {
        let shortcut = shortcut.trim();
        if !shortcut.contains('+') {
            return Err("快捷键必须包含 Ctrl、Alt、Shift 或 Super 修饰键".into());
        }
        let old_shortcut = state.configured_global_shortcut()?;
        if shortcut == old_shortcut {
            if !app.global_shortcut().is_registered(shortcut) {
                app.global_shortcut()
                    .register(shortcut)
                    .map_err(|error| format!("快捷键不可用或已被占用：{error}"))?;
            }
            return Ok(());
        }
        app.global_shortcut()
            .register(shortcut)
            .map_err(|error| format!("快捷键不可用或已被占用：{error}"))?;
        let old_was_registered = app.global_shortcut().is_registered(old_shortcut.as_str());
        if old_was_registered {
            if let Err(error) = app.global_shortcut().unregister(old_shortcut.as_str()) {
                let _ = app.global_shortcut().unregister(shortcut);
                return Err(format!("无法替换原快捷键：{error}"));
            }
        }

        let mut next_settings = {
            let snapshot = state
                .snapshot
                .lock()
                .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
            snapshot.settings.clone()
        };
        next_settings.global_shortcut = shortcut.to_string();
        if let Err(error) = state.store.save_settings(&next_settings) {
            let _ = app.global_shortcut().unregister(shortcut);
            if old_was_registered {
                let _ = app.global_shortcut().register(old_shortcut.as_str());
            }
            return Err(error);
        }
        if let Err(error) = update(&state, &app, |snapshot| {
            snapshot.settings.global_shortcut = shortcut.to_string();
            Ok(())
        }) {
            let _ = app.global_shortcut().unregister(shortcut);
            if old_was_registered {
                let _ = app.global_shortcut().register(old_shortcut.as_str());
            }
            let mut restored = next_settings;
            restored.global_shortcut = old_shortcut;
            let _ = state.store.save_settings(&restored);
            return Err(error);
        }
        Ok(())
    }
}

#[tauri::command]
pub fn create_import_intent(
    state: State<'_, ServiceState>,
    app: AppHandle,
    slot_id: String,
    revision: u64,
) -> Result<String, String> {
    let (_, slot, _) = validated_remote_slot(&state, &slot_id, revision)?;
    let mut snapshot = state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
    let import_id = uuid::Uuid::new_v4().simple().to_string();
    snapshot.imports.push(ImportOperation {
        id: import_id.clone(),
        slot_id: slot.id.clone(),
        revision,
        device_name: slot.device_name.clone(),
        source_summary: truncate_text(&slot.preview, 80),
        status: "awaiting_confirmation".into(),
        progress: 100,
        message: Some("确认后才会写入本机系统剪贴板".into()),
    });
    snapshot.bump();
    let emitted = snapshot.clone();
    drop(snapshot);
    emit_snapshot(&app, &emitted)?;
    Ok(import_id)
}

fn slot_content_type(slot: &DeviceSlot) -> &'static str {
    if slot
        .representations
        .iter()
        .any(|representation| representation.kind == "files")
    {
        "files"
    } else if slot
        .representations
        .iter()
        .any(|representation| representation.kind == "image")
    {
        "image"
    } else if slot
        .representations
        .iter()
        .any(|representation| representation.kind == "html")
    {
        "html"
    } else if slot
        .representations
        .iter()
        .any(|representation| representation.kind == "url")
    {
        "url"
    } else {
        "text"
    }
}

fn validated_remote_slot(
    state: &ServiceState,
    slot_id: &str,
    revision: u64,
) -> Result<(RemoteClipboardBody, DeviceSlot, &'static str), String> {
    let body = state
        .remote_bodies
        .lock()
        .map_err(|_| "远端正文缓存锁已损坏".to_string())?
        .get(slot_id)
        .cloned()
        .ok_or_else(|| "远端正文当前不可用".to_string())?;
    if body.sequence() != revision {
        return Err("设备槽位正文已经更新，请重新选择".into());
    }
    let snapshot = state
        .snapshot
        .lock()
        .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
    let slot = snapshot
        .slots
        .iter()
        .find(|slot| slot.id == slot_id && slot.revision == revision)
        .cloned()
        .ok_or_else(|| "设备槽位不存在或已经更新".to_string())?;
    let content_type = slot_content_type(&slot);
    state.validate_group_delivery(&slot.device_id, &slot.group_ids, content_type)?;
    let allowed = match content_type {
        "image" => snapshot.settings.allow_images,
        "html" => snapshot.settings.allow_html,
        "url" => snapshot.settings.allow_urls,
        "files" => snapshot.settings.allow_files,
        _ => snapshot.settings.allow_text,
    };
    if !allowed {
        return Err("本机策略已停用此内容类型的取用".into());
    }
    drop(snapshot);
    Ok((body, slot, content_type))
}

#[tauri::command]
pub fn prepare_slot_drag(
    state: State<'_, ServiceState>,
    slot_id: String,
    revision: u64,
) -> Result<PreparedSlotDrag, String> {
    let (body, _, content_type) = validated_remote_slot(&state, &slot_id, revision)?;
    let mut lease_id = None;
    let item = match body {
        RemoteClipboardBody::Text { text, .. } => {
            if content_type == "url" {
                let mut data = HashMap::new();
                data.insert("text/plain".into(), text.clone());
                data.insert("text/uri-list".into(), format!("{}\r\n", text.trim()));
                PreparedDragItem::Data {
                    data: PreparedDragData::Map(data),
                    types: vec!["text/plain".into(), "text/uri-list".into()],
                }
            } else {
                PreparedDragItem::Data {
                    data: PreparedDragData::Fixed(text),
                    types: vec!["text/plain".into()],
                }
            }
        }
        RemoteClipboardBody::Rich {
            text, html, rtf, ..
        } => {
            let mut data = HashMap::new();
            let mut types = Vec::new();
            if let Some(html) = html {
                data.insert("text/html".into(), html);
                types.push("text/html".into());
            }
            if let Some(rtf) = rtf {
                data.insert("text/rtf".into(), rtf);
                types.push("text/rtf".into());
            }
            data.insert("text/plain".into(), text);
            types.push("text/plain".into());
            PreparedDragItem::Data {
                data: PreparedDragData::Map(data),
                types,
            }
        }
        RemoteClipboardBody::Files { bundle, .. } => {
            let now = unix_seconds();
            let id = uuid::Uuid::new_v4().simple().to_string();
            let mut leases = state
                .drag_file_leases
                .lock()
                .map_err(|_| "拖放文件租约锁已损坏".to_string())?;
            leases.retain(|_, (_, created_at)| created_at.saturating_add(600) > now);
            leases.insert(id.clone(), (bundle.clone(), now));
            lease_id = Some(id);
            PreparedDragItem::Files(bundle.clipboard_paths())
        }
        RemoteClipboardBody::Image {
            rgba,
            width,
            height,
            ..
        } => PreparedDragItem::Files(vec![prepare_drag_image_file(&state, &rgba, width, height)?]),
    };
    Ok(PreparedSlotDrag { item, lease_id })
}

#[tauri::command]
pub fn release_slot_drag(app: AppHandle, lease_id: String) -> Result<(), String> {
    if lease_id.is_empty() || lease_id.len() > 64 {
        return Err("拖放文件租约无效".into());
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        let state = app.state::<ServiceState>();
        if let Ok(mut leases) = state.drag_file_leases.lock() {
            leases.remove(&lease_id);
        };
    });
    Ok(())
}

#[tauri::command]
pub fn confirm_import(
    state: State<'_, ServiceState>,
    app: AppHandle,
    import_id: String,
) -> Result<(), String> {
    let operation = {
        let snapshot = state
            .snapshot
            .lock()
            .map_err(|_| "Rust 服务状态锁已损坏".to_string())?;
        snapshot
            .imports
            .iter()
            .find(|item| item.id == import_id && item.status == "awaiting_confirmation")
            .cloned()
            .ok_or_else(|| "没有可确认的远端剪贴板导入".to_string())?
    };
    let body = state
        .remote_bodies
        .lock()
        .map_err(|_| "远端正文缓存锁已损坏".to_string())?
        .get(&operation.slot_id)
        .cloned()
        .ok_or_else(|| "远端正文已经不可用".to_string())?;
    if body.sequence() != operation.revision {
        return Err("远端槽位在确认前已经更新，请重新选择".into());
    }
    let slot_id = operation.slot_id;
    let textual = matches!(
        &body,
        RemoteClipboardBody::Text { .. } | RemoteClipboardBody::Rich { .. }
    );
    let image_body = matches!(&body, RemoteClipboardBody::Image { .. });
    let file_body = matches!(&body, RemoteClipboardBody::Files { .. });
    let (preview, types, image_preview, file_names) = match &body {
        RemoteClipboardBody::Text { text, .. } => {
            *state
                .suppress_next_capture
                .lock()
                .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())? = Some(text.clone());
            if let Err(error) = retry_clipboard_write(|| {
                app.clipboard()
                    .write_text(text)
                    .map_err(|error| error.to_string())
            }) {
                *state
                    .suppress_next_capture
                    .lock()
                    .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())? = None;
                return Err(format!("无法写入本机系统剪贴板：{error}"));
            }
            (
                truncate_text(text, 4096),
                vec![if text_content_type(text) == "url" {
                    "URL".into()
                } else {
                    "纯文本".into()
                }],
                None,
                None,
            )
        }
        RemoteClipboardBody::Rich {
            text, html, rtf, ..
        } => {
            *state
                .suppress_next_rich
                .lock()
                .map_err(|_| "富文本剪贴板回环抑制锁已损坏".to_string())? =
                Some(rich_hash(text, html.as_deref(), rtf.as_deref()));
            *state
                .suppress_next_capture
                .lock()
                .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())? = Some(text.clone());
            if let Err(error) = retry_clipboard_write(|| {
                crate::platform::write_rich_clipboard(text.clone(), html.clone(), rtf.clone())
            }) {
                *state
                    .suppress_next_rich
                    .lock()
                    .map_err(|_| "富文本剪贴板回环抑制锁已损坏".to_string())? = None;
                *state
                    .suppress_next_capture
                    .lock()
                    .map_err(|_| "剪贴板回环抑制锁已损坏".to_string())? = None;
                return Err(error);
            }
            (
                truncate_text(text, 4096),
                vec!["富文本 / HTML".into(), "纯文本降级".into()],
                None,
                None,
            )
        }
        RemoteClipboardBody::Files { bundle, .. } => {
            let paths = bundle.clipboard_paths();
            *state
                .suppress_next_files
                .lock()
                .map_err(|_| "文件剪贴板回环抑制锁已损坏".to_string())? =
                Some(file_list_hash(&paths));
            if let Err(error) =
                retry_clipboard_write(|| crate::platform::write_file_clipboard(paths.clone()))
            {
                *state
                    .suppress_next_files
                    .lock()
                    .map_err(|_| "文件剪贴板回环抑制锁已损坏".to_string())? = None;
                return Err(error);
            }
            *state
                .imported_files
                .lock()
                .map_err(|_| "已导入文件保留锁已损坏".to_string())? = Some(bundle.clone());
            (
                format!("{} 个文件或目录", bundle.clipboard_paths().len()),
                vec!["文件与目录".into()],
                None,
                Some(bundle.display_names()),
            )
        }
        RemoteClipboardBody::Image {
            rgba,
            width,
            height,
            ..
        } => {
            *state
                .suppress_next_image
                .lock()
                .map_err(|_| "图片剪贴板回环抑制锁已损坏".to_string())? =
                Some(image_hash(rgba, *width, *height));
            let image = tauri::image::Image::new_owned(rgba.clone(), *width, *height);
            if let Err(error) = retry_clipboard_write(|| {
                app.clipboard()
                    .write_image(&image)
                    .map_err(|error| error.to_string())
            }) {
                *state
                    .suppress_next_image
                    .lock()
                    .map_err(|_| "图片剪贴板回环抑制锁已损坏".to_string())? = None;
                return Err(format!("无法写入本机图片剪贴板：{error}"));
            }
            (
                format!("图片 · {width} × {height}"),
                vec!["图片".into()],
                image_preview_data_url(rgba, *width, *height),
                None,
            )
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
                preview: if textual && !snapshot.settings.preview_text {
                    "文本预览已隐藏".into()
                } else {
                    preview.clone()
                },
                image_preview: if image_body && snapshot.settings.preview_images {
                    image_preview.clone()
                } else {
                    None
                },
                file_names: if file_body && snapshot.settings.preview_file_names {
                    file_names.clone()
                } else {
                    None
                },
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn classifies_standalone_network_urls_without_treating_paths_as_urls() {
        assert_eq!(text_content_type("https://example.com/path"), "url");
        assert_eq!(text_content_type("mailto:user@example.com"), "url");
        assert_eq!(text_content_type("file:///tmp/example.txt"), "text");
        assert_eq!(text_content_type("See https://example.com"), "text");
        assert_eq!(text_content_type("C:\\Temp\\example.txt"), "text");
    }

    #[test]
    fn incoming_sequences_only_advance_monotonically() {
        let directory = std::env::temp_dir().join(format!(
            "airdrop-sequence-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let hash_7 = [7_u8; 32];
        let changed_hash_7 = [17_u8; 32];
        let hash_8 = [8_u8; 32];
        let state = ServiceState::open(&directory).unwrap();
        assert!(state
            .accept_incoming_sequence("ld1_peer", 7, &hash_7)
            .unwrap());
        assert_eq!(state.reconciliation_sequence("ld1_peer"), Some(7));
        assert!(!state
            .accept_incoming_sequence("ld1_peer", 7, &hash_7)
            .unwrap());
        drop(state);

        let state = ServiceState::open(&directory).unwrap();
        assert_eq!(state.reconciliation_sequence("ld1_peer"), None);
        assert!(!state
            .accept_incoming_sequence("ld1_peer", 7, &changed_hash_7)
            .unwrap());
        assert!(state
            .accept_incoming_sequence("ld1_peer", 7, &hash_7)
            .unwrap());
        assert_eq!(state.reconciliation_sequence("ld1_peer"), Some(7));
        assert!(!state
            .accept_incoming_sequence("ld1_peer", 7, &hash_7)
            .unwrap());
        assert!(!state
            .accept_incoming_sequence("ld1_peer", 6, &hash_7)
            .unwrap());
        assert!(state
            .accept_incoming_sequence("ld1_peer", 8, &hash_8)
            .unwrap());
        assert_eq!(state.reconciliation_sequence("ld1_peer"), Some(8));
        drop(state);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn text_slot_respects_preview_policy() {
        let device = TrustedDevice {
            device_id: "ld1_peer".into(),
            device_name: "Peer".into(),
            local_alias: None,
            platform: "linux".into(),
            public_key: vec![0; 32],
            certificate_der: vec![1],
            paired_at: "2026-07-14T00:00:00Z".into(),
            sync_enabled: true,
        };
        let hidden = text_slot(
            &device,
            1,
            "secret clipboard text",
            "2026-07-14T00:00:00Z".into(),
            SlotGroups {
                names: vec!["Personal".into()],
                ids: vec!["group".into()],
            },
            SlotPresentation {
                preview_content: false,
                online: true,
                availability: "ready",
            },
        );
        assert_eq!(hidden.preview, "文本预览已隐藏");
        let visible = text_slot(
            &device,
            2,
            "visible clipboard text",
            "2026-07-14T00:00:01Z".into(),
            SlotGroups {
                names: vec!["Personal".into()],
                ids: vec!["group".into()],
            },
            SlotPresentation {
                preview_content: true,
                online: true,
                availability: "ready",
            },
        );
        assert_eq!(visible.preview, "visible clipboard text");
    }

    #[test]
    fn preview_redaction_happens_before_snapshot_emission() {
        let settings = AppSettings {
            preview_text: false,
            preview_images: false,
            preview_file_names: false,
            ..AppSettings::default()
        };
        let mut snapshot = UiSnapshot::initial("Local Device".into(), settings, None, Vec::new());
        snapshot.current_clipboard.preview = "local secret".into();
        snapshot.current_clipboard.types = vec!["纯文本".into()];
        snapshot.current_clipboard.image_preview = Some("data:image/png;base64,secret".into());
        snapshot.current_clipboard.file_names = Some(vec!["secret.txt".into()]);
        snapshot.slots.push(DeviceSlot {
            id: "device:peer".into(),
            revision: 1,
            device_id: "peer".into(),
            device_name: "Peer".into(),
            platform: "linux".into(),
            online: true,
            pinned: None,
            availability: "ready".into(),
            preview: "remote secret".into(),
            image_preview: Some("data:image/png;base64,secret".into()),
            file_names: Some(vec!["secret.txt".into()]),
            captured_at: "2026-07-14T00:00:00Z".into(),
            age_label: "刚刚".into(),
            groups: vec!["Personal".into()],
            group_ids: vec!["group".into()],
            sequence: 1,
            size: 13,
            representations: vec![ClipboardRepresentation {
                id: "text/plain".into(),
                kind: "text".into(),
                label: "纯文本".into(),
                mime: "text/plain;charset=utf-8".into(),
                size: 13,
                status: "ready".into(),
                enabled: true,
            }],
            blocked_reason: None,
            progress: None,
        });
        redact_disallowed_previews(&mut snapshot);
        assert_eq!(snapshot.current_clipboard.preview, "文本预览已隐藏");
        assert!(snapshot.current_clipboard.image_preview.is_none());
        assert!(snapshot.current_clipboard.file_names.is_none());
        assert_eq!(snapshot.slots[0].preview, "文本预览已隐藏");
        assert!(snapshot.slots[0].image_preview.is_none());
        assert!(snapshot.slots[0].file_names.is_none());
    }
}
