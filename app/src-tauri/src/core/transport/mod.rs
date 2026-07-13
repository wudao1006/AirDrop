mod protocol;

use super::{
    discovery::TRANSPORT_PORT,
    identity::device_id_for_key,
    service::{self, PendingPairing, ServiceState},
    storage::TrustedDevice,
};
use crate::platform;
use data_encoding::{BASE64, HEXLOWER};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use protocol::{read_frame, write_frame, PairMessage, TrustedMessage, PAIR_ALPN, TRUSTED_ALPN};
use quinn::{crypto::rustls::QuicClientConfig, Connection, Endpoint};
use rcgen::{CertificateParams, KeyPair};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::mpsc;

type HmacSha256 = Hmac<Sha256>;

struct PairCommandRegistration {
    commands: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<bool>>>>,
    pairing_id: String,
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
    pairing_allowed_until: Arc<Mutex<u64>>,
    pair_commands: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<bool>>>>,
    peers: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<TrustedMessage>>>>,
    connecting: Arc<Mutex<HashSet<String>>>,
    latest_offer: Arc<Mutex<Option<TrustedMessage>>>,
}

impl TransportHandle {
    pub(crate) fn allow_pairing(&self, seconds: u64) {
        if let Ok(mut expiry) = self.pairing_allowed_until.lock() {
            *expiry = unix_seconds().saturating_add(seconds.min(120));
        }
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

    pub(crate) fn broadcast_text(&self, sequence: u64, text: String, captured_at: String) {
        if text.len() > 1024 * 1024 {
            return;
        }
        let message = TrustedMessage::ClipboardSlotOffer {
            schema_version: 1,
            message_id: uuid::Uuid::new_v4().simple().to_string(),
            origin_sequence: sequence,
            captured_at,
            text,
        };
        if let Ok(mut latest) = self.latest_offer.lock() {
            *latest = Some(message.clone());
        }
        if let Ok(peers) = self.peers.lock() {
            for sender in peers.values() {
                let _ = sender.send(message.clone());
            }
        }
    }

    pub(crate) fn connect_pairing(&self, app: AppHandle, nearby: service::NearbyDevice) {
        let handle = self.clone();
        self.runtime.spawn(async move {
            if let Err(error) = handle.connect_pairing_inner(app, nearby).await {
                tracing::warn!(error = %error, "pairing connection failed");
            }
        });
    }

    pub(crate) fn connect_trusted(&self, app: AppHandle, nearby: service::NearbyDevice) {
        let device_id = nearby.device_id.clone();
        let local_should_dial = {
            let state = app.state::<ServiceState>();
            state.device_id() < device_id.as_str()
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
        if !connecting.insert(device_id.clone()) {
            return;
        }
        drop(connecting);
        let handle = self.clone();
        self.runtime.spawn(async move {
            let mut retry_delay = 0u64;
            loop {
                if retry_delay > 0 {
                    tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                }
                let current = {
                    let state = app.state::<ServiceState>();
                    state.nearby_device(&device_id)
                };
                let Some(current) = current else {
                    break;
                };
                if let Err(error) = handle.connect_trusted_inner(app.clone(), current).await {
                    tracing::debug!(device_id = %device_id, error = %error, retry_delay, "trusted connection unavailable");
                }
                retry_delay = if retry_delay == 0 {
                    1
                } else {
                    (retry_delay * 2).min(30)
                };
            }
            if let Ok(mut connecting) = handle.connecting.lock() {
                connecting.remove(&device_id);
            }
        });
    }

    async fn connect_pairing_inner(
        &self,
        app: AppHandle,
        nearby: service::NearbyDevice,
    ) -> Result<(), String> {
        let address = preferred_address(&nearby)?;
        let config = client_config(None, PAIR_ALPN)?;
        let connection = self
            .endpoint
            .connect_with(config, address, "localdrop")
            .map_err(|error| format!("无法创建配对连接：{error}"))?
            .await
            .map_err(|error| format!("无法连接附近设备：{error}"))?;
        let peer_certificate = peer_certificate(&connection)?;
        let (mut send, mut receive) = connection
            .open_bi()
            .await
            .map_err(|error| format!("无法打开配对控制流：{error}"))?;
        let state = app.state::<ServiceState>();
        let pairing_id = uuid::Uuid::new_v4().simple().to_string();
        let initiator_nonce = random_bytes(32);
        write_frame(
            &mut send,
            &PairMessage::Init {
                schema_version: 1,
                pairing_id: pairing_id.clone(),
                nonce: BASE64.encode(&initiator_nonce),
                device_id: state.device_id().to_string(),
                device_name: state.device_name().to_string(),
                platform: platform::platform_name().to_string(),
                public_key: BASE64.encode(&state.identity().public_key_bytes()),
                certificate: BASE64.encode(&self.certificate_der),
            },
        )
        .await?;
        let hello: PairMessage = read_frame(&mut receive).await?;
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
            platform,
            public_key,
            certificate_der,
            paired_at: now(),
        };
        let context = pairing_context(
            &pairing_id,
            &initiator_nonce,
            &responder_nonce,
            state.device_id(),
            &device.device_id,
        );
        drop(state);
        self.run_pair_confirmation(
            app, connection, send, receive, device, pairing_id, context, "outgoing",
        )
        .await
    }

    async fn connect_trusted_inner(
        &self,
        app: AppHandle,
        nearby: service::NearbyDevice,
    ) -> Result<(), String> {
        let state = app.state::<ServiceState>();
        let trusted = state
            .trusted_device(&nearby.device_id)?
            .ok_or_else(|| "设备尚未配对".to_string())?;
        drop(state);
        let config = client_config(Some(trusted.certificate_der.clone()), TRUSTED_ALPN)?;
        let connection = self
            .endpoint
            .connect_with(config, preferred_address(&nearby)?, "localdrop")
            .map_err(|error| format!("无法创建可信连接：{error}"))?
            .await
            .map_err(|error| format!("可信连接失败：{error}"))?;
        let (send, receive) = connection
            .open_bi()
            .await
            .map_err(|error| format!("无法打开可信控制流：{error}"))?;
        self.run_trusted(app, connection, send, receive, Some(trusted))
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
    ) -> Result<(), String> {
        let mut exporter = [0u8; 32];
        connection
            .export_keying_material(&mut exporter, b"EXPORTER-localdrop-pairing-v1", &context)
            .map_err(|error| format!("无法导出配对会话密钥：{error:?}"))?;
        let sas = derive_sas(&exporter, &context)?;
        let context_hash = HEXLOWER.encode(&Sha256::digest(&context));
        let expires_at = (OffsetDateTime::now_utc() + time::Duration::seconds(120))
            .format(&Rfc3339)
            .unwrap_or_else(|_| now());
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
        drop(state);
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        self.pair_commands
            .lock()
            .map_err(|_| "配对命令锁已损坏".to_string())?
            .insert(pairing_id.clone(), command_tx);
        let _registration = PairCommandRegistration {
            commands: self.pair_commands.clone(),
            pairing_id: pairing_id.clone(),
        };
        let mut local_confirmed = false;
        let mut remote_confirmed = false;
        let mut complete_sent = false;
        loop {
            tokio::select! {
                command = command_rx.recv() => {
                    let accepted = command.ok_or_else(|| "配对会话已取消".to_string())?;
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
                    match message? {
                        PairMessage::Confirm { schema_version: 1, pairing_id: remote_id, context_hash: remote_hash, accepted }
                            if remote_id == pairing_id && remote_hash == context_hash => {
                                if !accepted { return Err("对方拒绝了配对".into()); }
                                remote_confirmed = true;
                            }
                        PairMessage::Complete { schema_version: 1, pairing_id: remote_id } if remote_id == pairing_id => {
                            let paired_at = now();
                            let state = app.state::<ServiceState>();
                            let promoted = state.promote_trusted_device(&pairing_id, &paired_at)?;
                            service::pairing_completed(&state, &app, &pairing_id, promoted)?;
                            let nearby = state.nearby_device(&device.device_id);
                            drop(state);
                            connection.close(0u32.into(), b"paired");
                            if let Some(nearby) = nearby {
                                self.connect_trusted(app.clone(), nearby);
                            }
                            return Ok(());
                        }
                        PairMessage::Abort { reason, .. } => return Err(reason),
                        _ => return Err("配对确认消息无效".into()),
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(120)) => return Err("配对确认已超时".into()),
            }
            if local_confirmed && remote_confirmed && !complete_sent {
                write_frame(
                    &mut send,
                    &PairMessage::Complete {
                        schema_version: 1,
                        pairing_id: pairing_id.clone(),
                    },
                )
                .await?;
                complete_sent = true;
            }
        }
    }

    async fn run_trusted(
        &self,
        app: AppHandle,
        connection: Connection,
        mut send: quinn::SendStream,
        mut receive: quinn::RecvStream,
        expected: Option<TrustedDevice>,
    ) -> Result<(), String> {
        let state = app.state::<ServiceState>();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let payload = hello_payload(state.device_id(), &nonce);
        let hello = TrustedMessage::Hello {
            schema_version: 1,
            device_id: state.device_id().to_string(),
            device_name: state.device_name().to_string(),
            platform: platform::platform_name().to_string(),
            nonce,
            public_key: BASE64.encode(&state.identity().public_key_bytes()),
            signature: BASE64.encode(&state.identity().sign(&payload).to_bytes()),
        };
        drop(state);
        write_frame(&mut send, &hello).await?;
        let remote: TrustedMessage = read_frame(&mut receive).await?;
        let TrustedMessage::Hello {
            schema_version: 1,
            device_id,
            device_name: _,
            platform: _,
            nonce,
            public_key,
            signature,
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
        let state = app.state::<ServiceState>();
        let trusted = state
            .trusted_device(&device_id)?
            .ok_or_else(|| "对端身份不在可信设备中".to_string())?;
        verify_hello(&trusted, &nonce, &public_key, &signature)?;
        service::set_trusted_online(&state, &app, &device_id, true)?;
        drop(state);
        let (sender, mut outbound) = mpsc::unbounded_channel::<TrustedMessage>();
        self.peers
            .lock()
            .map_err(|_| "可信连接表锁已损坏".to_string())?
            .insert(device_id.clone(), sender);
        if let Some(latest) = self
            .latest_offer
            .lock()
            .ok()
            .and_then(|latest| latest.clone())
        {
            if let Ok(peers) = self.peers.lock() {
                if let Some(sender) = peers.get(&device_id) {
                    let _ = sender.send(latest);
                }
            }
        }
        let writer = tokio::spawn(async move {
            while let Some(message) = outbound.recv().await {
                if write_frame(&mut send, &message).await.is_err() {
                    break;
                }
            }
        });
        loop {
            match read_frame::<TrustedMessage>(&mut receive).await {
                Ok(TrustedMessage::ClipboardSlotOffer {
                    schema_version: 1,
                    origin_sequence,
                    captured_at,
                    text,
                    ..
                }) => {
                    let state = app.state::<ServiceState>();
                    if let Err(error) = service::receive_remote_text(
                        &state,
                        &app,
                        &trusted,
                        origin_sequence,
                        text,
                        captured_at,
                    ) {
                        tracing::warn!(device_id = %device_id, error = %error, "remote clipboard rejected");
                    }
                }
                Ok(_) => return Err("可信连接收到意外消息".into()),
                Err(error) => {
                    writer.abort();
                    if let Ok(mut peers) = self.peers.lock() {
                        peers.remove(&device_id);
                    }
                    let state = app.state::<ServiceState>();
                    let _ = service::set_trusted_online(&state, &app, &device_id, false);
                    connection.close(0u32.into(), b"closed");
                    return Err(error);
                }
            }
        }
    }
}

pub(crate) fn start(app: AppHandle) -> Result<TransportHandle, String> {
    let state = app.state::<ServiceState>();
    let key_pair = KeyPair::try_from(state.identity().pkcs8_der())
        .map_err(|error| format!("无法载入 TLS 身份密钥：{error}"))?;
    let certificate = CertificateParams::new(vec![state.device_id().to_string()])
        .map_err(|error| format!("无法创建 TLS 证书参数：{error}"))?
        .self_signed(&key_pair)
        .map_err(|error| format!("无法签发 TLS 证书：{error}"))?;
    let certificate_der = certificate.der().to_vec();
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));
    drop(state);
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
                let result = create_endpoint(certificate_for_thread.clone(), private_key);
                let endpoint = match result {
                    Ok(endpoint) => endpoint,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                let handle = TransportHandle {
                    runtime: tokio::runtime::Handle::current(),
                    endpoint: endpoint.clone(),
                    certificate_der: certificate_for_thread,
                    pairing_allowed_until: Arc::new(Mutex::new(0)),
                    pair_commands: Arc::new(Mutex::new(HashMap::new())),
                    peers: Arc::new(Mutex::new(HashMap::new())),
                    connecting: Arc::new(Mutex::new(HashSet::new())),
                    latest_offer: Arc::new(Mutex::new(None)),
                };
                let _ = ready_tx.send(Ok(handle.clone()));
                while let Some(incoming) = endpoint.accept().await {
                    let handle = handle.clone();
                    let app = app_for_thread.clone();
                    tokio::spawn(async move {
                        if let Err(error) = accept_connection(handle, app, incoming).await {
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

async fn accept_connection(
    handle: TransportHandle,
    app: AppHandle,
    incoming: quinn::Incoming,
) -> Result<(), String> {
    let connection = incoming
        .await
        .map_err(|error| format!("QUIC 握手失败：{error}"))?;
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
        accept_pairing(handle, app, connection).await
    } else if alpn == Some(TRUSTED_ALPN) {
        let (send, receive) = connection
            .accept_bi()
            .await
            .map_err(|error| format!("无法接受可信控制流：{error}"))?;
        handle
            .run_trusted(app, connection, send, receive, None)
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
    let responder_nonce = random_bytes(32);
    let state = app.state::<ServiceState>();
    write_frame(
        &mut send,
        &PairMessage::Hello {
            schema_version: 1,
            pairing_id: pairing_id.clone(),
            initiator_nonce: nonce,
            responder_nonce: BASE64.encode(&responder_nonce),
            device_id: state.device_id().to_string(),
            device_name: state.device_name().to_string(),
            platform: platform::platform_name().to_string(),
            public_key: BASE64.encode(&state.identity().public_key_bytes()),
            certificate: BASE64.encode(&handle.certificate_der),
        },
    )
    .await?;
    let context = pairing_context(
        &pairing_id,
        &initiator_nonce,
        &responder_nonce,
        &device_id,
        state.device_id(),
    );
    drop(state);
    let device = TrustedDevice {
        device_id,
        device_name,
        platform,
        public_key,
        certificate_der,
        paired_at: now(),
    };
    handle
        .run_pair_confirmation(
            app, connection, send, receive, device, pairing_id, context, "incoming",
        )
        .await
}

fn create_endpoint(
    certificate_der: Vec<u8>,
    private_key: PrivateKeyDer<'static>,
) -> Result<Endpoint, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut crypto = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| format!("无法限定 TLS 1.3：{error}"))?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(certificate_der)], private_key)
        .map_err(|error| format!("无法配置 TLS 证书：{error}"))?;
    crypto.alpn_protocols = vec![TRUSTED_ALPN.to_vec(), PAIR_ALPN.to_vec()];
    crypto.max_early_data_size = 0;
    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(crypto)
        .map_err(|error| format!("无法配置 QUIC TLS：{error}"))?;
    let server = quinn::ServerConfig::with_crypto(Arc::new(quic));
    Endpoint::server(
        server,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), TRANSPORT_PORT),
    )
    .map_err(|error| format!("无法监听 QUIC 端口 {TRANSPORT_PORT}：{error}"))
}

fn client_config(
    expected_certificate: Option<Vec<u8>>,
    alpn: &[u8],
) -> Result<quinn::ClientConfig, String> {
    let verifier = Arc::new(PinnedCertificateVerifier::new(expected_certificate));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut crypto = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| format!("无法限定 TLS 1.3：{error}"))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    crypto.alpn_protocols = vec![alpn.to_vec()];
    crypto.enable_early_data = false;
    let quic = QuicClientConfig::try_from(crypto)
        .map_err(|error| format!("无法配置 QUIC 客户端：{error}"))?;
    Ok(quinn::ClientConfig::new(Arc::new(quic)))
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
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
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
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn preferred_address(nearby: &service::NearbyDevice) -> Result<SocketAddr, String> {
    nearby
        .addresses
        .iter()
        .filter_map(|value| value.parse::<IpAddr>().ok())
        .map(|address| SocketAddr::new(address, nearby.port))
        .find(|address| !address.ip().is_loopback())
        .or_else(|| {
            nearby
                .addresses
                .iter()
                .filter_map(|value| value.parse::<IpAddr>().ok())
                .map(|address| SocketAddr::new(address, nearby.port))
                .next()
        })
        .ok_or_else(|| "附近设备尚未解析出可连接地址".to_string())
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
    use crate::core::identity::Identity;

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
}
