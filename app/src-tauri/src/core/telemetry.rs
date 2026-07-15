use super::storage::Store;
use quinn::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub(crate) const TELEMETRY_EVENT: &str = "airdrop://telemetry";
const RECENT_TRANSFERS_PER_DEVICE: usize = 10;
const RECENT_TRANSFER_SNAPSHOT_LIMIT: usize = 200;
const RATE_SMOOTHING_TIME_CONSTANT_SECS: f64 = 3.0;
const LOSS_WINDOW_SAMPLES: usize = 10;
const TRANSFER_RATE_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);
const TRANSFER_RATE_STALE_AFTER: Duration = Duration::from_secs(2);
const HISTORY_WRITE_ATTEMPTS: usize = 3;
const HISTORY_WRITE_RETRY_BASE: Duration = Duration::from_millis(25);
const HISTORY_FLUSH_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TelemetrySnapshot {
    pub(crate) sampled_at: String,
    pub(crate) peers: Vec<PeerTelemetry>,
    pub(crate) transfers: Vec<TransferTelemetry>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PeerTelemetry {
    pub(crate) device_id: String,
    pub(crate) connected: bool,
    pub(crate) rtt_ms: Option<u64>,
    pub(crate) upload_bps: u64,
    pub(crate) download_bps: u64,
    pub(crate) recent_upload_bps: u64,
    pub(crate) recent_download_bps: u64,
    pub(crate) loss_percent: f64,
    pub(crate) total_uploaded_bytes: u64,
    pub(crate) total_downloaded_bytes: u64,
    pub(crate) connected_at: Option<String>,
    pub(crate) last_activity_at: Option<String>,
    pub(crate) reconnect_count: u32,
    pub(crate) last_disconnect_reason: Option<String>,
    pub(crate) last_disconnect_code: Option<String>,
    pub(crate) last_disconnected_at: Option<String>,
    pub(crate) last_disconnect_planned: bool,
    pub(crate) unexpected_disconnect_count: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferTelemetry {
    pub(crate) id: String,
    pub(crate) attempt_id: u64,
    pub(crate) device_id: String,
    pub(crate) direction: String,
    pub(crate) kind: String,
    pub(crate) total_bytes: u64,
    pub(crate) transferred_bytes: u64,
    pub(crate) started_at: String,
    pub(crate) completed_at: Option<String>,
    pub(crate) duration_ms: u64,
    #[serde(default)]
    pub(crate) network_duration_ms: Option<u64>,
    #[serde(default)]
    pub(crate) confirmation_duration_ms: Option<u64>,
    #[serde(default)]
    pub(crate) remote_processing_ms: Option<u64>,
    pub(crate) speed_bps: u64,
    pub(crate) average_bps: u64,
    pub(crate) status: String,
    pub(crate) message: Option<String>,
}

struct PeerTelemetryState {
    view: PeerTelemetry,
    connection_id: usize,
    last_tx_bytes: u64,
    last_rx_bytes: u64,
    last_sent_packets: u64,
    last_lost_packets: u64,
    last_sampled_at: Instant,
    smoothed_upload_bps: f64,
    smoothed_download_bps: f64,
    loss_window: VecDeque<(u64, u64)>,
    connected_before: bool,
}

struct ActiveTransfer {
    view: TransferTelemetry,
    started_at: Instant,
    last_progress_at: Instant,
    last_progress_bytes: u64,
    session_transferred_bytes: u64,
    smoothed_bps: f64,
    network_completed_at: Option<Instant>,
}

#[derive(Default)]
struct TelemetryInner {
    peers: HashMap<String, PeerTelemetryState>,
    active_transfers: HashMap<String, ActiveTransfer>,
    recent_transfers: HashMap<String, VecDeque<TransferTelemetry>>,
    next_attempt_id: u64,
}

enum HistoryCommand {
    Save(Box<TransferTelemetry>),
    Flush(mpsc::SyncSender<Result<(), String>>),
    Shutdown,
}

struct HistoryWorker {
    sender: mpsc::Sender<HistoryCommand>,
    join: Mutex<Option<thread::JoinHandle<()>>>,
}

impl HistoryWorker {
    fn save(&self, transfer: TransferTelemetry) -> Result<(), String> {
        self.sender
            .send(HistoryCommand::Save(Box::new(transfer)))
            .map_err(|_| "传输历史线程不可用".to_string())
    }

    fn flush(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.sender
            .send(HistoryCommand::Flush(reply_tx))
            .map_err(|_| "传输历史线程不可用".to_string())?;
        reply_rx
            .recv_timeout(HISTORY_FLUSH_TIMEOUT)
            .map_err(|_| "等待传输历史落盘超时".to_string())?
    }
}

impl Drop for HistoryWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(HistoryCommand::Shutdown);
        if let Ok(mut join) = self.join.lock() {
            if let Some(join) = join.take() {
                let _ = join.join();
            }
        }
    }
}

fn persist_transfer_history(store: &Store, transfer: &TransferTelemetry) -> Result<(), String> {
    let completed_at = transfer
        .completed_at
        .as_deref()
        .ok_or_else(|| "传输历史缺少完成时间".to_string())?;
    let payload =
        serde_json::to_string(transfer).map_err(|error| format!("传输历史编码失败：{error}"))?;
    let mut last_error = None;
    for attempt in 0..HISTORY_WRITE_ATTEMPTS {
        match store.save_transfer_history(
            transfer.attempt_id,
            &transfer.device_id,
            completed_at,
            &payload,
        ) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < HISTORY_WRITE_ATTEMPTS {
            thread::sleep(HISTORY_WRITE_RETRY_BASE.saturating_mul((attempt + 1) as u32));
        }
    }
    Err(last_error.unwrap_or_else(|| "传输历史写入失败".into()))
}

fn run_history_worker(store: Store, receiver: mpsc::Receiver<HistoryCommand>) {
    let mut last_error = None;
    while let Ok(command) = receiver.recv() {
        match command {
            HistoryCommand::Save(transfer) => {
                if let Err(error) = persist_transfer_history(&store, &transfer) {
                    tracing::warn!(error = %error, "transfer history persistence failed");
                    last_error = Some(error);
                }
            }
            HistoryCommand::Flush(reply) => {
                let result = last_error
                    .as_ref()
                    .map_or(Ok(()), |error| Err(error.clone()));
                let _ = reply.send(result);
            }
            HistoryCommand::Shutdown => break,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct TelemetryStore {
    inner: Arc<Mutex<TelemetryInner>>,
    history_worker: Option<Arc<HistoryWorker>>,
    change_notify: Arc<tokio::sync::Notify>,
}

impl TelemetryStore {
    pub(crate) fn with_store(store: Store) -> Result<Self, String> {
        let mut inner = TelemetryInner::default();
        for payload in store.load_transfer_history()? {
            let Ok(transfer) = serde_json::from_str::<TransferTelemetry>(&payload) else {
                tracing::warn!("ignoring malformed transfer history record");
                continue;
            };
            if transfer.status == "active" {
                continue;
            }
            inner.next_attempt_id = inner.next_attempt_id.max(transfer.attempt_id);
            let history = inner
                .recent_transfers
                .entry(transfer.device_id.clone())
                .or_default();
            history.push_back(transfer);
            history.truncate(RECENT_TRANSFERS_PER_DEVICE);
        }
        let (history_tx, history_rx) = mpsc::channel::<HistoryCommand>();
        let join = thread::Builder::new()
            .name("airdrop-telemetry-history".into())
            .spawn(move || run_history_worker(store, history_rx))
            .map_err(|error| format!("无法启动传输历史线程：{error}"))?;
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
            history_worker: Some(Arc::new(HistoryWorker {
                sender: history_tx,
                join: Mutex::new(Some(join)),
            })),
            change_notify: Arc::new(tokio::sync::Notify::new()),
        })
    }

    pub(crate) fn notifier(&self) -> Arc<tokio::sync::Notify> {
        self.change_notify.clone()
    }

    pub(crate) fn flush_history(&self) -> Result<(), String> {
        self.history_worker
            .as_ref()
            .map_or(Ok(()), |worker| worker.flush())
    }

    pub(crate) fn mark_connected(&self, device_id: &str, connection: &Connection) {
        let stats = connection.stats();
        let now = timestamp();
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let previous = inner.peers.remove(device_id);
        let reconnect_count = previous.as_ref().map_or(0, |peer| {
            peer.view
                .reconnect_count
                .saturating_add(u32::from(peer.connected_before))
        });
        let total_uploaded_bytes = previous
            .as_ref()
            .map_or(0, |peer| peer.view.total_uploaded_bytes);
        let total_downloaded_bytes = previous
            .as_ref()
            .map_or(0, |peer| peer.view.total_downloaded_bytes);
        let last_disconnect_reason = previous
            .as_ref()
            .and_then(|peer| peer.view.last_disconnect_reason.clone());
        let last_disconnect_code = previous
            .as_ref()
            .and_then(|peer| peer.view.last_disconnect_code.clone());
        let last_disconnected_at = previous
            .as_ref()
            .and_then(|peer| peer.view.last_disconnected_at.clone());
        let last_disconnect_planned = previous
            .as_ref()
            .is_some_and(|peer| peer.view.last_disconnect_planned);
        let unexpected_disconnect_count = previous
            .as_ref()
            .map_or(0, |peer| peer.view.unexpected_disconnect_count);
        inner.peers.insert(
            device_id.to_string(),
            PeerTelemetryState {
                view: PeerTelemetry {
                    device_id: device_id.to_string(),
                    connected: true,
                    rtt_ms: Some(duration_millis(connection.rtt())),
                    upload_bps: 0,
                    download_bps: 0,
                    recent_upload_bps: 0,
                    recent_download_bps: 0,
                    loss_percent: 0.0,
                    total_uploaded_bytes,
                    total_downloaded_bytes,
                    connected_at: Some(now.clone()),
                    last_activity_at: Some(now),
                    reconnect_count,
                    last_disconnect_reason,
                    last_disconnect_code,
                    last_disconnected_at,
                    last_disconnect_planned,
                    unexpected_disconnect_count,
                },
                connection_id: connection.stable_id(),
                last_tx_bytes: stats.udp_tx.bytes,
                last_rx_bytes: stats.udp_rx.bytes,
                last_sent_packets: stats.path.sent_packets,
                last_lost_packets: stats.path.lost_packets,
                last_sampled_at: Instant::now(),
                smoothed_upload_bps: 0.0,
                smoothed_download_bps: 0.0,
                loss_window: VecDeque::new(),
                connected_before: true,
            },
        );
    }

    pub(crate) fn mark_disconnected(
        &self,
        device_id: &str,
        connection: &Connection,
        code: &str,
        reason: impl Into<String>,
        planned: bool,
    ) {
        self.sample_connection(device_id, connection);
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let Some(peer) = inner.peers.get_mut(device_id) else {
            return;
        };
        if peer.connection_id != connection.stable_id() || !peer.view.connected {
            return;
        }
        peer.view.connected = false;
        peer.view.rtt_ms = None;
        peer.view.upload_bps = 0;
        peer.view.download_bps = 0;
        peer.view.recent_upload_bps = 0;
        peer.view.recent_download_bps = 0;
        peer.view.connected_at = None;
        peer.view.last_disconnect_code = Some(code.into());
        peer.view.last_disconnect_reason = Some(reason.into());
        peer.view.last_disconnected_at = Some(timestamp());
        peer.view.last_disconnect_planned = planned;
        if !planned {
            peer.view.unexpected_disconnect_count =
                peer.view.unexpected_disconnect_count.saturating_add(1);
        }
    }

    pub(crate) fn sample_connection(&self, device_id: &str, connection: &Connection) {
        let stats = connection.stats();
        let now = Instant::now();
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let Some(peer) = inner.peers.get_mut(device_id) else {
            return;
        };
        if peer.connection_id != connection.stable_id() || !peer.view.connected {
            return;
        }
        let elapsed = now
            .saturating_duration_since(peer.last_sampled_at)
            .as_secs_f64()
            .max(0.001);
        let tx_delta = stats.udp_tx.bytes.saturating_sub(peer.last_tx_bytes);
        let rx_delta = stats.udp_rx.bytes.saturating_sub(peer.last_rx_bytes);
        let sent_delta = stats
            .path
            .sent_packets
            .saturating_sub(peer.last_sent_packets);
        let lost_delta = stats
            .path
            .lost_packets
            .saturating_sub(peer.last_lost_packets);
        let upload_bps = tx_delta as f64 / elapsed;
        let download_bps = rx_delta as f64 / elapsed;
        peer.loss_window.push_back((sent_delta, lost_delta));
        while peer.loss_window.len() > LOSS_WINDOW_SAMPLES {
            peer.loss_window.pop_front();
        }
        let (window_sent, window_lost) = peer
            .loss_window
            .iter()
            .fold((0_u64, 0_u64), |(sent, lost), sample| {
                (sent.saturating_add(sample.0), lost.saturating_add(sample.1))
            });
        let loss_percent = if window_sent == 0 {
            0.0
        } else {
            window_lost as f64 * 100.0 / window_sent as f64
        }
        .clamp(0.0, 100.0);
        peer.smoothed_upload_bps = smooth(peer.smoothed_upload_bps, upload_bps, elapsed);
        peer.smoothed_download_bps = smooth(peer.smoothed_download_bps, download_bps, elapsed);
        peer.view.connected = true;
        peer.view.rtt_ms = Some(duration_millis(stats.path.rtt));
        peer.view.upload_bps = upload_bps.round() as u64;
        peer.view.download_bps = download_bps.round() as u64;
        peer.view.recent_upload_bps = peer.smoothed_upload_bps.round() as u64;
        peer.view.recent_download_bps = peer.smoothed_download_bps.round() as u64;
        peer.view.loss_percent = (loss_percent * 100.0).round() / 100.0;
        peer.view.total_uploaded_bytes = peer.view.total_uploaded_bytes.saturating_add(tx_delta);
        peer.view.total_downloaded_bytes =
            peer.view.total_downloaded_bytes.saturating_add(rx_delta);
        if tx_delta > 0 || rx_delta > 0 {
            peer.view.last_activity_at = Some(timestamp());
        }
        peer.last_tx_bytes = stats.udp_tx.bytes;
        peer.last_rx_bytes = stats.udp_rx.bytes;
        peer.last_sent_packets = stats.path.sent_packets;
        peer.last_lost_packets = stats.path.lost_packets;
        peer.last_sampled_at = now;
    }

    pub(crate) fn start_transfer(
        &self,
        id: impl Into<String>,
        device_id: impl Into<String>,
        direction: &str,
        kind: &str,
        total_bytes: u64,
    ) -> String {
        let id = id.into();
        let device_id = device_id.into();
        let Ok(mut inner) = self.inner.lock() else {
            return transfer_key(&id, &device_id, direction, 0);
        };
        inner.next_attempt_id = inner.next_attempt_id.saturating_add(1);
        let attempt_id = inner.next_attempt_id;
        let key = transfer_key(&id, &device_id, direction, attempt_id);
        inner.active_transfers.insert(
            key.clone(),
            ActiveTransfer {
                view: TransferTelemetry {
                    id,
                    attempt_id,
                    device_id,
                    direction: direction.into(),
                    kind: kind.into(),
                    total_bytes,
                    transferred_bytes: 0,
                    started_at: timestamp(),
                    completed_at: None,
                    duration_ms: 0,
                    network_duration_ms: None,
                    confirmation_duration_ms: None,
                    remote_processing_ms: None,
                    speed_bps: 0,
                    average_bps: 0,
                    status: "active".into(),
                    message: None,
                },
                started_at: Instant::now(),
                last_progress_at: Instant::now(),
                last_progress_bytes: 0,
                session_transferred_bytes: 0,
                smoothed_bps: 0.0,
                network_completed_at: None,
            },
        );
        drop(inner);
        self.change_notify.notify_one();
        key
    }

    pub(crate) fn set_transfer_baseline(&self, key: &str, transferred_bytes: u64) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if let Some(transfer) = inner.active_transfers.get_mut(key) {
            let baseline = transferred_bytes.min(transfer.view.total_bytes);
            transfer.view.transferred_bytes = baseline;
            transfer.view.speed_bps = 0;
            transfer.last_progress_at = Instant::now();
            transfer.last_progress_bytes = baseline;
            transfer.smoothed_bps = 0.0;
        }
    }

    pub(crate) fn update_transfer(&self, key: &str, transferred_bytes: u64) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if let Some(transfer) = inner.active_transfers.get_mut(key) {
            let previous_bytes = transfer.view.transferred_bytes;
            let next_bytes = transfer
                .view
                .transferred_bytes
                .max(transferred_bytes.min(transfer.view.total_bytes));
            transfer.view.transferred_bytes = next_bytes;
            transfer.session_transferred_bytes = transfer
                .session_transferred_bytes
                .saturating_add(next_bytes.saturating_sub(previous_bytes));
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(transfer.last_progress_at);
            if next_bytes > transfer.last_progress_bytes
                && (elapsed >= TRANSFER_RATE_SAMPLE_INTERVAL
                    || next_bytes == transfer.view.total_bytes)
            {
                let bytes_delta = next_bytes.saturating_sub(transfer.last_progress_bytes);
                let rate = bytes_delta as f64 / elapsed.as_secs_f64().max(0.001);
                transfer.smoothed_bps = smooth(transfer.smoothed_bps, rate, elapsed.as_secs_f64());
                transfer.view.speed_bps = transfer.smoothed_bps.round() as u64;
                transfer.last_progress_at = now;
                transfer.last_progress_bytes = next_bytes;
            }
        }
    }

    pub(crate) fn mark_network_complete(&self, key: &str) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if let Some(transfer) = inner.active_transfers.get_mut(key) {
            transfer
                .network_completed_at
                .get_or_insert_with(Instant::now);
        }
    }

    pub(crate) fn set_remote_processing(&self, key: &str, processing_ms: Option<u64>) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if let Some(transfer) = inner.active_transfers.get_mut(key) {
            transfer.view.remote_processing_ms = processing_ms;
        }
    }

    pub(crate) fn finish_transfer(&self, key: &str, success: bool, message: Option<String>) {
        self.finish_transfer_with_status(
            key,
            if success { "success" } else { "failed" },
            success,
            message,
        );
    }

    pub(crate) fn finish_unconfirmed(&self, key: &str, message: Option<String>) {
        self.finish_transfer_with_status(key, "sent", true, message);
    }

    fn finish_transfer_with_status(
        &self,
        key: &str,
        status: &str,
        completed: bool,
        message: Option<String>,
    ) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let Some(mut transfer) = inner.active_transfers.remove(key) else {
            return;
        };
        let elapsed = transfer.started_at.elapsed();
        if completed {
            transfer.view.transferred_bytes = transfer.view.total_bytes;
        }
        transfer.view.completed_at = Some(timestamp());
        transfer.view.duration_ms = duration_millis(elapsed);
        transfer.view.network_duration_ms = transfer.network_completed_at.map(|completed| {
            duration_millis(completed.saturating_duration_since(transfer.started_at))
        });
        transfer.view.confirmation_duration_ms = transfer
            .view
            .network_duration_ms
            .map(|network| transfer.view.duration_ms.saturating_sub(network));
        transfer.view.speed_bps = 0;
        transfer.view.average_bps = if elapsed.is_zero() {
            transfer.session_transferred_bytes
        } else {
            (transfer.session_transferred_bytes as f64 / elapsed.as_secs_f64()).round() as u64
        };
        transfer.view.status = status.into();
        transfer.view.message = message;
        let completed = transfer.view;
        let history = inner
            .recent_transfers
            .entry(completed.device_id.clone())
            .or_default();
        history.push_front(completed.clone());
        history.truncate(RECENT_TRANSFERS_PER_DEVICE);
        drop(inner);
        if let Some(worker) = self.history_worker.as_ref() {
            if let Err(error) = worker.save(completed) {
                tracing::warn!(error = %error, "transfer history thread is unavailable");
            }
        }
        self.change_notify.notify_one();
    }

    pub(crate) fn snapshot(&self) -> TelemetrySnapshot {
        let Ok(inner) = self.inner.lock() else {
            return TelemetrySnapshot::default();
        };
        let mut peers = inner
            .peers
            .values()
            .map(|peer| peer.view.clone())
            .collect::<Vec<_>>();
        peers.sort_by(|left, right| left.device_id.cmp(&right.device_id));
        let mut active = inner
            .active_transfers
            .values()
            .map(|transfer| {
                let mut view = transfer.view.clone();
                let elapsed = transfer.started_at.elapsed();
                if transfer.last_progress_at.elapsed() >= TRANSFER_RATE_STALE_AFTER {
                    view.speed_bps = 0;
                }
                view.duration_ms = duration_millis(elapsed);
                view.network_duration_ms = transfer.network_completed_at.map(|completed| {
                    duration_millis(completed.saturating_duration_since(transfer.started_at))
                });
                view.confirmation_duration_ms = view
                    .network_duration_ms
                    .map(|network| view.duration_ms.saturating_sub(network));
                view.average_bps = if elapsed.is_zero() {
                    transfer.session_transferred_bytes
                } else {
                    (transfer.session_transferred_bytes as f64 / elapsed.as_secs_f64()).round()
                        as u64
                };
                view
            })
            .collect::<Vec<_>>();
        active.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        let mut recent = inner
            .recent_transfers
            .values()
            .flat_map(|history| history.iter().cloned())
            .collect::<Vec<_>>();
        recent.sort_by(|left, right| right.completed_at.cmp(&left.completed_at));
        recent.truncate(RECENT_TRANSFER_SNAPSHOT_LIMIT);
        active.extend(recent);
        TelemetrySnapshot {
            sampled_at: timestamp(),
            peers,
            transfers: active,
        }
    }

    pub(crate) fn emit(&self, app: &AppHandle) {
        let _ = app.emit(TELEMETRY_EVENT, self.snapshot());
    }
}

fn transfer_key(id: &str, device_id: &str, direction: &str, attempt_id: u64) -> String {
    format!("{direction}:{device_id}:{id}:{attempt_id}")
}

fn smooth(previous: f64, current: f64, elapsed_seconds: f64) -> f64 {
    if previous == 0.0 {
        current
    } else {
        let alpha = 1.0 - (-elapsed_seconds.max(0.001) / RATE_SMOOTHING_TIME_CONSTANT_SECS).exp();
        previous * (1.0 - alpha) + current * alpha
    }
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_history_keeps_completed_metrics() {
        let telemetry = TelemetryStore::default();
        let key = telemetry.start_transfer("transfer-1", "peer-1", "upload", "files", 100);
        telemetry.update_transfer(&key, 60);
        telemetry.finish_transfer(&key, true, Some("已发送".into()));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.transfers.len(), 1);
        assert_eq!(snapshot.transfers[0].transferred_bytes, 100);
        assert_eq!(snapshot.transfers[0].status, "success");
    }

    #[test]
    fn failed_transfer_preserves_partial_progress() {
        let telemetry = TelemetryStore::default();
        let key = telemetry.start_transfer("transfer-2", "peer-2", "download", "image", 200);
        telemetry.update_transfer(&key, 75);
        telemetry.finish_transfer(&key, false, Some("连接中断".into()));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.transfers[0].transferred_bytes, 75);
        assert_eq!(snapshot.transfers[0].status, "failed");
    }

    #[test]
    fn repeated_logical_transfer_ids_get_independent_attempts() {
        let telemetry = TelemetryStore::default();
        let first = telemetry.start_transfer("transfer-3", "peer-3", "upload", "files", 100);
        let second = telemetry.start_transfer("transfer-3", "peer-3", "upload", "files", 100);
        assert_ne!(first, second);
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.transfers.len(), 2);
        assert_ne!(
            snapshot.transfers[0].attempt_id,
            snapshot.transfers[1].attempt_id
        );
    }

    #[test]
    fn resume_baseline_is_not_counted_as_current_attempt_throughput() {
        let telemetry = TelemetryStore::default();
        let key = telemetry.start_transfer("transfer-4", "peer-4", "download", "files", 1_000);
        telemetry.set_transfer_baseline(&key, 1_000);
        telemetry.finish_transfer(&key, true, Some("文件已存在".into()));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.transfers[0].transferred_bytes, 1_000);
        assert_eq!(snapshot.transfers[0].average_bps, 0);
    }

    #[test]
    fn unconfirmed_delivery_is_not_reported_as_success() {
        let telemetry = TelemetryStore::default();
        let key = telemetry.start_transfer("transfer-5", "peer-5", "upload", "text", 20);
        telemetry.update_transfer(&key, 20);
        telemetry.finish_unconfirmed(&key, Some("旧版本不支持回执".into()));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.transfers[0].status, "sent");
        assert_eq!(snapshot.transfers[0].transferred_bytes, 20);
    }

    #[test]
    fn completed_transfer_history_survives_store_reload() {
        let directory = std::env::temp_dir().join(format!(
            "airdrop-telemetry-history-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let store = Store::open(&directory).unwrap();
        let telemetry = TelemetryStore::with_store(store.clone()).unwrap();
        let key = telemetry.start_transfer("transfer-6", "peer-6", "upload", "text", 12);
        telemetry.update_transfer(&key, 12);
        telemetry.finish_transfer(&key, true, Some("已确认".into()));
        telemetry.flush_history().unwrap();
        assert_eq!(store.load_transfer_history().unwrap().len(), 1);
        let key = telemetry.start_transfer("transfer-7", "peer-6", "download", "image", 24);
        telemetry.update_transfer(&key, 24);
        telemetry.finish_transfer(&key, true, Some("已接收".into()));
        drop(telemetry);
        assert_eq!(store.load_transfer_history().unwrap().len(), 2);
        let restored = TelemetryStore::with_store(store).unwrap().snapshot();
        assert_eq!(restored.transfers.len(), 2);
        assert!(restored
            .transfers
            .iter()
            .any(|transfer| transfer.id == "transfer-6"));
        assert!(restored
            .transfers
            .iter()
            .any(|transfer| transfer.id == "transfer-7"));
        let _ = std::fs::remove_dir_all(directory);
    }
}
