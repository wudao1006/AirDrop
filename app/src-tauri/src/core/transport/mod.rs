mod protocol;

use super::{
    discovery::TRANSPORT_PORT,
    files::{
        safe_relative_path, ReceivedFileBundle, StagedFileBundle, MAX_FILE_BUNDLE_BYTES,
        MAX_FILE_ENTRIES,
    },
    group::{SignedGroupManifest, SignedGroupTombstone},
    identity::device_id_for_key,
    service::{self, PendingPairing, ServiceState},
    storage::TrustedDevice,
    telemetry::{TelemetrySnapshot, TelemetryStore},
};
use crate::platform;
use data_encoding::{BASE64, HEXLOWER};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
use protocol::{
    read_frame, write_frame, ClipboardCapabilities, FileBlobHeader, FileResumePlan,
    FileTransferAck, ImageBlobHeader, PairMessage, TrustedMessage, PAIR_ALPN, TRUSTED_ALPN,
};
use quinn::{crypto::rustls::QuicClientConfig, Connection, Endpoint, TransportConfig};
use rcgen::{CertificateParams, KeyPair};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
    DigitallySignedStruct, DistinguishedName, SignatureScheme,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

type HmacSha256 = Hmac<Sha256>;
const MAX_IMAGE_BLOB: usize = 16 * 1024 * 1024;
const IMAGE_BLOB_KIND: u8 = 1;
const FILE_BLOB_KIND: u8 = 2;
const CONNECTION_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const DELIVERY_RECEIPT_TIMEOUT: Duration = Duration::from_secs(15);

type PendingDeliveryReceipts = Arc<Mutex<HashMap<String, oneshot::Sender<DeliveryReceipt>>>>;

struct DeliveryReceipt {
    accepted: bool,
    message: Option<String>,
    processing_ms: Option<u64>,
}

struct PairCommandRegistration {
    commands: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<bool>>>>,
    pairing_id: String,
}

struct PeerConnection {
    sender: mpsc::UnboundedSender<TrustedMessage>,
    connection: Connection,
    capabilities: ClipboardCapabilities,
    pending_receipts: PendingDeliveryReceipts,
    blob_semaphore: Arc<tokio::sync::Semaphore>,
}

#[derive(Clone)]
struct LocalImageOffer {
    sequence: u64,
    captured_at: String,
    width: u32,
    height: u32,
    png: Arc<Vec<u8>>,
}

#[derive(Clone)]
struct LocalTextOffer {
    sequence: u64,
    captured_at: String,
    text: String,
    content_type: String,
}

#[derive(Clone)]
struct LocalRichOffer {
    sequence: u64,
    captured_at: String,
    text: String,
    html: Option<String>,
    rtf: Option<String>,
    fallback_type: String,
}

#[derive(Clone)]
struct LocalFileOffer {
    transfer_id: String,
    sequence: u64,
    captured_at: String,
    bundle: Arc<StagedFileBundle>,
}

pub(crate) struct RichDeliveryTargets<'a> {
    pub(crate) rich: &'a HashMap<String, Vec<String>>,
    pub(crate) text: &'a HashMap<String, Vec<String>>,
}

impl Drop for PairCommandRegistration {
    fn drop(&mut self) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.remove(&self.pairing_id);
        }
    }
}

#[derive(Clone)]
pub(crate) struct TransportHandle {
    runtime: tokio::runtime::Handle,
    endpoint: Endpoint,
    certificate_der: Vec<u8>,
    private_key_der: Vec<u8>,
    active: Arc<AtomicBool>,
    runtime_generation: Arc<AtomicU64>,
    pairing_allowed_until: Arc<Mutex<u64>>,
    pair_commands: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<bool>>>>,
    pairing_connecting: Arc<Mutex<HashMap<String, u64>>>,
    peers: Arc<Mutex<HashMap<String, PeerConnection>>>,
    preferred_addresses: Arc<Mutex<HashMap<String, IpAddr>>>,
    connecting: Arc<Mutex<HashMap<String, u64>>>,
    latest_offer: Arc<Mutex<Option<LocalTextOffer>>>,
    latest_rich: Arc<Mutex<Option<LocalRichOffer>>>,
    latest_image: Arc<Mutex<Option<LocalImageOffer>>>,
    latest_files: Arc<Mutex<Option<LocalFileOffer>>>,
    latest_content_sequence: Arc<AtomicU64>,
    telemetry_observed: Arc<AtomicBool>,
    telemetry_notify: Arc<tokio::sync::Notify>,
    telemetry: TelemetryStore,
}

impl TransportHandle {
    pub(crate) fn telemetry_snapshot(&self) -> TelemetrySnapshot {
        self.telemetry.snapshot()
    }

    pub(crate) fn flush_telemetry_history(&self) -> Result<(), String> {
        self.telemetry.flush_history()
    }

    fn sample_telemetry(&self) {
        let connections = self
            .peers
            .lock()
            .map(|peers| {
                peers
                    .iter()
                    .map(|(device_id, peer)| (device_id.clone(), peer.connection.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (device_id, connection) in connections {
            self.telemetry.sample_connection(&device_id, &connection);
        }
    }

    pub(crate) fn set_telemetry_observing(&self, app: &AppHandle, observing: bool) {
        self.telemetry_observed.store(observing, Ordering::Release);
        if observing {
            self.sample_telemetry();
            self.telemetry.emit(app);
        }
        self.telemetry_notify.notify_one();
    }

    pub(crate) fn allow_pairing(&self, seconds: u64) -> Result<u64, String> {
        if !self.is_active() {
            return Err("回到前台后才能开放配对窗口".into());
        }
        let expiry = unix_seconds().saturating_add(seconds.min(120));
        *self
            .pairing_allowed_until
            .lock()
            .map_err(|_| "配对窗口状态锁已损坏".to_string())? = expiry;
        Ok(expiry)
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    fn runtime_generation(&self) -> u64 {
        self.runtime_generation.load(Ordering::Acquire)
    }

    fn is_active_generation(&self, generation: u64) -> bool {
        self.is_active() && self.runtime_generation() == generation
    }

    #[cfg(mobile)]
    pub(crate) fn suspend(&self, app: AppHandle) {
        self.active.store(false, Ordering::Release);
        self.runtime_generation.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut expiry) = self.pairing_allowed_until.lock() {
            *expiry = 0;
        }
        let pairing_commands = self
            .pair_commands
            .lock()
            .map(|commands| commands.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for command in pairing_commands {
            let _ = command.send(false);
        }
        let connections = self
            .peers
            .lock()
            .map(|mut peers| {
                peers
                    .drain()
                    .map(|(device_id, peer)| (device_id, peer.connection, peer.pending_receipts))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (device_id, connection, pending_receipts) in connections {
            fail_pending_delivery_receipts(&pending_receipts, "移动端运行时已暂停");
            self.telemetry.mark_disconnected(
                &device_id,
                &connection,
                "app_suspended",
                "移动端运行时已暂停",
                true,
            );
            connection.close(4u32.into(), b"mobile runtime suspended");
        }
        self.telemetry.emit(&app);
        self.telemetry_notify.notify_one();
        if let Ok(mut connecting) = self.connecting.lock() {
            connecting.clear();
        }
        if let Ok(mut connecting) = self.pairing_connecting.lock() {
            connecting.clear();
        }
    }

    #[cfg(mobile)]
    pub(crate) fn resume(&self) {
        if !self.active.load(Ordering::Acquire) {
            self.runtime_generation.fetch_add(1, Ordering::AcqRel);
        }
        self.active.store(true, Ordering::Release);
        self.telemetry_notify.notify_one();
    }

    pub(crate) fn confirm_pairing(&self, pairing_id: &str, accepted: bool) -> Result<(), String> {
        let commands = self
            .pair_commands
            .lock()
            .map_err(|_| "配对命令锁已损坏".to_string())?;
        commands
            .get(pairing_id)
            .ok_or_else(|| "配对会话已结束".to_string())?
            .send(accepted)
            .map_err(|_| "配对连接已断开".to_string())
    }

    pub(crate) fn broadcast_text(
        &self,
        sequence: u64,
        text: String,
        captured_at: String,
        targets: &HashMap<String, Vec<String>>,
    ) {
        if !self.is_active() || text.len() > 1024 * 1024 {
            return;
        }
        let offer = LocalTextOffer {
            sequence,
            captured_at,
            content_type: service::text_content_type(&text).into(),
            text,
        };
        self.latest_content_sequence
            .store(sequence, Ordering::Release);
        if let Ok(mut latest) = self.latest_offer.lock() {
            *latest = Some(offer.clone());
        }
        self.clear_latest_rich();
        self.clear_latest_image();
        self.clear_latest_files();
        if let Ok(peers) = self.peers.lock() {
            for (device_id, peer) in peers.iter() {
                if peer.capabilities.text {
                    if let Some(group_ids) = targets.get(device_id) {
                        let _ = peer.sender.send(TrustedMessage::ClipboardSlotOffer {
                            schema_version: 1,
                            message_id: uuid::Uuid::new_v4().simple().to_string(),
                            origin_sequence: offer.sequence,
                            captured_at: offer.captured_at.clone(),
                            text: offer.text.clone(),
                            group_ids: group_ids.clone(),
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn broadcast_rich(
        &self,
        sequence: u64,
        text: String,
        html: Option<String>,
        rtf: Option<String>,
        captured_at: String,
        targets: RichDeliveryTargets<'_>,
    ) {
        let RichDeliveryTargets {
            rich: rich_targets,
            text: text_targets,
        } = targets;
        let total_size = text
            .len()
            .saturating_add(html.as_ref().map_or(0, String::len))
            .saturating_add(rtf.as_ref().map_or(0, String::len));
        if !self.is_active() || (html.is_none() && rtf.is_none()) || total_size > 1024 * 1024 {
            return;
        }
        let offer = LocalRichOffer {
            sequence,
            captured_at,
            fallback_type: service::text_content_type(&text).into(),
            text,
            html,
            rtf,
        };
        self.latest_content_sequence
            .store(sequence, Ordering::Release);
        if let Ok(mut latest) = self.latest_rich.lock() {
            *latest = Some(offer.clone());
        }
        self.clear_latest_text();
        self.clear_latest_image();
        self.clear_latest_files();
        if let Ok(peers) = self.peers.lock() {
            for (device_id, peer) in peers.iter() {
                if peer.capabilities.rich_text {
                    if let Some(group_ids) = rich_targets.get(device_id) {
                        let _ = peer.sender.send(TrustedMessage::RichClipboardSlotOffer {
                            schema_version: 1,
                            message_id: uuid::Uuid::new_v4().simple().to_string(),
                            origin_sequence: offer.sequence,
                            captured_at: offer.captured_at.clone(),
                            text: offer.text.clone(),
                            html: offer.html.clone(),
                            rtf: offer.rtf.clone(),
                            group_ids: group_ids.clone(),
                        });
                    } else if peer.capabilities.text {
                        if let Some(group_ids) = text_targets.get(device_id) {
                            let _ = peer.sender.send(TrustedMessage::ClipboardSlotOffer {
                                schema_version: 1,
                                message_id: uuid::Uuid::new_v4().simple().to_string(),
                                origin_sequence: offer.sequence,
                                captured_at: offer.captured_at.clone(),
                                text: offer.text.clone(),
                                group_ids: group_ids.clone(),
                            });
                        }
                    }
                } else if peer.capabilities.text {
                    if let Some(group_ids) = text_targets.get(device_id) {
                        let _ = peer.sender.send(TrustedMessage::ClipboardSlotOffer {
                            schema_version: 1,
                            message_id: uuid::Uuid::new_v4().simple().to_string(),
                            origin_sequence: offer.sequence,
                            captured_at: offer.captured_at.clone(),
                            text: offer.text.clone(),
                            group_ids: group_ids.clone(),
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn broadcast_image(
        &self,
        sequence: u64,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        captured_at: String,
        targets: &HashMap<String, Vec<String>>,
    ) {
        if !self.is_active() {
            return;
        }
        let mut png = Vec::new();
        if PngEncoder::new(&mut png)
            .write_image(&rgba, width, height, ExtendedColorType::Rgba8)
            .is_err()
            || png.len() > MAX_IMAGE_BLOB
        {
            tracing::warn!(
                width,
                height,
                "clipboard image could not be encoded within limit"
            );
            return;
        }
        let offer = LocalImageOffer {
            sequence,
            captured_at,
            width,
            height,
            png: Arc::new(png),
        };
        self.latest_content_sequence
            .store(sequence, Ordering::Release);
        if let Ok(mut latest) = self.latest_image.lock() {
            *latest = Some(offer.clone());
        }
        self.clear_latest_text();
        self.clear_latest_rich();
        self.clear_latest_files();
        let connections = self
            .peers
            .lock()
            .map(|peers| {
                peers
                    .iter()
                    .filter_map(|(device_id, peer)| {
                        peer.capabilities.images.then(|| {
                            targets.get(device_id).map(|groups| {
                                (
                                    device_id.clone(),
                                    peer.connection.clone(),
                                    groups.clone(),
                                    peer.capabilities
                                        .delivery_receipts
                                        .then(|| peer.pending_receipts.clone()),
                                    peer.blob_semaphore.clone(),
                                )
                            })
                        })?
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (device_id, connection, group_ids, pending_receipts, blob_semaphore) in connections {
            let offer = offer.clone();
            let telemetry = self.telemetry.clone();
            let latest_sequence = self.latest_content_sequence.clone();
            self.runtime.spawn(async move {
                let Ok(_permit) = blob_semaphore.acquire_owned().await else {
                    return;
                };
                if latest_sequence.load(Ordering::Acquire) != offer.sequence {
                    return;
                }
                let transfer_id = uuid::Uuid::new_v4().simple().to_string();
                let transfer_key = telemetry.start_transfer(
                    transfer_id.clone(),
                    device_id,
                    "upload",
                    "image",
                    offer.png.len() as u64,
                );
                let receipt = pending_receipts
                    .as_ref()
                    .map(|pending| register_delivery_receipt(pending, &transfer_id))
                    .transpose();
                let receipt = match receipt {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        telemetry.finish_transfer(&transfer_key, false, Some(error));
                        return;
                    }
                };
                let result = send_image_blob(
                    connection,
                    offer,
                    group_ids,
                    transfer_id.clone(),
                    &telemetry,
                    &transfer_key,
                )
                .await;
                if result.is_ok() {
                    telemetry.mark_network_complete(&transfer_key);
                }
                if let Err(error) = result {
                    if let Some(pending) = pending_receipts.as_ref() {
                        cancel_delivery_receipt(pending, &transfer_id);
                    }
                    telemetry.finish_transfer(&transfer_key, false, Some(error.clone()));
                    tracing::debug!(error = %error, "clipboard image send failed");
                } else if let (Some(pending), Some(receiver)) = (pending_receipts, receipt) {
                    finish_after_delivery_receipt(
                        pending,
                        transfer_id,
                        receiver,
                        telemetry,
                        transfer_key,
                    )
                    .await;
                } else {
                    telemetry.finish_unconfirmed(
                        &transfer_key,
                        Some("图片已发送；对端版本不支持接收确认".into()),
                    );
                }
            });
        }
    }

    pub(crate) fn disable_peer(&self, device_id: &str) {
        if let Ok(mut peers) = self.peers.lock() {
            if let Some(peer) = peers.remove(device_id) {
                fail_pending_delivery_receipts(&peer.pending_receipts, "设备同步已停用");
                self.telemetry.mark_disconnected(
                    device_id,
                    &peer.connection,
                    "user_disabled",
                    "设备同步已停用",
                    true,
                );
                peer.connection
                    .close(3u32.into(), b"device synchronization disabled");
            }
        }
        if let Ok(mut connecting) = self.connecting.lock() {
            connecting.remove(device_id);
        }
    }

    pub(crate) fn refresh_local_profile(&self) {
        let connections = self
            .peers
            .lock()
            .map(|mut peers| {
                peers
                    .drain()
                    .map(|(device_id, peer)| (device_id, peer.connection, peer.pending_receipts))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (device_id, connection, pending_receipts) in connections {
            fail_pending_delivery_receipts(&pending_receipts, "本机设备资料已更新");
            self.telemetry.mark_disconnected(
                &device_id,
                &connection,
                "profile_changed",
                "本机设备资料已更新",
                true,
            );
            connection.close(5u32.into(), b"local device profile changed");
        }
    }

    fn remove_peer_if_current(&self, device_id: &str, connection: &Connection) -> bool {
        let Ok(mut peers) = self.peers.lock() else {
            return false;
        };
        if peers
            .get(device_id)
            .is_some_and(|peer| peer.connection.stable_id() == connection.stable_id())
        {
            peers.remove(device_id);
            true
        } else {
            false
        }
    }

    pub(crate) fn broadcast_files(
        &self,
        sequence: u64,
        bundle: StagedFileBundle,
        captured_at: String,
        targets: &HashMap<String, Vec<String>>,
    ) {
        if !self.is_active() {
            return;
        }
        let offer = LocalFileOffer {
            transfer_id: uuid::Uuid::new_v4().simple().to_string(),
            sequence,
            captured_at,
            bundle: Arc::new(bundle),
        };
        self.latest_content_sequence
            .store(sequence, Ordering::Release);
        if let Ok(mut latest) = self.latest_files.lock() {
            *latest = Some(offer.clone());
        }
        self.clear_latest_text();
        self.clear_latest_rich();
        self.clear_latest_image();
        let connections = self
            .peers
            .lock()
            .map(|peers| {
                peers
                    .iter()
                    .filter_map(|(device_id, peer)| {
                        peer.capabilities.files.then(|| {
                            targets.get(device_id).map(|groups| {
                                (
                                    device_id.clone(),
                                    peer.connection.clone(),
                                    groups.clone(),
                                    peer.blob_semaphore.clone(),
                                )
                            })
                        })?
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (device_id, connection, group_ids, blob_semaphore) in connections {
            let offer = offer.clone();
            let telemetry = self.telemetry.clone();
            let latest_sequence = self.latest_content_sequence.clone();
            self.runtime.spawn(async move {
                let Ok(_permit) = blob_semaphore.acquire_owned().await else {
                    return;
                };
                if latest_sequence.load(Ordering::Acquire) != offer.sequence {
                    return;
                }
                let transfer_key = telemetry.start_transfer(
                    offer.transfer_id.clone(),
                    device_id,
                    "upload",
                    "files",
                    offer.bundle.total_size,
                );
                let result = send_file_blob_with_retry(
                    connection,
                    offer,
                    group_ids,
                    &telemetry,
                    &transfer_key,
                )
                .await;
                telemetry.finish_transfer(
                    &transfer_key,
                    result.is_ok(),
                    result
                        .as_ref()
                        .err()
                        .cloned()
                        .or_else(|| Some("文件已确认接收".into())),
                );
                if let Err(error) = result {
                    tracing::warn!(error = %error, "clipboard file send failed");
                }
            });
        }
    }

    pub(crate) fn clear_latest_text(&self) {
        if let Ok(mut latest) = self.latest_offer.lock() {
            *latest = None;
        }
    }

    pub(crate) fn retain_enabled_latest_text(&self, allow_text: bool, allow_urls: bool) {
        if let Ok(mut latest) = self.latest_offer.lock() {
            if latest.as_ref().is_some_and(|offer| {
                (offer.content_type == "url" && !allow_urls)
                    || (offer.content_type == "text" && !allow_text)
            }) {
                *latest = None;
            }
        }
    }

    pub(crate) fn clear_latest_image(&self) {
        if let Ok(mut latest) = self.latest_image.lock() {
            *latest = None;
        }
    }

    pub(crate) fn clear_latest_rich(&self) {
        if let Ok(mut latest) = self.latest_rich.lock() {
            *latest = None;
        }
    }

    pub(crate) fn downgrade_latest_rich(&self, allow_text: bool, allow_urls: bool) {
        let rich = self
            .latest_rich
            .lock()
            .ok()
            .and_then(|mut latest| latest.take());
        let Some(rich) = rich else { return };
        let fallback_allowed = if rich.fallback_type == "url" {
            allow_urls
        } else {
            allow_text
        };
        if fallback_allowed {
            if let Ok(mut latest) = self.latest_offer.lock() {
                *latest = Some(LocalTextOffer {
                    sequence: rich.sequence,
                    captured_at: rich.captured_at,
                    text: rich.text,
                    content_type: rich.fallback_type,
                });
            }
        }
    }

    pub(crate) fn clear_latest_files(&self) {
        if let Ok(mut latest) = self.latest_files.lock() {
            *latest = None;
        }
    }

    pub(crate) fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    pub(crate) fn send_to(&self, device_id: &str, message: TrustedMessage) -> Result<(), String> {
        if !self.is_active() {
            return Err("移动端当前处于后台暂停状态".into());
        }
        let peers = self
            .peers
            .lock()
            .map_err(|_| "可信连接表锁已损坏".to_string())?;
        peers
            .get(device_id)
            .ok_or_else(|| "目标设备当前不在线".to_string())?
            .sender
            .send(message)
            .map_err(|_| "目标设备连接已断开".to_string())
    }

    pub(crate) fn send_group_invite(
        &self,
        device_id: &str,
        invite_id: String,
        expires_at: String,
        manifest: SignedGroupManifest,
    ) -> Result<(), String> {
        self.send_to(
            device_id,
            TrustedMessage::GroupInvite {
                schema_version: 1,
                message_id: uuid::Uuid::new_v4().simple().to_string(),
                invite_id,
                target_device_id: device_id.to_string(),
                expires_at,
                manifest,
            },
        )
    }

    pub(crate) fn send_group_accept(
        &self,
        owner_device_id: &str,
        invite_id: String,
        group_id: String,
        accepted: bool,
    ) -> Result<(), String> {
        self.send_to(
            owner_device_id,
            TrustedMessage::GroupAccept {
                schema_version: 1,
                message_id: uuid::Uuid::new_v4().simple().to_string(),
                invite_id,
                group_id,
                accepted,
            },
        )
    }

    pub(crate) fn send_group_manifest(
        &self,
        device_id: &str,
        manifest: SignedGroupManifest,
    ) -> Result<(), String> {
        self.send_to(
            device_id,
            TrustedMessage::GroupManifestUpdate {
                schema_version: 1,
                message_id: uuid::Uuid::new_v4().simple().to_string(),
                manifest,
            },
        )
    }

    pub(crate) fn send_group_leave(
        &self,
        owner_device_id: &str,
        group_id: String,
        leave_id: String,
    ) -> Result<(), String> {
        self.send_to(
            owner_device_id,
            TrustedMessage::GroupLeaveNotice {
                schema_version: 1,
                message_id: uuid::Uuid::new_v4().simple().to_string(),
                group_id,
                leave_id,
            },
        )
    }

    pub(crate) fn send_group_tombstone(
        &self,
        device_id: &str,
        tombstone: SignedGroupTombstone,
    ) -> Result<(), String> {
        self.send_to(
            device_id,
            TrustedMessage::GroupTombstone {
                schema_version: 1,
                message_id: uuid::Uuid::new_v4().simple().to_string(),
                tombstone,
            },
        )
    }

    pub(crate) fn connect_pairing(
        &self,
        app: AppHandle,
        nearby: service::NearbyDevice,
    ) -> Result<(), String> {
        if !self.is_active() {
            return Err("回到前台后才能发起配对".into());
        }
        let device_id = nearby.device_id.clone();
        let generation = self.runtime_generation();
        let mut connecting = self
            .pairing_connecting
            .lock()
            .map_err(|_| "配对连接状态锁已损坏".to_string())?;
        if connecting.contains_key(&device_id) {
            return Err("该设备的配对请求正在进行".into());
        }
        connecting.insert(device_id.clone(), generation);
        drop(connecting);
        let handle = self.clone();
        self.runtime.spawn(async move {
            if let Err(error) = handle.connect_pairing_inner(app, nearby, generation).await {
                tracing::warn!(error = %error, "pairing connection failed");
            }
            if let Ok(mut connecting) = handle.pairing_connecting.lock() {
                if connecting.get(&device_id) == Some(&generation) {
                    connecting.remove(&device_id);
                }
            }
        });
        Ok(())
    }

    pub(crate) fn connect_trusted(&self, app: AppHandle, nearby: service::NearbyDevice) {
        if !self.is_active() {
            return;
        }
        let device_id = nearby.device_id.clone();
        let local_should_dial = {
            let state = app.state::<ServiceState>();
            state.device_id() < device_id.as_str()
                && state
                    .authorized_device(&device_id)
                    .ok()
                    .flatten()
                    .is_some_and(|device| device.sync_enabled)
        };
        if !local_should_dial {
            return;
        }
        if self
            .peers
            .lock()
            .is_ok_and(|peers| peers.contains_key(&device_id))
        {
            return;
        }
        let Ok(mut connecting) = self.connecting.lock() else {
            return;
        };
        let generation = self.runtime_generation();
        if connecting.insert(device_id.clone(), generation).is_some() {
            return;
        }
        drop(connecting);
        let handle = self.clone();
        self.runtime.spawn(async move {
            let mut retry_delay = 0u64;
            loop {
                if !handle.is_active_generation(generation) {
                    break;
                }
                if retry_delay > 0 {
                    tokio::time::sleep(jittered_retry_delay(retry_delay)).await;
                    if !handle.is_active_generation(generation) {
                        break;
                    }
                }
                let current = {
                    let state = app.state::<ServiceState>();
                    if !state
                        .authorized_device(&device_id)
                        .ok()
                        .flatten()
                        .is_some_and(|device| device.sync_enabled)
                    {
                        None
                    } else {
                        state.nearby_device(&device_id)
                    }
                };
                let Some(current) = current else {
                    break;
                };
                let attempt_started = Instant::now();
                if let Err(error) = handle
                    .connect_trusted_inner(app.clone(), current, generation)
                    .await
                {
                    tracing::debug!(device_id = %device_id, error = %error, retry_delay, "trusted connection unavailable");
                }
                retry_delay = if attempt_started.elapsed() >= Duration::from_secs(30)
                    || retry_delay == 0
                {
                    1
                } else {
                    (retry_delay * 2).min(30)
                };
            }
            if let Ok(mut connecting) = handle.connecting.lock() {
                if connecting.get(&device_id) == Some(&generation) {
                    connecting.remove(&device_id);
                }
            }
        });
    }

    async fn connect_nearby(
        &self,
        nearby: &service::NearbyDevice,
        config: quinn::ClientConfig,
        purpose: &str,
    ) -> Result<Connection, String> {
        let preferred = self
            .preferred_addresses
            .lock()
            .ok()
            .and_then(|addresses| addresses.get(&nearby.device_id).copied());
        let addresses = candidate_addresses(nearby, preferred);
        if addresses.is_empty() {
            return Err("附近设备尚未解析出可连接地址".into());
        }
        let mut attempts = tokio::task::JoinSet::new();
        for (index, address) in addresses.into_iter().enumerate() {
            let endpoint = self.endpoint.clone();
            let config = config.clone();
            let delay = Duration::from_millis((index.min(4) as u64) * 175);
            attempts.spawn(async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                let result = match endpoint.connect_with(config, address, "localdrop") {
                    Ok(connecting) => connecting.await.map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                };
                (address, result)
            });
        }
        let mut errors = Vec::new();
        while let Some(attempt) = attempts.join_next().await {
            match attempt {
                Ok((address, Ok(connection))) => {
                    attempts.abort_all();
                    if let Ok(mut preferred) = self.preferred_addresses.lock() {
                        preferred.insert(nearby.device_id.clone(), address.ip());
                    }
                    return Ok(connection);
                }
                Ok((address, Err(error))) => errors.push(format!("{address}: {error}")),
                Err(error) if !error.is_cancelled() => errors.push(error.to_string()),
                Err(_) => {}
            }
        }
        Err(format!(
            "{purpose}无法连接任何已发现地址：{}",
            errors.join("；")
        ))
    }

    async fn connect_pairing_inner(
        &self,
        app: AppHandle,
        nearby: service::NearbyDevice,
        generation: u64,
    ) -> Result<(), String> {
        if !self.is_active_generation(generation) {
            return Err("回到前台后才能发起配对".into());
        }
        let config = client_config(
            None,
            PAIR_ALPN,
            self.certificate_der.clone(),
            self.private_key_der.clone(),
        )?;
        let connection = self.connect_nearby(&nearby, config, "配对").await?;
        if !self.is_active_generation(generation) {
            connection.close(4u32.into(), b"mobile runtime suspended");
            return Err("移动端当前处于后台暂停状态".into());
        }
        let peer_certificate = peer_certificate(&connection)?;
        let (mut send, mut receive) = connection
            .open_bi()
            .await
            .map_err(|error| format!("无法打开配对控制流：{error}"))?;
        let pairing_id = uuid::Uuid::new_v4().simple().to_string();
        let initiator_nonce = random_bytes(32);
        let (init, local_device_id) = {
            let state = app.state::<ServiceState>();
            (
                PairMessage::Init {
                    schema_version: 1,
                    pairing_id: pairing_id.clone(),
                    nonce: BASE64.encode(&initiator_nonce),
                    device_id: state.device_id().to_string(),
                    device_name: state.device_name()?,
                    platform: platform::platform_name().to_string(),
                    public_key: BASE64.encode(&state.identity().public_key_bytes()),
                    certificate: BASE64.encode(&self.certificate_der),
                },
                state.device_id().to_string(),
            )
        };
        write_frame(&mut send, &init).await?;
        let hello: PairMessage = read_frame(&mut receive).await?;
        if !self.is_active_generation(generation) {
            connection.close(4u32.into(), b"mobile runtime suspended");
            return Err("移动端当前处于后台暂停状态".into());
        }
        let PairMessage::Hello {
            schema_version: 1,
            pairing_id: echoed_id,
            initiator_nonce: echoed_nonce,
            responder_nonce,
            device_id,
            device_name,
            platform,
            public_key,
            certificate,
        } = hello
        else {
            return Err("配对响应类型或版本无效".into());
        };
        if echoed_id != pairing_id || echoed_nonce != BASE64.encode(&initiator_nonce) {
            return Err("配对响应未绑定当前会话".into());
        }
        let certificate_der = decode(&certificate, "设备证书")?;
        if certificate_der != peer_certificate {
            return Err("配对身份与 TLS 证书不一致".into());
        }
        let public_key = validate_identity(&device_id, &public_key)?;
        let responder_nonce = decode(&responder_nonce, "响应随机数")?;
        let device = TrustedDevice {
            device_id,
            device_name,
            local_alias: None,
            platform,
            public_key,
            certificate_der,
            paired_at: now(),
            sync_enabled: true,
        };
        let context = pairing_context(
            &pairing_id,
            &initiator_nonce,
            &responder_nonce,
            &local_device_id,
            &device.device_id,
        );
        self.run_pair_confirmation(
            app, connection, send, receive, device, pairing_id, context, "outgoing", generation,
        )
        .await
    }

    async fn connect_trusted_inner(
        &self,
        app: AppHandle,
        nearby: service::NearbyDevice,
        generation: u64,
    ) -> Result<(), String> {
        if !self.is_active_generation(generation) {
            return Err("移动端当前处于后台暂停状态".into());
        }
        let trusted = {
            let state = app.state::<ServiceState>();
            state
                .authorized_device(&nearby.device_id)?
                .ok_or_else(|| "设备尚未配对".to_string())?
        };
        if !trusted.sync_enabled {
            return Err("该设备的剪贴板同步已停用".into());
        }
        let config = client_config(
            Some(trusted.certificate_der.clone()),
            TRUSTED_ALPN,
            self.certificate_der.clone(),
            self.private_key_der.clone(),
        )?;
        let connection = self.connect_nearby(&nearby, config, "可信连接").await?;
        let (send, receive) = connection
            .open_bi()
            .await
            .map_err(|error| format!("无法打开可信控制流：{error}"))?;
        self.run_trusted(app, connection, send, receive, Some(trusted), generation)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_pair_confirmation(
        &self,
        app: AppHandle,
        connection: Connection,
        mut send: quinn::SendStream,
        mut receive: quinn::RecvStream,
        device: TrustedDevice,
        pairing_id: String,
        context: Vec<u8>,
        direction: &str,
        generation: u64,
    ) -> Result<(), String> {
        if !self.is_active_generation(generation) {
            connection.close(4u32.into(), b"mobile runtime suspended");
            return Err("移动端当前处于后台暂停状态".into());
        }
        let mut exporter = [0u8; 32];
        connection
            .export_keying_material(&mut exporter, b"EXPORTER-localdrop-pairing-v1", &context)
            .map_err(|error| format!("无法导出配对会话密钥：{error:?}"))?;
        let sas = derive_sas(&exporter, &context)?;
        let context_hash = HEXLOWER.encode(&Sha256::digest(&context));
        let expires_at = (OffsetDateTime::now_utc() + time::Duration::seconds(120))
            .format(&Rfc3339)
            .unwrap_or_else(|_| now());
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        self.pair_commands
            .lock()
            .map_err(|_| "配对命令锁已损坏".to_string())?
            .insert(pairing_id.clone(), command_tx);
        let _registration = PairCommandRegistration {
            commands: self.pair_commands.clone(),
            pairing_id: pairing_id.clone(),
        };
        if !self.is_active_generation(generation) {
            connection.close(4u32.into(), b"mobile runtime suspended");
            return Err("移动端当前处于后台暂停状态".into());
        }
        {
            let state = app.state::<ServiceState>();
            state.save_pending_pairing(&pairing_id, &device, &expires_at)?;
            service::show_pending_pairing(
                &state,
                &app,
                PendingPairing {
                    pairing_id: pairing_id.clone(),
                    device_id: device.device_id.clone(),
                    device_name: device.device_name.clone(),
                    platform: device.platform.clone(),
                    sas,
                    direction: direction.into(),
                    expires_at,
                    status: "awaiting_confirmation".into(),
                },
            )?;
        }
        let mut local_confirmed = false;
        let mut remote_confirmed = false;
        let mut local_complete_sent = false;
        let mut remote_completed = false;
        let result: Result<(), String> = async {
            loop {
                tokio::select! {
                    command = command_rx.recv() => {
                        let accepted = command.ok_or_else(|| "配对会话已取消".to_string())?;
                        if local_confirmed {
                            continue;
                        }
                        write_frame(&mut send, &PairMessage::Confirm {
                            schema_version: 1,
                            pairing_id: pairing_id.clone(),
                            context_hash: context_hash.clone(),
                            accepted,
                        }).await?;
                        if !accepted {
                            return Err("用户拒绝了配对".into());
                        }
                        local_confirmed = true;
                        let state = app.state::<ServiceState>();
                        let _ = service::pairing_status(&state, &app, &pairing_id, "waiting_for_peer");
                    }
                    message = read_frame::<PairMessage>(&mut receive) => {
                        match message {
                            Err(error) => return Err(error),
                            Ok(message) => match message {
                            PairMessage::Confirm { schema_version: 1, pairing_id: remote_id, context_hash: remote_hash, accepted }
                                if remote_id == pairing_id && remote_hash == context_hash => {
                                    if !accepted { return Err("对方拒绝了配对".into()); }
                                    remote_confirmed = true;
                                    if !local_confirmed {
                                        let state = app.state::<ServiceState>();
                                        let _ = service::pairing_status(&state, &app, &pairing_id, "peer_confirmed");
                                    }
                            }
                            PairMessage::Complete { schema_version: 1, pairing_id: remote_id }
                                if remote_id == pairing_id && local_confirmed && remote_confirmed => {
                                    remote_completed = true;
                            }
                            PairMessage::Abort { reason, .. } => return Err(reason),
                            _ => return Err("配对确认消息无效".into()),
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(if local_complete_sent { 10 } else { 120 })) => {
                        return Err(if local_complete_sent {
                            "未收到对端配对完成确认".into()
                        } else {
                            "配对确认已超时".into()
                        });
                    },
                }
                if local_confirmed && remote_confirmed && !local_complete_sent {
                    write_frame(
                        &mut send,
                        &PairMessage::Complete {
                            schema_version: 1,
                            pairing_id: pairing_id.clone(),
                        },
                    )
                    .await?;
                    local_complete_sent = true;
                    let state = app.state::<ServiceState>();
                    let _ = service::pairing_status(
                        &state,
                        &app,
                        &pairing_id,
                        "waiting_for_peer_complete",
                    );
                }
                if local_complete_sent && remote_completed {
                    let paired_at = now();
                    let nearby = {
                        let state = app.state::<ServiceState>();
                        let promoted = state.promote_trusted_device(&pairing_id, &paired_at)?;
                        service::pairing_completed(&state, &app, &pairing_id, promoted)?;
                        state.nearby_device(&device.device_id)
                    };
                    let _ = send.finish();
                    if let Some(nearby) = nearby {
                        self.connect_trusted(app.clone(), nearby);
                    }
                    return Ok(());
                }
            }
        }
        .await;
        if result.is_err() {
            let state = app.state::<ServiceState>();
            let _ = service::pairing_cancelled(&state, &app, &pairing_id);
            connection.close(1u32.into(), b"pairing cancelled");
        }
        result
    }

    async fn run_trusted(
        &self,
        app: AppHandle,
        connection: Connection,
        mut send: quinn::SendStream,
        mut receive: quinn::RecvStream,
        expected: Option<TrustedDevice>,
        generation: u64,
    ) -> Result<(), String> {
        if !self.is_active_generation(generation) {
            connection.close(4u32.into(), b"mobile runtime suspended");
            return Err("移动端当前处于后台暂停状态".into());
        }
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let hello = {
            let state = app.state::<ServiceState>();
            let payload = hello_payload(state.device_id(), &nonce);
            TrustedMessage::Hello {
                schema_version: 1,
                device_id: state.device_id().to_string(),
                device_name: state.device_name()?,
                platform: platform::platform_name().to_string(),
                nonce,
                public_key: BASE64.encode(&state.identity().public_key_bytes()),
                signature: BASE64.encode(&state.identity().sign(&payload).to_bytes()),
                capabilities: ClipboardCapabilities::local(),
            }
        };
        write_frame(&mut send, &hello).await?;
        let remote: TrustedMessage = read_frame(&mut receive).await?;
        let TrustedMessage::Hello {
            schema_version: 1,
            device_id,
            device_name: remote_device_name,
            platform: remote_platform,
            nonce,
            public_key,
            signature,
            capabilities: remote_capabilities,
        } = remote
        else {
            return Err("可信连接缺少有效 Hello".into());
        };
        if expected
            .as_ref()
            .is_some_and(|item| item.device_id != device_id)
        {
            return Err("可信连接返回了不同设备身份".into());
        }
        if !self.is_active_generation(generation) {
            connection.close(4u32.into(), b"mobile runtime suspended");
            return Err("移动端当前处于后台暂停状态".into());
        }
        let presented_certificate = peer_certificate(&connection)?;
        let trusted = {
            let state = app.state::<ServiceState>();
            let mut trusted = state
                .authorized_device(&device_id)?
                .ok_or_else(|| "对端身份不在可信设备中".to_string())?;
            if !trusted.sync_enabled {
                return Err("该设备的剪贴板同步已停用".into());
            }
            if trusted.certificate_der != presented_certificate {
                return Err("可信连接的 TLS 客户端证书与固定身份不一致".into());
            }
            verify_hello(&trusted, &nonce, &public_key, &signature)?;
            let (advertised_name, platform) = service::update_trusted_device_profile(
                &state,
                &app,
                &device_id,
                &remote_device_name,
                &remote_platform,
            )?;
            trusted.device_name = advertised_name;
            trusted.platform = platform;
            trusted
        };
        let remote_received_sequence = if remote_capabilities.state_reconciliation {
            let last_received_sequence = {
                let state = app.state::<ServiceState>();
                state.reconciliation_sequence(&device_id)
            };
            write_frame(
                &mut send,
                &TrustedMessage::ClipboardStateSummary {
                    schema_version: 1,
                    last_received_sequence,
                },
            )
            .await?;
            match read_frame::<TrustedMessage>(&mut receive).await? {
                TrustedMessage::ClipboardStateSummary {
                    schema_version: 1,
                    last_received_sequence,
                } => last_received_sequence,
                _ => return Err("可信连接缺少有效剪贴板状态摘要".into()),
            }
        } else {
            None
        };
        let (sender, mut outbound) = mpsc::unbounded_channel::<TrustedMessage>();
        let receipt_sender = sender.clone();
        let pending_receipts = PendingDeliveryReceipts::default();
        let blob_semaphore = Arc::new(tokio::sync::Semaphore::new(1));
        let replaced_peer = {
            let mut peers = self
                .peers
                .lock()
                .map_err(|_| "可信连接表锁已损坏".to_string())?;
            if !self.is_active_generation(generation) {
                connection.close(4u32.into(), b"mobile runtime suspended");
                return Err("移动端当前处于后台暂停状态".into());
            }
            peers.insert(
                device_id.clone(),
                PeerConnection {
                    sender,
                    connection: connection.clone(),
                    capabilities: remote_capabilities.clone(),
                    pending_receipts: pending_receipts.clone(),
                    blob_semaphore: blob_semaphore.clone(),
                },
            )
        };
        if let Some(replaced) = replaced_peer {
            fail_pending_delivery_receipts(&replaced.pending_receipts, "可信连接已被新的连接替换");
            self.telemetry.mark_disconnected(
                &device_id,
                &replaced.connection,
                "connection_replaced",
                "可信连接已被新的连接替换",
                true,
            );
            replaced
                .connection
                .close(6u32.into(), b"superseded trusted connection");
        }
        {
            let state = app.state::<ServiceState>();
            service::set_trusted_online(&state, &app, &device_id, true)?;
        }
        self.telemetry.mark_connected(&device_id, &connection);
        self.telemetry.emit(&app);
        if !self.is_active_generation(generation) {
            if self.remove_peer_if_current(&device_id, &connection) {
                let state = app.state::<ServiceState>();
                let _ = service::set_trusted_online(&state, &app, &device_id, false);
                self.telemetry.mark_disconnected(
                    &device_id,
                    &connection,
                    "app_suspended",
                    "移动端运行时已暂停",
                    true,
                );
                self.telemetry.emit(&app);
            }
            connection.close(4u32.into(), b"mobile runtime suspended");
            return Err("移动端当前处于后台暂停状态".into());
        }
        {
            let state = app.state::<ServiceState>();
            state.replay_group_state(self, &device_id);
        }
        if remote_capabilities.text {
            if let Some(latest) = self
                .latest_offer
                .lock()
                .ok()
                .and_then(|latest| latest.clone())
                .filter(|latest| should_replay_sequence(latest.sequence, remote_received_sequence))
            {
                let groups = {
                    let state = app.state::<ServiceState>();
                    state
                        .can_publish_content(&latest.content_type)
                        .then(|| {
                            state
                                .delivery_targets(&latest.content_type)
                                .ok()
                                .and_then(|targets| targets.get(&device_id).cloned())
                        })
                        .flatten()
                };
                if let Some(group_ids) = groups {
                    if let Ok(peers) = self.peers.lock() {
                        if let Some(peer) = peers.get(&device_id) {
                            let _ = peer.sender.send(TrustedMessage::ClipboardSlotOffer {
                                schema_version: 1,
                                message_id: uuid::Uuid::new_v4().simple().to_string(),
                                origin_sequence: latest.sequence,
                                captured_at: latest.captured_at,
                                text: latest.text,
                                group_ids,
                            });
                        }
                    }
                }
            }
        }
        if let Some(latest) = self
            .latest_rich
            .lock()
            .ok()
            .and_then(|latest| latest.clone())
            .filter(|latest| should_replay_sequence(latest.sequence, remote_received_sequence))
        {
            let (rich_groups, text_groups) = {
                let state = app.state::<ServiceState>();
                let rich = state
                    .can_publish_content("html")
                    .then(|| {
                        state
                            .delivery_targets("html")
                            .ok()
                            .and_then(|targets| targets.get(&device_id).cloned())
                    })
                    .flatten();
                let text = state
                    .can_publish_content(&latest.fallback_type)
                    .then(|| {
                        state
                            .delivery_targets(&latest.fallback_type)
                            .ok()
                            .and_then(|targets| targets.get(&device_id).cloned())
                    })
                    .flatten();
                (rich, text)
            };
            if let Ok(peers) = self.peers.lock() {
                if let Some(peer) = peers.get(&device_id) {
                    if remote_capabilities.rich_text {
                        if let Some(group_ids) = rich_groups {
                            let _ = peer.sender.send(TrustedMessage::RichClipboardSlotOffer {
                                schema_version: 1,
                                message_id: uuid::Uuid::new_v4().simple().to_string(),
                                origin_sequence: latest.sequence,
                                captured_at: latest.captured_at,
                                text: latest.text,
                                html: latest.html,
                                rtf: latest.rtf,
                                group_ids,
                            });
                        } else if remote_capabilities.text {
                            if let Some(group_ids) = text_groups {
                                let _ = peer.sender.send(TrustedMessage::ClipboardSlotOffer {
                                    schema_version: 1,
                                    message_id: uuid::Uuid::new_v4().simple().to_string(),
                                    origin_sequence: latest.sequence,
                                    captured_at: latest.captured_at,
                                    text: latest.text,
                                    group_ids,
                                });
                            }
                        }
                    } else if remote_capabilities.text {
                        if let Some(group_ids) = text_groups {
                            let _ = peer.sender.send(TrustedMessage::ClipboardSlotOffer {
                                schema_version: 1,
                                message_id: uuid::Uuid::new_v4().simple().to_string(),
                                origin_sequence: latest.sequence,
                                captured_at: latest.captured_at,
                                text: latest.text,
                                group_ids,
                            });
                        }
                    }
                }
            }
        }
        if remote_capabilities.images {
            if let Some(image) = self
                .latest_image
                .lock()
                .ok()
                .and_then(|latest| latest.clone())
                .filter(|latest| should_replay_sequence(latest.sequence, remote_received_sequence))
            {
                let connection = connection.clone();
                let groups = {
                    let state = app.state::<ServiceState>();
                    state
                        .can_publish_content("image")
                        .then(|| {
                            state
                                .delivery_targets("image")
                                .ok()
                                .and_then(|targets| targets.get(&device_id).cloned())
                        })
                        .flatten()
                };
                let telemetry = self.telemetry.clone();
                let telemetry_device_id = device_id.clone();
                let image_receipts = remote_capabilities
                    .delivery_receipts
                    .then(|| pending_receipts.clone());
                let image_blob_semaphore = blob_semaphore.clone();
                let latest_sequence = self.latest_content_sequence.clone();
                self.runtime.spawn(async move {
                    if let Some(group_ids) = groups {
                        let Ok(_permit) = image_blob_semaphore.acquire_owned().await else {
                            return;
                        };
                        if latest_sequence.load(Ordering::Acquire) != image.sequence {
                            return;
                        }
                        let transfer_id = uuid::Uuid::new_v4().simple().to_string();
                        let transfer_key = telemetry.start_transfer(
                            transfer_id.clone(),
                            telemetry_device_id,
                            "upload",
                            "image",
                            image.png.len() as u64,
                        );
                        let receipt = image_receipts
                            .as_ref()
                            .map(|pending| register_delivery_receipt(pending, &transfer_id))
                            .transpose();
                        let receipt = match receipt {
                            Ok(receipt) => receipt,
                            Err(error) => {
                                telemetry.finish_transfer(&transfer_key, false, Some(error));
                                return;
                            }
                        };
                        let result = send_image_blob(
                            connection,
                            image,
                            group_ids,
                            transfer_id.clone(),
                            &telemetry,
                            &transfer_key,
                        )
                        .await;
                        if result.is_ok() {
                            telemetry.mark_network_complete(&transfer_key);
                        }
                        if let Err(error) = result {
                            if let Some(pending) = image_receipts.as_ref() {
                                cancel_delivery_receipt(pending, &transfer_id);
                            }
                            telemetry.finish_transfer(&transfer_key, false, Some(error.clone()));
                            tracing::debug!(error = %error, "cached clipboard image send failed");
                        } else if let (Some(pending), Some(receiver)) = (image_receipts, receipt) {
                            finish_after_delivery_receipt(
                                pending,
                                transfer_id,
                                receiver,
                                telemetry,
                                transfer_key,
                            )
                            .await;
                        } else {
                            telemetry.finish_unconfirmed(
                                &transfer_key,
                                Some("离线图片已补发；对端版本不支持接收确认".into()),
                            );
                        }
                    }
                });
            }
        }
        if remote_capabilities.files {
            if let Some(files) = self
                .latest_files
                .lock()
                .ok()
                .and_then(|latest| latest.clone())
                .filter(|latest| should_replay_sequence(latest.sequence, remote_received_sequence))
            {
                let connection = connection.clone();
                let groups = {
                    let state = app.state::<ServiceState>();
                    state
                        .can_publish_content("files")
                        .then(|| {
                            state
                                .delivery_targets("files")
                                .ok()
                                .and_then(|targets| targets.get(&device_id).cloned())
                        })
                        .flatten()
                };
                let telemetry = self.telemetry.clone();
                let telemetry_device_id = device_id.clone();
                let file_blob_semaphore = blob_semaphore.clone();
                let latest_sequence = self.latest_content_sequence.clone();
                self.runtime.spawn(async move {
                    if let Some(group_ids) = groups {
                        let Ok(_permit) = file_blob_semaphore.acquire_owned().await else {
                            return;
                        };
                        if latest_sequence.load(Ordering::Acquire) != files.sequence {
                            return;
                        }
                        let transfer_key = telemetry.start_transfer(
                            files.transfer_id.clone(),
                            telemetry_device_id,
                            "upload",
                            "files",
                            files.bundle.total_size,
                        );
                        let result = send_file_blob_with_retry(
                            connection,
                            files,
                            group_ids,
                            &telemetry,
                            &transfer_key,
                        )
                        .await;
                        telemetry.finish_transfer(
                            &transfer_key,
                            result.is_ok(),
                            result
                                .as_ref()
                                .err()
                                .cloned()
                                .or_else(|| Some("离线文件已补发".into())),
                        );
                        if let Err(error) = result {
                            tracing::warn!(error = %error, "cached clipboard file send failed");
                        }
                    }
                });
            }
        }
        let writer_connection = connection.clone();
        let writer_telemetry = self.telemetry.clone();
        let writer_device_id = device_id.clone();
        let writer_pending_receipts = pending_receipts.clone();
        let writer_supports_receipts = remote_capabilities.delivery_receipts;
        let writer = tokio::spawn(async move {
            let mut deferred = VecDeque::new();
            loop {
                let mut message = if let Some(message) = deferred.pop_front() {
                    message
                } else if let Some(message) = outbound.recv().await {
                    message
                } else {
                    break;
                };
                if is_clipboard_offer(&message) {
                    while let Ok(next) = outbound.try_recv() {
                        if is_clipboard_offer(&next) {
                            message = next;
                        } else {
                            deferred.push_back(next);
                            break;
                        }
                    }
                }
                let transfer = trusted_message_transfer(&message).map(|(id, kind, size)| {
                    let key = writer_telemetry.start_transfer(
                        id.clone(),
                        writer_device_id.clone(),
                        "upload",
                        kind,
                        size,
                    );
                    (id, key)
                });
                let receipt = if writer_supports_receipts {
                    match transfer.as_ref() {
                        Some((message_id, transfer_key)) => {
                            match register_delivery_receipt(&writer_pending_receipts, message_id) {
                                Ok(receiver) => Some(receiver),
                                Err(error) => {
                                    writer_telemetry.finish_transfer(
                                        transfer_key,
                                        false,
                                        Some(error),
                                    );
                                    writer_connection.close(
                                        1u32.into(),
                                        b"delivery receipt registration failed",
                                    );
                                    break;
                                }
                            }
                        }
                        None => None,
                    }
                } else {
                    None
                };
                match write_frame(&mut send, &message).await {
                    Ok(()) => {
                        if let Some((message_id, key)) = transfer {
                            writer_telemetry.update_transfer(&key, trusted_message_size(&message));
                            writer_telemetry.mark_network_complete(&key);
                            if let Some(receiver) = receipt {
                                tokio::spawn(finish_after_delivery_receipt(
                                    writer_pending_receipts.clone(),
                                    message_id,
                                    receiver,
                                    writer_telemetry.clone(),
                                    key,
                                ));
                            } else {
                                writer_telemetry.finish_unconfirmed(
                                    &key,
                                    Some("已发送；对端版本不支持接收确认".into()),
                                );
                            }
                        }
                    }
                    Err(error) => {
                        if let Some((message_id, key)) = transfer {
                            cancel_delivery_receipt(&writer_pending_receipts, &message_id);
                            writer_telemetry.finish_transfer(&key, false, Some(error));
                        }
                        writer_connection.close(1u32.into(), b"trusted writer failed");
                        break;
                    }
                }
            }
        });
        let blob_reader = tokio::spawn(receive_clipboard_blobs(
            app.clone(),
            connection.clone(),
            trusted.clone(),
            self.telemetry.clone(),
            remote_capabilities
                .delivery_receipts
                .then(|| receipt_sender.clone()),
        ));
        let file_reader = tokio::spawn(receive_file_streams(
            app.clone(),
            connection.clone(),
            trusted.clone(),
            self.telemetry.clone(),
        ));
        loop {
            match read_frame::<TrustedMessage>(&mut receive).await {
                Ok(TrustedMessage::ClipboardSlotOffer {
                    schema_version: 1,
                    message_id,
                    origin_sequence,
                    captured_at,
                    text,
                    group_ids,
                }) => {
                    let transfer_key = self.telemetry.start_transfer(
                        message_id.clone(),
                        device_id.clone(),
                        "download",
                        service::text_content_type(&text),
                        text.len() as u64,
                    );
                    let processing_started = Instant::now();
                    let state = app.state::<ServiceState>();
                    let result = service::receive_remote_text(
                        &state,
                        &app,
                        &trusted,
                        origin_sequence,
                        text,
                        captured_at,
                        group_ids,
                    );
                    let processing_ms = elapsed_millis(processing_started.elapsed());
                    if let Err(error) = &result {
                        tracing::warn!(device_id = %device_id, error = %error, "remote clipboard rejected");
                    }
                    let accepted = result.is_ok();
                    let receipt_message = result.as_ref().err().cloned();
                    self.telemetry.finish_transfer(
                        &transfer_key,
                        accepted,
                        receipt_message
                            .clone()
                            .or_else(|| Some("已写入设备槽位".into())),
                    );
                    if remote_capabilities.delivery_receipts {
                        let _ = receipt_sender.send(TrustedMessage::ClipboardDeliveryAck {
                            schema_version: 1,
                            message_id,
                            accepted,
                            message: receipt_message,
                            processing_ms: Some(processing_ms),
                        });
                    }
                }
                Ok(TrustedMessage::RichClipboardSlotOffer {
                    schema_version: 1,
                    message_id,
                    origin_sequence,
                    captured_at,
                    text,
                    html,
                    rtf,
                    group_ids,
                }) => {
                    let total_size = text
                        .len()
                        .saturating_add(html.as_ref().map_or(0, String::len))
                        .saturating_add(rtf.as_ref().map_or(0, String::len));
                    let transfer_key = self.telemetry.start_transfer(
                        message_id.clone(),
                        device_id.clone(),
                        "download",
                        "html",
                        total_size as u64,
                    );
                    let processing_started = Instant::now();
                    let state = app.state::<ServiceState>();
                    let capabilities = ClipboardCapabilities::local();
                    let result = if capabilities.rich_text {
                        service::receive_remote_rich(
                            &state,
                            &app,
                            &trusted,
                            service::RemoteRich {
                                sequence: origin_sequence,
                                text,
                                html,
                                rtf,
                                captured_at,
                                group_ids,
                            },
                        )
                    } else if capabilities.text {
                        service::receive_remote_text(
                            &state,
                            &app,
                            &trusted,
                            origin_sequence,
                            text,
                            captured_at,
                            group_ids,
                        )
                    } else {
                        Err("本机不支持接收文本剪贴板".into())
                    };
                    let processing_ms = elapsed_millis(processing_started.elapsed());
                    let accepted = result.is_ok();
                    let receipt_message = result.as_ref().err().cloned();
                    if let Some(error) = receipt_message.as_ref() {
                        tracing::warn!(device_id = %device_id, error = %error, "remote rich clipboard rejected");
                        self.telemetry
                            .finish_transfer(&transfer_key, false, Some(error.clone()));
                    } else {
                        self.telemetry.finish_transfer(
                            &transfer_key,
                            true,
                            Some("已写入设备槽位".into()),
                        );
                    }
                    if remote_capabilities.delivery_receipts {
                        let _ = receipt_sender.send(TrustedMessage::ClipboardDeliveryAck {
                            schema_version: 1,
                            message_id,
                            accepted,
                            message: receipt_message,
                            processing_ms: Some(processing_ms),
                        });
                    }
                }
                Ok(TrustedMessage::ClipboardDeliveryAck {
                    schema_version: 1,
                    message_id,
                    accepted,
                    message,
                    processing_ms,
                }) => {
                    if !complete_delivery_receipt(
                        &pending_receipts,
                        &message_id,
                        accepted,
                        message,
                        processing_ms,
                    ) {
                        tracing::debug!(message_id = %message_id, "received stale clipboard delivery receipt");
                    }
                }
                Ok(TrustedMessage::GroupInvite {
                    schema_version: 1,
                    invite_id,
                    target_device_id,
                    expires_at,
                    manifest,
                    ..
                }) => {
                    let state = app.state::<ServiceState>();
                    if let Err(error) = service::receive_group_invite(
                        &state,
                        &app,
                        &device_id,
                        invite_id,
                        target_device_id,
                        expires_at,
                        manifest,
                    ) {
                        tracing::warn!(device_id = %device_id, error = %error, "group invite rejected");
                    }
                }
                Ok(TrustedMessage::GroupAccept {
                    schema_version: 1,
                    invite_id,
                    group_id,
                    accepted,
                    ..
                }) => {
                    let state = app.state::<ServiceState>();
                    if let Err(error) = service::receive_group_accept(
                        &state, &app, self, &device_id, &invite_id, &group_id, accepted,
                    ) {
                        tracing::warn!(device_id = %device_id, error = %error, "group accept rejected");
                    }
                }
                Ok(TrustedMessage::GroupManifestUpdate {
                    schema_version: 1,
                    manifest,
                    ..
                }) => {
                    let state = app.state::<ServiceState>();
                    if let Err(error) =
                        service::receive_group_manifest(&state, &app, self, manifest)
                    {
                        tracing::warn!(device_id = %device_id, error = %error, "group manifest rejected");
                    }
                }
                Ok(TrustedMessage::GroupLeaveNotice {
                    schema_version: 1,
                    group_id,
                    leave_id,
                    ..
                }) => {
                    let state = app.state::<ServiceState>();
                    if let Err(error) = service::receive_group_leave(
                        &state, &app, self, &device_id, &group_id, &leave_id,
                    ) {
                        tracing::warn!(device_id = %device_id, error = %error, "group leave rejected");
                    }
                }
                Ok(TrustedMessage::GroupTombstone {
                    schema_version: 1,
                    tombstone,
                    ..
                }) => {
                    let state = app.state::<ServiceState>();
                    if let Err(error) = service::receive_group_tombstone(&state, &app, tombstone) {
                        tracing::warn!(device_id = %device_id, error = %error, "group tombstone rejected");
                    }
                }
                Ok(_) => {
                    fail_pending_delivery_receipts(&pending_receipts, "可信连接收到意外消息");
                    writer.abort();
                    blob_reader.abort();
                    file_reader.abort();
                    if self.remove_peer_if_current(&device_id, &connection) {
                        let state = app.state::<ServiceState>();
                        let _ = service::set_trusted_online(&state, &app, &device_id, false);
                        self.telemetry.mark_disconnected(
                            &device_id,
                            &connection,
                            "protocol_rejected",
                            "可信连接收到意外消息",
                            false,
                        );
                        self.telemetry.emit(&app);
                    }
                    return Err("可信连接收到意外消息".into());
                }
                Err(error) => {
                    fail_pending_delivery_receipts(&pending_receipts, &error);
                    writer.abort();
                    blob_reader.abort();
                    file_reader.abort();
                    if self.remove_peer_if_current(&device_id, &connection) {
                        let state = app.state::<ServiceState>();
                        let _ = service::set_trusted_online(&state, &app, &device_id, false);
                        self.telemetry.mark_disconnected(
                            &device_id,
                            &connection,
                            disconnect_code(&error),
                            error.clone(),
                            false,
                        );
                        self.telemetry.emit(&app);
                    }
                    connection.close(0u32.into(), b"closed");
                    return Err(error);
                }
            }
        }
    }
}

fn register_delivery_receipt(
    pending: &PendingDeliveryReceipts,
    message_id: &str,
) -> Result<oneshot::Receiver<DeliveryReceipt>, String> {
    let (sender, receiver) = oneshot::channel();
    let mut receipts = pending
        .lock()
        .map_err(|_| "传输回执状态锁已损坏".to_string())?;
    if receipts.contains_key(message_id) {
        return Err("传输消息 ID 出现重复".into());
    }
    receipts.insert(message_id.to_string(), sender);
    Ok(receiver)
}

fn cancel_delivery_receipt(pending: &PendingDeliveryReceipts, message_id: &str) {
    if let Ok(mut receipts) = pending.lock() {
        receipts.remove(message_id);
    }
}

fn complete_delivery_receipt(
    pending: &PendingDeliveryReceipts,
    message_id: &str,
    accepted: bool,
    message: Option<String>,
    processing_ms: Option<u64>,
) -> bool {
    pending
        .lock()
        .ok()
        .and_then(|mut receipts| receipts.remove(message_id))
        .is_some_and(|sender| {
            sender
                .send(DeliveryReceipt {
                    accepted,
                    message,
                    processing_ms,
                })
                .is_ok()
        })
}

fn fail_pending_delivery_receipts(pending: &PendingDeliveryReceipts, reason: &str) {
    let senders = pending
        .lock()
        .map(|mut receipts| {
            receipts
                .drain()
                .map(|(_, sender)| sender)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for sender in senders {
        let _ = sender.send(DeliveryReceipt {
            accepted: false,
            message: Some(reason.to_string()),
            processing_ms: None,
        });
    }
}

async fn finish_after_delivery_receipt(
    pending: PendingDeliveryReceipts,
    message_id: String,
    receiver: oneshot::Receiver<DeliveryReceipt>,
    telemetry: TelemetryStore,
    transfer_key: String,
) {
    match tokio::time::timeout(DELIVERY_RECEIPT_TIMEOUT, receiver).await {
        Ok(Ok(receipt)) => {
            telemetry.set_remote_processing(&transfer_key, receipt.processing_ms);
            telemetry.finish_transfer(
                &transfer_key,
                receipt.accepted,
                receipt.message.or_else(|| {
                    Some(if receipt.accepted {
                        "对端已确认接收".into()
                    } else {
                        "对端拒绝了同步内容".into()
                    })
                }),
            );
        }
        Ok(Err(_)) => telemetry.finish_transfer(
            &transfer_key,
            false,
            Some("可信连接在返回接收确认前已关闭".into()),
        ),
        Err(_) => {
            cancel_delivery_receipt(&pending, &message_id);
            telemetry.finish_transfer(&transfer_key, false, Some("等待对端接收确认超时".into()));
        }
    }
}

fn trusted_message_transfer(message: &TrustedMessage) -> Option<(String, &'static str, u64)> {
    match message {
        TrustedMessage::ClipboardSlotOffer {
            message_id, text, ..
        } => Some((
            message_id.clone(),
            service::text_content_type(text),
            text.len() as u64,
        )),
        TrustedMessage::RichClipboardSlotOffer {
            message_id,
            text,
            html,
            rtf,
            ..
        } => Some((
            message_id.clone(),
            "html",
            text.len()
                .saturating_add(html.as_ref().map_or(0, String::len))
                .saturating_add(rtf.as_ref().map_or(0, String::len)) as u64,
        )),
        _ => None,
    }
}

fn is_clipboard_offer(message: &TrustedMessage) -> bool {
    matches!(
        message,
        TrustedMessage::ClipboardSlotOffer { .. } | TrustedMessage::RichClipboardSlotOffer { .. }
    )
}

fn trusted_message_size(message: &TrustedMessage) -> u64 {
    trusted_message_transfer(message).map_or(0, |(_, _, size)| size)
}

fn should_replay_sequence(sequence: u64, remote_received_sequence: Option<u64>) -> bool {
    remote_received_sequence.is_none_or(|received| sequence > received)
}

async fn send_image_blob(
    connection: Connection,
    offer: LocalImageOffer,
    group_ids: Vec<String>,
    message_id: String,
    telemetry: &TelemetryStore,
    transfer_key: &str,
) -> Result<(), String> {
    let mut send = connection
        .open_uni()
        .await
        .map_err(|error| format!("无法打开图片数据流：{error}"))?;
    let header = ImageBlobHeader {
        schema_version: 1,
        message_id,
        origin_sequence: offer.sequence,
        captured_at: offer.captured_at,
        width: offer.width,
        height: offer.height,
        png_length: offer.png.len() as u64,
        sha256: HEXLOWER.encode(&Sha256::digest(offer.png.as_slice())),
        group_ids,
    };
    send.write_u8(IMAGE_BLOB_KIND)
        .await
        .map_err(|error| format!("图片数据流类型发送失败：{error}"))?;
    write_frame(&mut send, &header).await?;
    let mut transferred = 0_u64;
    for chunk in offer.png.chunks(64 * 1024) {
        send.write_all(chunk)
            .await
            .map_err(|error| format!("图片数据发送失败：{error}"))?;
        transferred = transferred.saturating_add(chunk.len() as u64);
        telemetry.update_transfer(transfer_key, transferred);
    }
    send.finish()
        .map_err(|error| format!("图片数据流结束失败：{error}"))
}

async fn send_file_blob(
    connection: Connection,
    offer: LocalFileOffer,
    group_ids: Vec<String>,
    telemetry: &TelemetryStore,
    transfer_key: &str,
) -> Result<(), String> {
    let (mut send, mut receive) = connection
        .open_bi()
        .await
        .map_err(|error| format!("无法打开文件数据流：{error}"))?;
    let header = FileBlobHeader {
        schema_version: 1,
        message_id: offer.transfer_id.clone(),
        origin_sequence: offer.sequence,
        captured_at: offer.captured_at,
        total_size: offer.bundle.total_size,
        entries: offer.bundle.entries.clone(),
        group_ids,
    };
    send.write_u8(FILE_BLOB_KIND)
        .await
        .map_err(|error| format!("文件数据流类型发送失败：{error}"))?;
    write_frame(&mut send, &header).await?;
    let plan: FileResumePlan = read_frame(&mut receive).await?;
    if plan.schema_version != 1
        || plan.transfer_id != header.message_id
        || plan.offsets.len() != header.entries.len()
    {
        return Err("文件续传计划无效".into());
    }
    let mut transferred = header
        .entries
        .iter()
        .zip(&plan.offsets)
        .map(|(entry, offset)| (*offset).min(entry.size))
        .sum::<u64>();
    telemetry.set_transfer_baseline(transfer_key, transferred);
    let mut buffer = vec![0_u8; 64 * 1024];
    for (entry, offset) in header.entries.iter().zip(plan.offsets) {
        if entry.is_directory {
            if offset != 0 {
                return Err("目录续传偏移无效".into());
            }
            continue;
        }
        if offset > entry.size {
            return Err("文件续传偏移超过声明大小".into());
        }
        let relative = safe_relative_path(&entry.relative_path)?;
        let mut file = tokio::fs::File::open(offer.bundle.root.join(relative))
            .await
            .map_err(|error| format!("无法打开暂存文件：{error}"))?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|error| format!("无法定位暂存文件续传位置：{error}"))?;
        let mut remaining = entry.size - offset;
        while remaining > 0 {
            let limit = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| "文件分块大小溢出".to_string())?;
            let read = file
                .read(&mut buffer[..limit])
                .await
                .map_err(|error| format!("无法读取暂存文件：{error}"))?;
            if read == 0 {
                return Err("暂存文件在发送时被截断".into());
            }
            send.write_all(&buffer[..read])
                .await
                .map_err(|error| format!("文件数据发送失败：{error}"))?;
            remaining -= read as u64;
            transferred = transferred.saturating_add(read as u64);
            telemetry.update_transfer(transfer_key, transferred);
        }
    }
    send.finish()
        .map_err(|error| format!("文件数据流结束失败：{error}"))?;
    telemetry.mark_network_complete(transfer_key);
    let acknowledgement: FileTransferAck = read_frame(&mut receive).await?;
    telemetry.set_remote_processing(transfer_key, acknowledgement.processing_ms);
    if acknowledgement.schema_version != 1
        || acknowledgement.transfer_id != header.message_id
        || !acknowledgement.accepted
    {
        return Err(acknowledgement
            .message
            .unwrap_or_else(|| "接收端拒绝了文件快照".into()));
    }
    Ok(())
}

async fn send_file_blob_with_retry(
    connection: Connection,
    offer: LocalFileOffer,
    group_ids: Vec<String>,
    telemetry: &TelemetryStore,
    transfer_key: &str,
) -> Result<(), String> {
    match send_file_blob(
        connection.clone(),
        offer.clone(),
        group_ids.clone(),
        telemetry,
        transfer_key,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(first_error) => {
            if connection.close_reason().is_some() {
                return Err(first_error);
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
            send_file_blob(connection, offer, group_ids, telemetry, transfer_key)
                .await
                .map_err(|second_error| {
                    format!("文件流首次发送失败：{first_error}；重试失败：{second_error}")
                })
        }
    }
}

async fn receive_clipboard_blobs(
    app: AppHandle,
    connection: Connection,
    device: TrustedDevice,
    telemetry: TelemetryStore,
    receipt_sender: Option<mpsc::UnboundedSender<TrustedMessage>>,
) {
    loop {
        let mut receive = match connection.accept_uni().await {
            Ok(receive) => receive,
            Err(_) => return,
        };
        let result = match receive.read_u8().await {
            Ok(IMAGE_BLOB_KIND) => {
                receive_image_blob(
                    &app,
                    &device,
                    &mut receive,
                    &telemetry,
                    receipt_sender.as_ref(),
                )
                .await
            }
            Ok(FILE_BLOB_KIND) => Err("文件剪贴板必须使用可续传双向流".into()),
            Ok(_) => Err("未知剪贴板数据流类型".into()),
            Err(error) => Err(format!("无法读取剪贴板数据流类型：{error}")),
        };
        if let Err(error) = result {
            tracing::warn!(device_id = %device.device_id, error = %error, "remote clipboard blob rejected");
        }
    }
}

async fn receive_file_streams(
    app: AppHandle,
    connection: Connection,
    device: TrustedDevice,
    telemetry: TelemetryStore,
) {
    loop {
        let (mut send, mut receive) = match connection.accept_bi().await {
            Ok(streams) => streams,
            Err(_) => return,
        };
        let result = match receive.read_u8().await {
            Ok(FILE_BLOB_KIND) => {
                receive_file_blob(&app, &device, &mut send, &mut receive, &telemetry).await
            }
            Ok(_) => Err("可信连接收到未知双向数据流".into()),
            Err(error) => Err(format!("无法读取双向数据流类型：{error}")),
        };
        if let Err(error) = result {
            tracing::warn!(device_id = %device.device_id, error = %error, "remote resumable file stream rejected");
        }
    }
}

async fn receive_image_blob(
    app: &AppHandle,
    device: &TrustedDevice,
    receive: &mut quinn::RecvStream,
    telemetry: &TelemetryStore,
    receipt_sender: Option<&mpsc::UnboundedSender<TrustedMessage>>,
) -> Result<(), String> {
    let header: ImageBlobHeader = read_frame(receive).await?;
    let message_id = header.message_id.clone();
    let transfer_key = telemetry.start_transfer(
        message_id.clone(),
        device.device_id.clone(),
        "download",
        "image",
        header.png_length,
    );
    let mut processing_ms = None;
    let result = async {
        if !ClipboardCapabilities::local().images {
            return Err("本机平台不支持图片剪贴板".into());
        }
        if header.schema_version != 1
            || header.png_length == 0
            || header.png_length as usize > MAX_IMAGE_BLOB
            || header.width == 0
            || header.height == 0
        {
            return Err("图片数据流头无效".to_string());
        }
        {
            let state = app.state::<ServiceState>();
            state.validate_incoming_offer(&device.device_id, &header.group_ids, "image")?;
        }
        let mut png = Vec::with_capacity(header.png_length as usize);
        let mut buffer = vec![0_u8; 64 * 1024];
        while png.len() < header.png_length as usize {
            let remaining = header.png_length as usize - png.len();
            let limit = remaining.min(buffer.len());
            let read = receive
                .read(&mut buffer[..limit])
                .await
                .map_err(|error| format!("图片数据读取失败：{error}"))?
                .ok_or_else(|| "图片数据流提前结束".to_string())?;
            png.extend_from_slice(&buffer[..read]);
            telemetry.update_transfer(&transfer_key, png.len() as u64);
        }
        if !receive
            .read_to_end(1)
            .await
            .map_err(|error| format!("图片数据流尾部无效：{error}"))?
            .is_empty()
        {
            return Err("图片数据流包含未声明内容".into());
        }
        telemetry.mark_network_complete(&transfer_key);
        let processing_started = Instant::now();
        let processing_result = (|| {
            if HEXLOWER.encode(&Sha256::digest(&png)) != header.sha256 {
                return Err("图片数据长度或哈希不匹配".into());
            }
            let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
                .map_err(|error| format!("图片数据解码失败：{error}"))?
                .to_rgba8();
            if decoded.width() != header.width || decoded.height() != header.height {
                return Err("图片数据尺寸与声明不匹配".into());
            }
            let state = app.state::<ServiceState>();
            service::receive_remote_image(
                &state,
                app,
                device,
                service::RemoteImage {
                    sequence: header.origin_sequence,
                    rgba: decoded.into_raw(),
                    width: header.width,
                    height: header.height,
                    captured_at: header.captured_at,
                    group_ids: header.group_ids,
                },
            )
        })();
        processing_ms = Some(elapsed_millis(processing_started.elapsed()));
        processing_result
    }
    .await;
    let accepted = result.is_ok();
    let receipt_message = result.as_ref().err().cloned();
    telemetry.finish_transfer(
        &transfer_key,
        accepted,
        receipt_message
            .clone()
            .or_else(|| Some("图片已写入设备槽位".into())),
    );
    if let Some(sender) = receipt_sender {
        let _ = sender.send(TrustedMessage::ClipboardDeliveryAck {
            schema_version: 1,
            message_id,
            accepted,
            message: receipt_message,
            processing_ms,
        });
    }
    result
}

async fn receive_file_blob(
    app: &AppHandle,
    device: &TrustedDevice,
    send: &mut quinn::SendStream,
    receive: &mut quinn::RecvStream,
    telemetry: &TelemetryStore,
) -> Result<(), String> {
    if !ClipboardCapabilities::local().files {
        return Err("本机平台不支持文件剪贴板".into());
    }
    let header: FileBlobHeader = read_frame(receive).await?;
    validate_file_header(&header)?;
    let transfer_id = uuid::Uuid::parse_str(&header.message_id)
        .map_err(|_| "文件数据流消息 ID 无效".to_string())?
        .simple()
        .to_string();
    let (incoming_root, already_accepted) = {
        let state = app.state::<ServiceState>();
        state.validate_incoming_offer(&device.device_id, &header.group_ids, "files")?;
        let accepted = state.has_accepted_file_transfer(&device.device_id, &transfer_id);
        if !accepted {
            state.validate_incoming_sequence(&device.device_id, header.origin_sequence)?;
        }
        let peer_key = HEXLOWER.encode(&Sha256::digest(device.device_id.as_bytes())[..12]);
        (state.incoming_files_root().join(peer_key), accepted)
    };
    if already_accepted {
        let transfer_key = telemetry.start_transfer(
            header.message_id.clone(),
            device.device_id.clone(),
            "download",
            "files",
            header.total_size,
        );
        telemetry.set_transfer_baseline(&transfer_key, header.total_size);
        let result = async {
            write_frame(
                send,
                &FileResumePlan {
                    schema_version: 1,
                    transfer_id: header.message_id.clone(),
                    offsets: header
                        .entries
                        .iter()
                        .map(|entry| if entry.is_directory { 0 } else { entry.size })
                        .collect(),
                },
            )
            .await?;
            if !receive
                .read_to_end(1)
                .await
                .map_err(|error| format!("无法确认重复文件流：{error}"))?
                .is_empty()
            {
                return Err("重复文件流包含未声明正文".into());
            }
            telemetry.mark_network_complete(&transfer_key);
            write_frame(
                send,
                &FileTransferAck {
                    schema_version: 1,
                    transfer_id: header.message_id,
                    accepted: true,
                    message: None,
                    processing_ms: Some(0),
                },
            )
            .await?;
            send.finish()
                .map_err(|error| format!("无法结束重复文件确认流：{error}"))
        }
        .await;
        telemetry.finish_transfer(
            &transfer_key,
            result.is_ok(),
            result
                .as_ref()
                .err()
                .cloned()
                .or_else(|| Some("文件已存在，无需重复接收".into())),
        );
        return result;
    }
    create_private_dir_all(&incoming_root).await?;
    let temporary = incoming_root.join(format!(".part-{transfer_id}"));
    let completed = incoming_root.join(format!("bundle-{transfer_id}"));
    let partial = prepare_partial_file_bundle(&temporary, &completed, &header).await?;
    write_frame(
        send,
        &FileResumePlan {
            schema_version: 1,
            transfer_id: header.message_id.clone(),
            offsets: partial.offsets.clone(),
        },
    )
    .await?;
    let transfer_key = telemetry.start_transfer(
        header.message_id.clone(),
        device.device_id.clone(),
        "download",
        "files",
        header.total_size,
    );
    let mut processing_ms = None;
    let result = async {
        receive_file_entries(
            receive,
            &partial.root,
            &header,
            &partial.offsets,
            telemetry,
            &transfer_key,
        )
        .await?;
        telemetry.mark_network_complete(&transfer_key);
        let processing_started = Instant::now();
        let processing_result = async {
            if !partial.completed {
                tokio::fs::rename(&temporary, &completed)
                    .await
                    .map_err(|error| format!("无法提交接收文件：{error}"))?;
            }
            let bundle = Arc::new(ReceivedFileBundle::new(
                completed.clone(),
                header.entries.clone(),
            ));
            let state = app.state::<ServiceState>();
            service::receive_remote_files(
                &state,
                app,
                device,
                service::RemoteFiles {
                    sequence: header.origin_sequence,
                    bundle,
                    captured_at: header.captured_at.clone(),
                    group_ids: header.group_ids.clone(),
                    total_size: header.total_size,
                },
            )
        }
        .await;
        processing_ms = Some(elapsed_millis(processing_started.elapsed()));
        processing_result
    }
    .await;
    telemetry.finish_transfer(
        &transfer_key,
        result.is_ok(),
        result
            .as_ref()
            .err()
            .cloned()
            .or_else(|| Some("文件已写入设备槽位".into())),
    );
    if result.is_ok() {
        let state = app.state::<ServiceState>();
        state.mark_accepted_file_transfer(&device.device_id, transfer_id);
    }
    let acknowledgement = FileTransferAck {
        schema_version: 1,
        transfer_id: header.message_id,
        accepted: result.is_ok(),
        message: result.as_ref().err().cloned(),
        processing_ms,
    };
    write_frame(send, &acknowledgement).await?;
    send.finish()
        .map_err(|error| format!("无法结束文件确认流：{error}"))?;
    result
}

struct PartialFileBundle {
    root: std::path::PathBuf,
    offsets: Vec<u64>,
    completed: bool,
}

async fn prepare_partial_file_bundle(
    temporary: &std::path::Path,
    completed: &std::path::Path,
    header: &FileBlobHeader,
) -> Result<PartialFileBundle, String> {
    let completed_exists = completed.exists();
    let root = if completed_exists {
        completed.to_path_buf()
    } else {
        temporary.to_path_buf()
    };
    create_private_dir_all(&root).await?;
    let manifest_path = root.join(".localdrop-manifest.json");
    let manifest =
        serde_json::to_vec(&(header.origin_sequence, header.total_size, &header.entries))
            .map_err(|error| format!("无法编码文件续传清单：{error}"))?;
    if manifest_path.exists() {
        let existing = tokio::fs::read(&manifest_path)
            .await
            .map_err(|error| format!("无法读取文件续传清单：{error}"))?;
        if existing != manifest {
            return Err("相同文件传输 ID 出现不同清单".into());
        }
    } else {
        write_private_file(&manifest_path, &manifest).await?;
    }
    let mut offsets = Vec::with_capacity(header.entries.len());
    for entry in &header.entries {
        let destination = root.join(safe_relative_path(&entry.relative_path)?);
        if entry.is_directory {
            create_private_dir_all(&destination).await?;
            offsets.push(0);
            continue;
        }
        if let Some(parent) = destination.parent() {
            create_private_dir_all(parent).await?;
        }
        let offset = match tokio::fs::symlink_metadata(&destination).await {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err("文件续传缓存包含不安全文件类型".into());
            }
            Ok(metadata) if metadata.len() <= entry.size => metadata.len(),
            Ok(_) => {
                tokio::fs::remove_file(&destination)
                    .await
                    .map_err(|error| format!("无法重置超长续传文件：{error}"))?;
                0
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(format!("无法检查文件续传状态：{error}")),
        };
        offsets.push(offset);
    }
    Ok(PartialFileBundle {
        root,
        offsets,
        completed: completed_exists,
    })
}

fn validate_file_header(header: &FileBlobHeader) -> Result<(), String> {
    if header.schema_version != 1
        || header.entries.is_empty()
        || header.entries.len() > MAX_FILE_ENTRIES
        || header.total_size > MAX_FILE_BUNDLE_BYTES
    {
        return Err("文件数据流头无效".into());
    }
    let mut paths = HashSet::new();
    let mut total_size = 0_u64;
    for entry in &header.entries {
        safe_relative_path(&entry.relative_path)?;
        if !paths.insert(entry.relative_path.clone()) {
            return Err("文件清单包含重复路径".into());
        }
        if entry.is_directory {
            if entry.size != 0 || !entry.sha256.is_empty() {
                return Err("目录清单字段无效".into());
            }
        } else {
            if entry.sha256.len() != 64
                || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err("文件哈希格式无效".into());
            }
            total_size = total_size
                .checked_add(entry.size)
                .ok_or_else(|| "文件总大小溢出".to_string())?;
        }
    }
    if total_size != header.total_size {
        return Err("文件清单总大小不匹配".into());
    }
    Ok(())
}

async fn receive_file_entries(
    receive: &mut quinn::RecvStream,
    root: &std::path::Path,
    header: &FileBlobHeader,
    offsets: &[u64],
    telemetry: &TelemetryStore,
    transfer_key: &str,
) -> Result<(), String> {
    let mut transferred = header
        .entries
        .iter()
        .zip(offsets)
        .map(|(entry, offset)| (*offset).min(entry.size))
        .sum::<u64>();
    telemetry.set_transfer_baseline(transfer_key, transferred);
    let mut buffer = vec![0_u8; 64 * 1024];
    for (entry, offset) in header.entries.iter().zip(offsets) {
        let relative = safe_relative_path(&entry.relative_path)?;
        let destination = root.join(relative);
        if entry.is_directory {
            create_private_dir_all(&destination).await?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            create_private_dir_all(parent).await?;
        }
        let mut options = tokio::fs::OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options
            .open(&destination)
            .await
            .map_err(|error| format!("无法打开接收文件：{error}"))?;
        let mut hash = Sha256::new();
        let mut prefix_remaining = *offset;
        while prefix_remaining > 0 {
            let limit = usize::try_from(prefix_remaining.min(buffer.len() as u64))
                .map_err(|_| "文件续传前缀大小溢出".to_string())?;
            let read = file
                .read(&mut buffer[..limit])
                .await
                .map_err(|error| format!("无法校验文件续传前缀：{error}"))?;
            if read == 0 {
                return Err("文件续传前缀被意外截断".into());
            }
            hash.update(&buffer[..read]);
            prefix_remaining -= read as u64;
        }
        file.seek(std::io::SeekFrom::Start(*offset))
            .await
            .map_err(|error| format!("无法定位文件续传写入位置：{error}"))?;
        let mut remaining = entry.size - *offset;
        while remaining > 0 {
            let limit = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| "文件分块大小溢出".to_string())?;
            let read = receive
                .read(&mut buffer[..limit])
                .await
                .map_err(|error| format!("文件数据读取失败：{error}"))?
                .ok_or_else(|| "文件数据流提前结束".to_string())?;
            hash.update(&buffer[..read]);
            file.write_all(&buffer[..read])
                .await
                .map_err(|error| format!("文件数据写入失败：{error}"))?;
            remaining -= read as u64;
            transferred = transferred.saturating_add(read as u64);
            telemetry.update_transfer(transfer_key, transferred);
        }
        file.sync_all()
            .await
            .map_err(|error| format!("文件数据提交失败：{error}"))?;
        if HEXLOWER.encode(&hash.finalize()) != entry.sha256.to_ascii_lowercase() {
            drop(file);
            let _ = tokio::fs::remove_file(&destination).await;
            return Err(format!("文件 {} 哈希不匹配", entry.relative_path));
        }
    }
    let trailing = receive
        .read_to_end(1)
        .await
        .map_err(|error| format!("文件数据流尾部无效：{error}"))?;
    if !trailing.is_empty() {
        return Err("文件数据流包含未声明内容".into());
    }
    Ok(())
}

async fn create_private_dir_all(path: &std::path::Path) -> Result<(), String> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|error| format!("无法创建私有文件缓存目录：{error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|error| format!("无法限制文件缓存目录权限：{error}"))?;
    }
    Ok(())
}

async fn write_private_file(path: &std::path::Path, contents: &[u8]) -> Result<(), String> {
    let mut options = tokio::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .await
        .map_err(|error| format!("无法创建私有文件缓存对象：{error}"))?;
    file.write_all(contents)
        .await
        .map_err(|error| format!("无法写入私有文件缓存对象：{error}"))?;
    file.sync_all()
        .await
        .map_err(|error| format!("无法提交私有文件缓存对象：{error}"))
}

pub(crate) fn start(app: AppHandle) -> Result<TransportHandle, String> {
    let (certificate_der, private_key_der) = {
        let state = app.state::<ServiceState>();
        let private_key_der = state.identity().pkcs8_der();
        let key_pair = KeyPair::try_from(private_key_der.clone())
            .map_err(|error| format!("无法载入 TLS 身份密钥：{error}"))?;
        let certificate = CertificateParams::new(vec![state.device_id().to_string()])
            .map_err(|error| format!("无法创建 TLS 证书参数：{error}"))?
            .self_signed(&key_pair)
            .map_err(|error| format!("无法签发 TLS 证书：{error}"))?;
        (certificate.der().to_vec(), private_key_der)
    };
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let app_for_thread = app.clone();
    let certificate_for_thread = certificate_der.clone();
    std::thread::Builder::new()
        .name("airdrop-transport".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("airdrop-network")
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready_tx.send(Err(format!("无法启动网络运行时：{error}")));
                    return;
                }
            };
            runtime.block_on(async move {
                let result = create_endpoint(
                    certificate_for_thread.clone(),
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(private_key_der.clone())),
                );
                let endpoint = match result {
                    Ok(endpoint) => endpoint,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                let telemetry = {
                    let state = app_for_thread.state::<ServiceState>();
                    match TelemetryStore::with_store(state.store()) {
                        Ok(telemetry) => telemetry,
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                            return;
                        }
                    }
                };
                let telemetry_notify = telemetry.notifier();
                let handle = TransportHandle {
                    runtime: tokio::runtime::Handle::current(),
                    endpoint: endpoint.clone(),
                    certificate_der: certificate_for_thread,
                    private_key_der,
                    active: Arc::new(AtomicBool::new(true)),
                    runtime_generation: Arc::new(AtomicU64::new(0)),
                    pairing_allowed_until: Arc::new(Mutex::new(0)),
                    pair_commands: Arc::new(Mutex::new(HashMap::new())),
                    pairing_connecting: Arc::new(Mutex::new(HashMap::new())),
                    peers: Arc::new(Mutex::new(HashMap::new())),
                    preferred_addresses: Arc::new(Mutex::new(HashMap::new())),
                    connecting: Arc::new(Mutex::new(HashMap::new())),
                    latest_offer: Arc::new(Mutex::new(None)),
                    latest_rich: Arc::new(Mutex::new(None)),
                    latest_image: Arc::new(Mutex::new(None)),
                    latest_files: Arc::new(Mutex::new(None)),
                    latest_content_sequence: Arc::new(AtomicU64::new(0)),
                    telemetry_observed: Arc::new(AtomicBool::new(false)),
                    telemetry_notify,
                    telemetry,
                };
                let telemetry_handle = handle.clone();
                let telemetry_app = app_for_thread.clone();
                tokio::spawn(async move {
                    loop {
                        let observing = telemetry_handle.telemetry_observed.load(Ordering::Acquire);
                        if observing && telemetry_handle.is_active() {
                            telemetry_handle.sample_telemetry();
                            telemetry_handle.telemetry.emit(&telemetry_app);
                        }
                        let delay = if observing {
                            Duration::from_secs(1)
                        } else {
                            Duration::from_secs(30)
                        };
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => {}
                            _ = telemetry_handle.telemetry_notify.notified() => {}
                        }
                    }
                });
                let _ = ready_tx.send(Ok(handle.clone()));
                loop {
                    let generation = handle.runtime_generation();
                    let Some(incoming) = endpoint.accept().await else {
                        break;
                    };
                    let handle = handle.clone();
                    let app = app_for_thread.clone();
                    tokio::spawn(async move {
                        if let Err(error) =
                            accept_connection(handle, app, incoming, generation).await
                        {
                            tracing::debug!(error = %error, "incoming connection rejected");
                        }
                    });
                }
            });
        })
        .map_err(|error| format!("无法启动网络线程：{error}"))?;
    ready_rx
        .recv()
        .map_err(|_| "网络线程启动失败".to_string())?
}

#[tauri::command]
pub(crate) fn get_telemetry(state: State<'_, TransportHandle>) -> TelemetrySnapshot {
    state.telemetry_snapshot()
}

#[tauri::command]
pub(crate) fn set_telemetry_observing(
    state: State<'_, TransportHandle>,
    app: AppHandle,
    observing: bool,
) {
    state.set_telemetry_observing(&app, observing);
}

async fn accept_connection(
    handle: TransportHandle,
    app: AppHandle,
    incoming: quinn::Incoming,
    generation: u64,
) -> Result<(), String> {
    if !handle.is_active_generation(generation) {
        return Err("移动端当前处于后台暂停状态".into());
    }
    let connection = incoming
        .await
        .map_err(|error| format!("QUIC 握手失败：{error}"))?;
    if !handle.is_active_generation(generation) {
        connection.close(4u32.into(), b"mobile runtime suspended");
        return Err("移动端当前处于后台暂停状态".into());
    }
    let handshake = connection
        .handshake_data()
        .ok_or_else(|| "缺少 TLS 握手信息".to_string())?
        .downcast::<quinn::crypto::rustls::HandshakeData>()
        .map_err(|_| "TLS 握手信息类型无效".to_string())?;
    let alpn = handshake.protocol.as_deref();
    if alpn == Some(PAIR_ALPN) {
        let pairing_allowed = {
            let allowed = handle
                .pairing_allowed_until
                .lock()
                .map_err(|_| "配对窗口状态锁已损坏".to_string())?;
            *allowed >= unix_seconds()
        };
        if !pairing_allowed {
            connection.close(1u32.into(), b"pairing window closed");
            return Err("当前未开放配对窗口".into());
        }
        accept_pairing(handle, app, connection, generation).await
    } else if alpn == Some(TRUSTED_ALPN) {
        let (send, receive) = connection
            .accept_bi()
            .await
            .map_err(|error| format!("无法接受可信控制流：{error}"))?;
        handle
            .run_trusted(app, connection, send, receive, None, generation)
            .await
    } else {
        connection.close(2u32.into(), b"unsupported alpn");
        Err("不支持的应用协议".into())
    }
}

async fn accept_pairing(
    handle: TransportHandle,
    app: AppHandle,
    connection: Connection,
    generation: u64,
) -> Result<(), String> {
    let (mut send, mut receive) = connection
        .accept_bi()
        .await
        .map_err(|error| format!("无法接受配对控制流：{error}"))?;
    let init: PairMessage = read_frame(&mut receive).await?;
    let PairMessage::Init {
        schema_version: 1,
        pairing_id,
        nonce,
        device_id,
        device_name,
        platform,
        public_key,
        certificate,
    } = init
    else {
        return Err("配对请求类型或版本无效".into());
    };
    let initiator_nonce = decode(&nonce, "发起方随机数")?;
    if initiator_nonce.len() != 32 {
        return Err("发起方随机数长度无效".into());
    }
    let public_key = validate_identity(&device_id, &public_key)?;
    let certificate_der = decode(&certificate, "设备证书")?;
    if certificate_der != peer_certificate(&connection)? {
        return Err("配对身份与 TLS 客户端证书不一致".into());
    }
    let responder_nonce = random_bytes(32);
    let (hello, local_device_id) = {
        let state = app.state::<ServiceState>();
        (
            PairMessage::Hello {
                schema_version: 1,
                pairing_id: pairing_id.clone(),
                initiator_nonce: nonce,
                responder_nonce: BASE64.encode(&responder_nonce),
                device_id: state.device_id().to_string(),
                device_name: state.device_name()?,
                platform: platform::platform_name().to_string(),
                public_key: BASE64.encode(&state.identity().public_key_bytes()),
                certificate: BASE64.encode(&handle.certificate_der),
            },
            state.device_id().to_string(),
        )
    };
    write_frame(&mut send, &hello).await?;
    let context = pairing_context(
        &pairing_id,
        &initiator_nonce,
        &responder_nonce,
        &device_id,
        &local_device_id,
    );
    let device = TrustedDevice {
        device_id,
        device_name,
        local_alias: None,
        platform,
        public_key,
        certificate_der,
        paired_at: now(),
        sync_enabled: true,
    };
    handle
        .run_pair_confirmation(
            app, connection, send, receive, device, pairing_id, context, "incoming", generation,
        )
        .await
}

fn create_endpoint(
    certificate_der: Vec<u8>,
    private_key: PrivateKeyDer<'static>,
) -> Result<Endpoint, String> {
    let server = server_config(certificate_der, private_key)?;
    Endpoint::server(
        server,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), TRANSPORT_PORT),
    )
    .map_err(|error| format!("无法监听 QUIC 端口 {TRANSPORT_PORT}：{error}"))
}

fn server_config(
    certificate_der: Vec<u8>,
    private_key: PrivateKeyDer<'static>,
) -> Result<quinn::ServerConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut crypto = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| format!("无法限定 TLS 1.3：{error}"))?
        .with_client_cert_verifier(Arc::new(AnyEd25519ClientCertificate::new()))
        .with_single_cert(vec![CertificateDer::from(certificate_der)], private_key)
        .map_err(|error| format!("无法配置 TLS 证书：{error}"))?;
    crypto.alpn_protocols = vec![TRUSTED_ALPN.to_vec(), PAIR_ALPN.to_vec()];
    crypto.max_early_data_size = 0;
    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(crypto)
        .map_err(|error| format!("无法配置 QUIC TLS：{error}"))?;
    let mut server = quinn::ServerConfig::with_crypto(Arc::new(quic));
    server.transport_config(persistent_transport_config()?);
    Ok(server)
}

fn client_config(
    expected_certificate: Option<Vec<u8>>,
    alpn: &[u8],
    certificate_der: Vec<u8>,
    private_key_der: Vec<u8>,
) -> Result<quinn::ClientConfig, String> {
    let verifier = Arc::new(PinnedCertificateVerifier::new(expected_certificate));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut crypto = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| format!("无法限定 TLS 1.3：{error}"))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(
            vec![CertificateDer::from(certificate_der)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(private_key_der)),
        )
        .map_err(|error| format!("无法配置 TLS 客户端身份：{error}"))?;
    crypto.alpn_protocols = vec![alpn.to_vec()];
    crypto.enable_early_data = false;
    let quic = QuicClientConfig::try_from(crypto)
        .map_err(|error| format!("无法配置 QUIC 客户端：{error}"))?;
    let mut client = quinn::ClientConfig::new(Arc::new(quic));
    client.transport_config(persistent_transport_config()?);
    Ok(client)
}

fn persistent_transport_config() -> Result<Arc<TransportConfig>, String> {
    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(CONNECTION_KEEP_ALIVE_INTERVAL));
    transport.max_idle_timeout(Some(
        CONNECTION_IDLE_TIMEOUT
            .try_into()
            .map_err(|_| "可信连接空闲超时配置无效".to_string())?,
    ));
    Ok(Arc::new(transport))
}

#[derive(Debug)]
struct AnyEd25519ClientCertificate {
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl AnyEd25519ClientCertificate {
    fn new() -> Self {
        Self {
            provider: Arc::new(rustls::crypto::ring::default_provider()),
        }
    }
}

impl ClientCertVerifier for AnyEd25519ClientCertificate {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        if end_entity.is_empty() || end_entity.len() > 64 * 1024 || !intermediates.is_empty() {
            return Err(rustls::Error::General(
                "客户端必须提供单个有界 Ed25519 证书".into(),
            ));
        }
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_ed25519_tls12_signature(message, cert, dss, &self.provider)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_ed25519_tls13_signature(message, cert, dss, &self.provider)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

fn verify_ed25519_tls12_signature(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
    provider: &rustls::crypto::CryptoProvider,
) -> Result<HandshakeSignatureValid, rustls::Error> {
    if dss.scheme != SignatureScheme::ED25519 {
        return Err(rustls::Error::General("只允许 Ed25519 TLS 身份签名".into()));
    }
    rustls::crypto::verify_tls12_signature(
        message,
        cert,
        dss,
        &provider.signature_verification_algorithms,
    )
}

fn verify_ed25519_tls13_signature(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
    provider: &rustls::crypto::CryptoProvider,
) -> Result<HandshakeSignatureValid, rustls::Error> {
    if dss.scheme != SignatureScheme::ED25519 {
        return Err(rustls::Error::General("只允许 Ed25519 TLS 身份签名".into()));
    }
    rustls::crypto::verify_tls13_signature(
        message,
        cert,
        dss,
        &provider.signature_verification_algorithms,
    )
}

#[derive(Debug)]
struct PinnedCertificateVerifier {
    expected: Option<Vec<u8>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl PinnedCertificateVerifier {
    fn new(expected: Option<Vec<u8>>) -> Self {
        Self {
            expected,
            provider: Arc::new(rustls::crypto::ring::default_provider()),
        }
    }
}

impl ServerCertVerifier for PinnedCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if end_entity.is_empty() || end_entity.len() > 64 * 1024 || !intermediates.is_empty() {
            return Err(rustls::Error::General(
                "服务端必须提供单个有界 Ed25519 证书".into(),
            ));
        }
        if self
            .expected
            .as_ref()
            .is_some_and(|expected| expected.as_slice() != end_entity.as_ref())
        {
            return Err(rustls::Error::General("可信设备证书固定校验失败".into()));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_ed25519_tls12_signature(message, cert, dss, &self.provider)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_ed25519_tls13_signature(message, cert, dss, &self.provider)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

fn candidate_addresses(
    nearby: &service::NearbyDevice,
    preferred: Option<IpAddr>,
) -> Vec<SocketAddr> {
    let mut seen = HashSet::new();
    let mut addresses = nearby
        .addresses
        .iter()
        .filter_map(|value| value.parse::<IpAddr>().ok())
        .filter(|address| seen.insert(*address))
        .map(|address| SocketAddr::new(address, nearby.port))
        .collect::<Vec<_>>();
    let has_non_loopback = addresses.iter().any(|address| !address.ip().is_loopback());
    if has_non_loopback {
        addresses.retain(|address| !address.ip().is_loopback());
    }
    addresses.sort_by_key(|address| {
        let preferred_rank = u8::from(preferred != Some(address.ip()));
        let family_rank = match address.ip() {
            IpAddr::V4(_) => 0,
            IpAddr::V6(address) if !address.is_unicast_link_local() => 1,
            IpAddr::V6(_) => 2,
        };
        (preferred_rank, family_rank)
    });
    addresses
}

fn jittered_retry_delay(seconds: u64) -> Duration {
    let jitter = 750 + (uuid::Uuid::new_v4().as_u128() % 501) as u64;
    Duration::from_millis(seconds.saturating_mul(jitter))
}

fn elapsed_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn disconnect_code(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("timeout") || error.contains("超时") {
        "network_timeout"
    } else if normalized.contains("certificate") || error.contains("证书") {
        "identity_rejected"
    } else if normalized.contains("protocol") || error.contains("协议") {
        "protocol_error"
    } else if normalized.contains("closed") || error.contains("关闭") {
        "connection_closed"
    } else {
        "network_error"
    }
}

fn peer_certificate(connection: &Connection) -> Result<Vec<u8>, String> {
    let identity = connection
        .peer_identity()
        .ok_or_else(|| "TLS 对端未提供证书".to_string())?;
    let certificates = identity
        .downcast::<Vec<CertificateDer<'static>>>()
        .map_err(|_| "TLS 对端证书格式无效".to_string())?;
    certificates
        .first()
        .map(|certificate| certificate.as_ref().to_vec())
        .ok_or_else(|| "TLS 对端证书链为空".to_string())
}

fn validate_identity(device_id: &str, encoded_key: &str) -> Result<Vec<u8>, String> {
    let bytes = decode(encoded_key, "设备公钥")?;
    let key_bytes: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "设备公钥长度无效".to_string())?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| "设备公钥无效".to_string())?;
    if device_id_for_key(&key) != device_id {
        return Err("device_id 与身份公钥不匹配".into());
    }
    Ok(bytes)
}

fn verify_hello(
    trusted: &TrustedDevice,
    nonce: &str,
    public_key: &str,
    signature: &str,
) -> Result<(), String> {
    let public = validate_identity(&trusted.device_id, public_key)?;
    if public != trusted.public_key {
        return Err("可信设备公钥已变化，需要重新配对".into());
    }
    let key_bytes: [u8; 32] = public
        .as_slice()
        .try_into()
        .map_err(|_| "可信设备公钥长度无效".to_string())?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| "可信设备公钥无效".to_string())?;
    let signature_bytes = decode(signature, "Hello 签名")?;
    let signature =
        Signature::from_slice(&signature_bytes).map_err(|_| "Hello 签名长度无效".to_string())?;
    key.verify(&hello_payload(&trusted.device_id, nonce), &signature)
        .map_err(|_| "可信设备 Hello 签名验证失败".to_string())
}

fn hello_payload(device_id: &str, nonce: &str) -> Vec<u8> {
    format!("localdrop-trusted-hello-v1\0{device_id}\0{nonce}").into_bytes()
}

fn pairing_context(
    pairing_id: &str,
    initiator_nonce: &[u8],
    responder_nonce: &[u8],
    initiator_device_id: &str,
    responder_device_id: &str,
) -> Vec<u8> {
    let mut context = b"localdrop-pairing-context-v1\0".to_vec();
    for field in [
        pairing_id.as_bytes(),
        initiator_nonce,
        responder_nonce,
        initiator_device_id.as_bytes(),
        responder_device_id.as_bytes(),
    ] {
        context.extend_from_slice(&(field.len() as u32).to_be_bytes());
        context.extend_from_slice(field);
    }
    context
}

fn derive_sas(exporter: &[u8; 32], context: &[u8]) -> Result<String, String> {
    let salt = Sha256::digest(context);
    let hkdf = Hkdf::<Sha256>::new(Some(&salt), exporter);
    let mut key = [0u8; 32];
    hkdf.expand(b"localdrop-sas-v1", &mut key)
        .map_err(|_| "无法派生配对验证码".to_string())?;
    let limit = u32::MAX - (u32::MAX % 1_000_000);
    for counter in 0u32..1000 {
        let mut mac =
            HmacSha256::new_from_slice(&key).map_err(|_| "无法初始化配对验证码".to_string())?;
        mac.update(b"code");
        mac.update(&counter.to_be_bytes());
        let digest = mac.finalize().into_bytes();
        let value = u32::from_be_bytes(digest[..4].try_into().expect("four bytes"));
        if value < limit {
            return Ok(format!("{:06}", value % 1_000_000));
        }
    }
    Err("无法生成配对验证码".into())
}

fn random_bytes(length: usize) -> Vec<u8> {
    use rand_core::RngCore;
    let mut bytes = vec![0; length];
    rand_core::OsRng.fill_bytes(&mut bytes);
    bytes
}

fn decode(value: &str, label: &str) -> Result<Vec<u8>, String> {
    BASE64
        .decode(value.as_bytes())
        .map_err(|_| format!("{label}编码无效"))
}

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{files::FileEntry, identity::Identity};

    #[test]
    fn sas_is_stable_and_six_digits() {
        let exporter = [42u8; 32];
        let context = pairing_context("pair", &[1; 32], &[2; 32], "device-a", "device-b");
        let first = derive_sas(&exporter, &context).unwrap();
        let second = derive_sas(&exporter, &context).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 6);
        assert!(first.bytes().all(|byte| byte.is_ascii_digit()));
    }

    #[test]
    fn pairing_context_is_role_sensitive() {
        let left = pairing_context("pair", &[1; 32], &[2; 32], "device-a", "device-b");
        let right = pairing_context("pair", &[1; 32], &[2; 32], "device-b", "device-a");
        assert_ne!(left, right);
    }

    #[test]
    fn reconnect_replays_only_newer_clipboard_sequences() {
        assert!(should_replay_sequence(8, None));
        assert!(should_replay_sequence(8, Some(7)));
        assert!(!should_replay_sequence(8, Some(8)));
        assert!(!should_replay_sequence(8, Some(9)));
    }

    #[test]
    fn candidate_addresses_prefer_last_success_and_keep_fallbacks() {
        let nearby = service::NearbyDevice {
            instance_id: "instance".into(),
            device_id: "peer".into(),
            device_name: "Peer".into(),
            platform: "linux".into(),
            addresses: vec!["fe80::1".into(), "192.168.1.10".into(), "10.0.0.8".into()],
            port: 45_821,
            last_seen_at: "2026-07-15T00:00:00Z".into(),
            paired: true,
        };
        let preferred = "10.0.0.8".parse::<IpAddr>().unwrap();
        let addresses = candidate_addresses(&nearby, Some(preferred));
        assert_eq!(addresses[0].ip(), preferred);
        assert!(addresses.iter().any(|address| address.ip().is_ipv4()));
        assert!(addresses.iter().any(|address| address.ip().is_ipv6()));
    }

    #[tokio::test]
    async fn delivery_receipt_resolves_the_matching_message() {
        let pending = PendingDeliveryReceipts::default();
        let receiver = register_delivery_receipt(&pending, "message-1").unwrap();
        assert!(complete_delivery_receipt(
            &pending,
            "message-1",
            true,
            Some("已接收".into()),
            Some(7),
        ));
        let receipt = receiver.await.unwrap();
        assert!(receipt.accepted);
        assert_eq!(receipt.message.as_deref(), Some("已接收"));
        assert_eq!(receipt.processing_ms, Some(7));
        assert!(!complete_delivery_receipt(
            &pending,
            "message-1",
            true,
            None,
            None,
        ));
    }

    #[test]
    fn certificate_is_stable_for_persistent_identity() {
        let directory = std::env::temp_dir().join(format!(
            "airdrop-certificate-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let identity = Identity::load_or_create(&directory).unwrap();
        let issue = || {
            let key_pair = KeyPair::try_from(identity.pkcs8_der()).unwrap();
            CertificateParams::new(vec![identity.device_id().to_string()])
                .unwrap()
                .self_signed(&key_pair)
                .unwrap()
                .der()
                .to_vec()
        };
        assert_eq!(issue(), issue());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn quic_server_rejects_clients_without_device_certificate() {
        let directory = std::env::temp_dir().join(format!(
            "airdrop-mtls-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let identity = Identity::load_or_create(&directory).unwrap();
        let key_pair = KeyPair::try_from(identity.pkcs8_der()).unwrap();
        let certificate = CertificateParams::new(vec![identity.device_id().to_string()])
            .unwrap()
            .self_signed(&key_pair)
            .unwrap()
            .der()
            .to_vec();
        let server = Endpoint::server(
            server_config(
                certificate.clone(),
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der())),
            )
            .unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let verifier = Arc::new(PinnedCertificateVerifier::new(Some(certificate)));
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut crypto = rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        crypto.alpn_protocols = vec![TRUSTED_ALPN.to_vec()];
        let quic = QuicClientConfig::try_from(crypto).unwrap();
        let mut client = Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        client.set_default_client_config(quinn::ClientConfig::new(Arc::new(quic)));
        let server_address = server.local_addr().unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            client.connect(server_address, "localdrop").unwrap(),
        )
        .await;
        assert!(!matches!(result, Ok(Ok(_))));
        client.close(0u32.into(), b"done");
        server.close(0u32.into(), b"done");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn partial_file_bundle_reports_durable_resume_offsets() {
        let directory = std::env::temp_dir().join(format!(
            "airdrop-resume-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let temporary = directory.join(".part-transfer");
        let completed = directory.join("bundle-transfer");
        let header = FileBlobHeader {
            schema_version: 1,
            message_id: uuid::Uuid::new_v4().simple().to_string(),
            origin_sequence: 11,
            captured_at: "2026-07-13T00:00:00Z".into(),
            total_size: 10,
            entries: vec![FileEntry {
                relative_path: "folder/payload.txt".into(),
                size: 10,
                sha256: HEXLOWER.encode(&Sha256::digest(b"0123456789")),
                is_directory: false,
            }],
            group_ids: vec!["00112233-4455-6677-8899-aabbccddeeff".into()],
        };
        let initial = prepare_partial_file_bundle(&temporary, &completed, &header)
            .await
            .unwrap();
        assert_eq!(initial.offsets, vec![0]);
        tokio::fs::write(initial.root.join("folder/payload.txt"), b"0123")
            .await
            .unwrap();
        let resumed = prepare_partial_file_bundle(&temporary, &completed, &header)
            .await
            .unwrap();
        assert_eq!(resumed.offsets, vec![4]);
        let mut conflicting = header;
        conflicting.total_size = 9;
        assert!(
            prepare_partial_file_bundle(&temporary, &completed, &conflicting)
                .await
                .is_err()
        );
        let _ = tokio::fs::remove_dir_all(directory).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pinned_quic_connection_exchanges_framed_messages() {
        let directory = std::env::temp_dir().join(format!(
            "airdrop-quic-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let identity = Identity::load_or_create(&directory).unwrap();
        let key_pair = KeyPair::try_from(identity.pkcs8_der()).unwrap();
        let certificate = CertificateParams::new(vec![identity.device_id().to_string()])
            .unwrap()
            .self_signed(&key_pair)
            .unwrap()
            .der()
            .to_vec();
        let private_key_der = key_pair.serialize_der();
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(private_key_der.clone()));
        let server = Endpoint::server(
            server_config(certificate.clone(), private_key).unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let server_address = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            assert!(peer_certificate(&connection).is_ok());
            let (mut send, mut receive) = connection.accept_bi().await.unwrap();
            let message: TrustedMessage = read_frame(&mut receive).await.unwrap();
            assert!(matches!(
                message,
                TrustedMessage::ClipboardSlotOffer {
                    origin_sequence: 7,
                    ref text,
                    ..
                } if text == "hello over quic"
            ));
            write_frame(&mut send, &message).await.unwrap();
            send.finish().unwrap();
            let mut image_stream = connection.accept_uni().await.unwrap();
            assert_eq!(image_stream.read_u8().await.unwrap(), IMAGE_BLOB_KIND);
            let header: ImageBlobHeader = read_frame(&mut image_stream).await.unwrap();
            let png = image_stream.read_to_end(MAX_IMAGE_BLOB).await.unwrap();
            assert_eq!(header.width, 2);
            assert_eq!(header.height, 1);
            assert_eq!(header.png_length, png.len() as u64);
            let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
                .unwrap()
                .to_rgba8();
            assert_eq!(decoded.into_raw(), vec![255, 0, 0, 255, 0, 0, 255, 255]);
            let (mut file_response, mut file_stream) = connection.accept_bi().await.unwrap();
            assert_eq!(file_stream.read_u8().await.unwrap(), FILE_BLOB_KIND);
            let file_header: FileBlobHeader = read_frame(&mut file_stream).await.unwrap();
            write_frame(
                &mut file_response,
                &FileResumePlan {
                    schema_version: 1,
                    transfer_id: file_header.message_id.clone(),
                    offsets: vec![5],
                },
            )
            .await
            .unwrap();
            let file_bytes = file_stream
                .read_to_end(MAX_FILE_BUNDLE_BYTES as usize)
                .await
                .unwrap();
            assert_eq!(file_header.entries.len(), 1);
            assert_eq!(file_header.total_size, 14);
            assert_eq!(file_bytes, b"over quic");
            write_frame(
                &mut file_response,
                &FileTransferAck {
                    schema_version: 1,
                    transfer_id: file_header.message_id,
                    accepted: true,
                    message: None,
                    processing_ms: Some(4),
                },
            )
            .await
            .unwrap();
            file_response.finish().unwrap();
            let mut ack = connection.open_uni().await.unwrap();
            ack.write_all(b"ok").await.unwrap();
            ack.finish().unwrap();
            connection.closed().await;
        });

        let mut client = Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        client.set_default_client_config(
            client_config(
                Some(certificate.clone()),
                TRUSTED_ALPN,
                certificate,
                private_key_der,
            )
            .unwrap(),
        );
        let connection = client
            .connect(server_address, "localdrop")
            .unwrap()
            .await
            .unwrap();
        let connection_telemetry = TelemetryStore::default();
        connection_telemetry.mark_connected("peer", &connection);
        connection_telemetry.mark_disconnected(
            "peer",
            &connection,
            "network_error",
            "测试断联原因",
            false,
        );
        connection_telemetry.mark_connected("peer", &connection);
        let connection_snapshot = connection_telemetry.snapshot();
        assert_eq!(
            connection_snapshot.peers[0]
                .last_disconnect_reason
                .as_deref(),
            Some("测试断联原因"),
        );
        let (mut send, mut receive) = connection.open_bi().await.unwrap();
        let offer = TrustedMessage::ClipboardSlotOffer {
            schema_version: 1,
            message_id: uuid::Uuid::new_v4().simple().to_string(),
            origin_sequence: 7,
            captured_at: "2026-07-13T00:00:00Z".into(),
            text: "hello over quic".into(),
            group_ids: vec!["00112233-4455-6677-8899-aabbccddeeff".into()],
        };
        write_frame(&mut send, &offer).await.unwrap();
        let echoed: TrustedMessage = read_frame(&mut receive).await.unwrap();
        assert!(matches!(
            echoed,
            TrustedMessage::ClipboardSlotOffer {
                origin_sequence: 7,
                text,
                ..
            } if text == "hello over quic"
        ));
        let rgba = vec![255, 0, 0, 255, 0, 0, 255, 255];
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&rgba, 2, 1, ExtendedColorType::Rgba8)
            .unwrap();
        send_image_blob(
            connection.clone(),
            LocalImageOffer {
                sequence: 8,
                captured_at: "2026-07-13T00:00:01Z".into(),
                width: 2,
                height: 1,
                png: Arc::new(png),
            },
            vec!["00112233-4455-6677-8899-aabbccddeeff".into()],
            "image-test".into(),
            &TelemetryStore::default(),
            "upload:peer:image-test",
        )
        .await
        .unwrap();
        let source = directory.join("payload.txt");
        std::fs::write(&source, b"file over quic").unwrap();
        let bundle = crate::core::files::stage_file_bundle(
            &[source.to_string_lossy().into_owned()],
            &directory.join("staged"),
            9,
        )
        .unwrap();
        let telemetry = TelemetryStore::default();
        let transfer_key =
            telemetry.start_transfer("file-test", "peer", "upload", "files", bundle.total_size);
        send_file_blob(
            connection.clone(),
            LocalFileOffer {
                transfer_id: uuid::Uuid::new_v4().simple().to_string(),
                sequence: 9,
                captured_at: "2026-07-13T00:00:02Z".into(),
                bundle: Arc::new(bundle),
            },
            vec!["00112233-4455-6677-8899-aabbccddeeff".into()],
            &telemetry,
            &transfer_key,
        )
        .await
        .unwrap();
        let mut ack = connection.accept_uni().await.unwrap();
        assert_eq!(ack.read_to_end(2).await.unwrap(), b"ok");
        connection_telemetry.mark_disconnected(
            "peer",
            &connection,
            "connection_closed",
            "测试连接结束",
            true,
        );
        let final_connection_snapshot = connection_telemetry.snapshot();
        assert!(final_connection_snapshot.peers[0].total_uploaded_bytes > 0);
        assert!(final_connection_snapshot.peers[0].total_downloaded_bytes > 0);
        client.close(0u32.into(), b"done");
        server_task.await.unwrap();
        let _ = std::fs::remove_dir_all(directory);
    }
}
