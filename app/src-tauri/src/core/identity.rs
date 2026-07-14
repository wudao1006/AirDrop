use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

const IDENTITY_FILE: &str = "identity.ed25519";
const ED25519_SPKI_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

pub(crate) struct Identity {
    signing_key: SigningKey,
    device_id: String,
    device_name: String,
}

impl Identity {
    pub(crate) fn load_or_create(data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_dir).map_err(|error| format!("无法创建身份目录：{error}"))?;
        let path = data_dir.join(IDENTITY_FILE);
        let signing_key = if path.exists() {
            let bytes = fs::read(&path).map_err(|error| format!("无法读取设备身份：{error}"))?;
            let secret: [u8; 32] = bytes
                .try_into()
                .map_err(|_| "设备身份文件长度无效".to_string())?;
            SigningKey::from_bytes(&secret)
        } else {
            let key = SigningKey::generate(&mut OsRng);
            write_secret_atomically(&path, &key.to_bytes())?;
            key
        };
        let device_id = device_id_for_key(&signing_key.verifying_key());
        let device_name = crate::platform::device_name();
        Ok(Self {
            signing_key,
            device_id,
            device_name,
        })
    }

    pub(crate) fn device_id(&self) -> &str {
        &self.device_id
    }

    pub(crate) fn device_name(&self) -> &str {
        &self.device_name
    }

    pub(crate) fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub(crate) fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    pub(crate) fn pkcs8_der(&self) -> Vec<u8> {
        const PREFIX: &[u8] = &[
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        let mut der = Vec::with_capacity(PREFIX.len() + 32);
        der.extend_from_slice(PREFIX);
        der.extend_from_slice(&self.signing_key.to_bytes());
        der
    }
}

pub(crate) fn device_id_for_key(key: &VerifyingKey) -> String {
    let mut spki = Vec::with_capacity(ED25519_SPKI_PREFIX.len() + key.as_bytes().len());
    spki.extend_from_slice(ED25519_SPKI_PREFIX);
    spki.extend_from_slice(key.as_bytes());
    let digest = Sha256::digest(&spki);
    format!("ld1_{}", BASE32_NOPAD.encode(&digest).to_ascii_lowercase())
}

fn write_secret_atomically(path: &Path, secret: &[u8; 32]) -> Result<(), String> {
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
        .map_err(|error| format!("无法创建设备身份：{error}"))?;
    file.write_all(secret)
        .map_err(|error| format!("无法写入设备身份：{error}"))?;
    file.sync_all()
        .map_err(|error| format!("无法持久化设备身份：{error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("无法提交设备身份：{error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn identity_is_stable_across_restarts() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("airdrop-identity-{nonce}"));
        let first = Identity::load_or_create(&directory).unwrap();
        let second = Identity::load_or_create(&directory).unwrap();
        assert_eq!(first.device_id(), second.device_id());
        assert_eq!(first.public_key_bytes(), second.public_key_bytes());
        assert!(first.device_id().starts_with("ld1_"));
        assert_eq!(first.device_id().len(), 56);
        let _ = fs::remove_dir_all(directory);
    }
}
