use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) const PAIR_ALPN: &[u8] = b"localdrop-pair/1";
pub(crate) const TRUSTED_ALPN: &[u8] = b"localdrop/1";
const MAX_FRAME: usize = 1024 * 1024;

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
    },
    ClipboardSlotOffer {
        schema_version: u8,
        message_id: String,
        origin_sequence: u64,
        captured_at: String,
        text: String,
    },
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
