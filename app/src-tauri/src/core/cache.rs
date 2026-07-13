use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use data_encoding::{BASE64, HEXLOWER};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Write, path::PathBuf};

const CACHE_SERVICE: &str = "io.github.wudao1006.airdrop";
const CACHE_ACCOUNT: &str = "clipboard-cache-v1";
const CACHE_MAGIC: &[u8; 8] = b"LDCLIP01";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CachedText {
    pub(crate) device_id: String,
    pub(crate) sequence: u64,
    pub(crate) text: String,
    pub(crate) captured_at: String,
}

pub(crate) struct ClipboardCache {
    root: PathBuf,
    cipher: Option<XChaCha20Poly1305>,
}

impl ClipboardCache {
    pub(crate) fn open(data_dir: &std::path::Path) -> Self {
        let root = data_dir.join("cache").join("clipboard");
        let cipher = match load_or_create_key() {
            Ok(key) => {
                if let Err(error) = fs::create_dir_all(&root) {
                    tracing::warn!(error = %error, "encrypted clipboard cache directory unavailable");
                    None
                } else {
                    Some(XChaCha20Poly1305::new((&key).into()))
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "credential storage unavailable; clipboard cache remains memory-only");
                None
            }
        };
        Self { root, cipher }
    }

    pub(crate) fn available(&self) -> bool {
        self.cipher.is_some()
    }

    pub(crate) fn store(&self, value: &CachedText) -> Result<Option<String>, String> {
        let Some(cipher) = &self.cipher else {
            return Ok(None);
        };
        let plaintext =
            serde_json::to_vec(value).map_err(|error| format!("无法编码剪贴板缓存：{error}"))?;
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let aad = cache_aad(&value.device_id);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| "无法加密剪贴板缓存".to_string())?;
        let object_name = format!(
            "{}-{}.ldcache",
            HEXLOWER.encode(&Sha256::digest(value.device_id.as_bytes())[..12]),
            value.sequence
        );
        let path = self.root.join(&object_name);
        let temporary = path.with_extension("tmp");
        let mut options = fs::OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| format!("无法创建剪贴板缓存：{error}"))?;
        file.write_all(CACHE_MAGIC)
            .and_then(|_| file.write_all(&nonce))
            .and_then(|_| file.write_all(&ciphertext))
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("无法持久化剪贴板缓存：{error}"))?;
        fs::rename(&temporary, &path).map_err(|error| format!("无法提交剪贴板缓存：{error}"))?;
        Ok(Some(object_name))
    }

    pub(crate) fn load(&self, device_id: &str, object_name: &str) -> Result<CachedText, String> {
        let cipher = self
            .cipher
            .as_ref()
            .ok_or_else(|| "系统凭据存储当前不可用".to_string())?;
        if object_name.contains('/') || object_name.contains('\\') {
            return Err("剪贴板缓存对象名称无效".into());
        }
        let bytes = fs::read(self.root.join(object_name))
            .map_err(|error| format!("无法读取剪贴板缓存：{error}"))?;
        if bytes.len() < CACHE_MAGIC.len() + 24 || &bytes[..CACHE_MAGIC.len()] != CACHE_MAGIC {
            return Err("剪贴板缓存格式无效".into());
        }
        let nonce_start = CACHE_MAGIC.len();
        let ciphertext_start = nonce_start + 24;
        let aad = cache_aad(device_id);
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&bytes[nonce_start..ciphertext_start]),
                Payload {
                    msg: &bytes[ciphertext_start..],
                    aad: &aad,
                },
            )
            .map_err(|_| "剪贴板缓存认证失败".to_string())?;
        let value: CachedText = serde_json::from_slice(&plaintext)
            .map_err(|error| format!("剪贴板缓存内容无效：{error}"))?;
        if value.device_id != device_id {
            return Err("剪贴板缓存设备身份不匹配".into());
        }
        Ok(value)
    }

    pub(crate) fn remove(&self, object_name: &str) {
        if !object_name.contains('/') && !object_name.contains('\\') {
            let _ = fs::remove_file(self.root.join(object_name));
        }
    }

    pub(crate) fn prune_except(&self, retained: &HashSet<String>) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_file() && !retained.contains(&name) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn load_or_create_key() -> Result<[u8; 32], String> {
    let entry = keyring::Entry::new(CACHE_SERVICE, CACHE_ACCOUNT)
        .map_err(|error| format!("无法访问系统凭据存储：{error}"))?;
    match entry.get_password() {
        Ok(encoded) => {
            let bytes = BASE64
                .decode(encoded.as_bytes())
                .map_err(|_| "系统凭据中的缓存密钥已损坏".to_string())?;
            bytes
                .try_into()
                .map_err(|_| "系统凭据中的缓存密钥长度无效".to_string())
        }
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            entry
                .set_password(&BASE64.encode(&key))
                .map_err(|error| format!("无法保存剪贴板缓存密钥：{error}"))?;
            Ok(key)
        }
        Err(error) => Err(format!("无法读取剪贴板缓存密钥：{error}")),
    }
}

fn cache_aad(device_id: &str) -> Vec<u8> {
    format!("localdrop-clipboard-cache-v1\0{device_id}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_cache_rejects_wrong_device() {
        let root = std::env::temp_dir().join(format!("airdrop-cache-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let cache = ClipboardCache {
            root: root.clone(),
            cipher: Some(XChaCha20Poly1305::new((&[9u8; 32]).into())),
        };
        let object = cache
            .store(&CachedText {
                device_id: "device-a".into(),
                sequence: 4,
                text: "secret text".into(),
                captured_at: "2026-07-13T00:00:00Z".into(),
            })
            .unwrap()
            .unwrap();
        assert_eq!(cache.load("device-a", &object).unwrap().text, "secret text");
        assert!(cache.load("device-b", &object).is_err());
        let raw = fs::read(root.join(object)).unwrap();
        assert!(!raw
            .windows(b"secret text".len())
            .any(|part| part == b"secret text"));
        let _ = fs::remove_dir_all(root);
    }
}
