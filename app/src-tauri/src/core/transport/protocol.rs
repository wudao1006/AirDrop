use crate::core::files::FileEntry;
use crate::core::group::{SignedGroupManifest, SignedGroupTombstone};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) const PAIR_ALPN: &[u8] = b"localdrop-pair/1";
pub(crate) const TRUSTED_ALPN: &[u8] = b"localdrop/1";
const MAX_FRAME: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipboardCapabilities {
    pub(crate) text: bool,
    pub(crate) rich_text: bool,
    pub(crate) images: bool,
    pub(crate) files: bool,
    #[serde(default)]
    pub(crate) delivery_receipts: bool,
    #[serde(default)]
    pub(crate) state_reconciliation: bool,
}

impl ClipboardCapabilities {
    pub(crate) fn local() -> Self {
        if cfg!(target_os = "android") {
            Self {
                text: true,
                rich_text: false,
                images: false,
                files: false,
                delivery_receipts: true,
                state_reconciliation: true,
            }
        } else {
            Self {
                delivery_receipts: true,
                state_reconciliation: true,
                ..Self::default()
            }
        }
    }
}

impl Default for ClipboardCapabilities {
    fn default() -> Self {
        Self {
            text: true,
            rich_text: true,
            images: true,
            files: true,
            delivery_receipts: false,
            state_reconciliation: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum PairMessage {
    Init {
        schema_version: u8,
        pairing_id: String,
        nonce: String,
        device_id: String,
        device_name: String,
        platform: String,
        public_key: String,
        certificate: String,
    },
    Hello {
        schema_version: u8,
        pairing_id: String,
        initiator_nonce: String,
        responder_nonce: String,
        device_id: String,
        device_name: String,
        platform: String,
        public_key: String,
        certificate: String,
    },
    Confirm {
        schema_version: u8,
        pairing_id: String,
        context_hash: String,
        accepted: bool,
    },
    Complete {
        schema_version: u8,
        pairing_id: String,
    },
    Abort {
        schema_version: u8,
        pairing_id: String,
        reason: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum TrustedMessage {
    Hello {
        schema_version: u8,
        device_id: String,
        device_name: String,
        platform: String,
        nonce: String,
        public_key: String,
        signature: String,
        #[serde(default)]
        capabilities: ClipboardCapabilities,
    },
    ClipboardSlotOffer {
        schema_version: u8,
        message_id: String,
        origin_sequence: u64,
        captured_at: String,
        text: String,
        group_ids: Vec<String>,
    },
    RichClipboardSlotOffer {
        schema_version: u8,
        message_id: String,
        origin_sequence: u64,
        captured_at: String,
        text: String,
        html: Option<String>,
        rtf: Option<String>,
        group_ids: Vec<String>,
    },
    ClipboardDeliveryAck {
        schema_version: u8,
        message_id: String,
        accepted: bool,
        message: Option<String>,
        #[serde(default)]
        processing_ms: Option<u64>,
    },
    ClipboardStateSummary {
        schema_version: u8,
        last_received_sequence: Option<u64>,
    },
    GroupInvite {
        schema_version: u8,
        message_id: String,
        invite_id: String,
        target_device_id: String,
        expires_at: String,
        manifest: SignedGroupManifest,
    },
    GroupAccept {
        schema_version: u8,
        message_id: String,
        invite_id: String,
        group_id: String,
        accepted: bool,
    },
    GroupManifestUpdate {
        schema_version: u8,
        message_id: String,
        manifest: SignedGroupManifest,
    },
    GroupLeaveNotice {
        schema_version: u8,
        message_id: String,
        group_id: String,
        leave_id: String,
    },
    GroupTombstone {
        schema_version: u8,
        message_id: String,
        tombstone: SignedGroupTombstone,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageBlobHeader {
    pub(crate) schema_version: u8,
    pub(crate) message_id: String,
    pub(crate) origin_sequence: u64,
    pub(crate) captured_at: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) png_length: u64,
    pub(crate) sha256: String,
    pub(crate) group_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileBlobHeader {
    pub(crate) schema_version: u8,
    pub(crate) message_id: String,
    pub(crate) origin_sequence: u64,
    pub(crate) captured_at: String,
    pub(crate) total_size: u64,
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) group_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileResumePlan {
    pub(crate) schema_version: u8,
    pub(crate) transfer_id: String,
    pub(crate) offsets: Vec<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileTransferAck {
    pub(crate) schema_version: u8,
    pub(crate) transfer_id: String,
    pub(crate) accepted: bool,
    pub(crate) message: Option<String>,
    #[serde(default)]
    pub(crate) processing_ms: Option<u64>,
}

pub(crate) async fn write_frame<T: Serialize>(
    send: &mut quinn::SendStream,
    value: &T,
) -> Result<(), String> {
    let payload = serde_json::to_vec(value).map_err(|error| format!("协议编码失败：{error}"))?;
    if payload.len() > MAX_FRAME {
        return Err("协议帧超过 1 MiB".into());
    }
    send.write_u32(payload.len() as u32)
        .await
        .map_err(|error| format!("协议帧长度发送失败：{error}"))?;
    send.write_all(&payload)
        .await
        .map_err(|error| format!("协议帧发送失败：{error}"))?;
    Ok(())
}

pub(crate) async fn read_frame<T: DeserializeOwned>(
    receive: &mut quinn::RecvStream,
) -> Result<T, String> {
    let length = receive
        .read_u32()
        .await
        .map_err(|error| format!("协议帧长度读取失败：{error}"))? as usize;
    if length == 0 || length > MAX_FRAME {
        return Err("协议帧长度无效".into());
    }
    let mut payload = vec![0; length];
    receive
        .read_exact(&mut payload)
        .await
        .map_err(|error| format!("协议帧读取失败：{error}"))?;
    serde_json::from_slice(&payload).map_err(|error| format!("协议帧格式无效：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_hello_without_capabilities_keeps_desktop_compatibility() {
        let message: TrustedMessage = serde_json::from_value(serde_json::json!({
            "type": "hello",
            "schema_version": 1,
            "device_id": "ld1_peer",
            "device_name": "Peer",
            "platform": "windows",
            "nonce": "nonce",
            "public_key": "key",
            "signature": "signature"
        }))
        .unwrap();
        let TrustedMessage::Hello { capabilities, .. } = message else {
            panic!("expected trusted hello");
        };
        assert!(capabilities.text);
        assert!(capabilities.rich_text);
        assert!(capabilities.images);
        assert!(capabilities.files);
        assert!(!capabilities.delivery_receipts);
        assert!(!capabilities.state_reconciliation);
        assert!(ClipboardCapabilities::local().delivery_receipts);
        assert!(ClipboardCapabilities::local().state_reconciliation);
    }

    #[test]
    fn older_capability_payload_does_not_enable_delivery_receipts() {
        let capabilities: ClipboardCapabilities = serde_json::from_value(serde_json::json!({
            "text": true,
            "richText": true,
            "images": true,
            "files": true
        }))
        .unwrap();
        assert!(!capabilities.delivery_receipts);
        assert!(!capabilities.state_reconciliation);
    }

    #[test]
    fn older_delivery_ack_without_processing_time_is_accepted() {
        let message: TrustedMessage = serde_json::from_value(serde_json::json!({
            "type": "clipboard_delivery_ack",
            "schema_version": 1,
            "message_id": "message-1",
            "accepted": true,
            "message": null
        }))
        .unwrap();
        let TrustedMessage::ClipboardDeliveryAck { processing_ms, .. } = message else {
            panic!("expected clipboard delivery acknowledgement");
        };
        assert_eq!(processing_ms, None);
    }
}
