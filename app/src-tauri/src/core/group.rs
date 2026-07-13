use super::identity::{device_id_for_key, Identity};
use data_encoding::BASE64;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

pub(crate) const GROUP_ENCODING_VERSION: u16 = 1;
pub(crate) const MAX_GROUP_MEMBERS: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemberState {
    Invited,
    Active,
    Removed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SyncDirection {
    Disabled,
    SendOnly,
    ReceiveOnly,
    Bidirectional,
}

impl SyncDirection {
    pub(crate) fn can_publish(&self) -> bool {
        matches!(self, Self::SendOnly | Self::Bidirectional)
    }

    pub(crate) fn can_subscribe(&self) -> bool {
        matches!(self, Self::ReceiveOnly | Self::Bidirectional)
    }

    fn code(&self) -> u8 {
        match self {
            Self::Disabled => 0,
            Self::SendOnly => 1,
            Self::ReceiveOnly => 2,
            Self::Bidirectional => 3,
        }
    }
}

impl MemberState {
    fn code(&self) -> u8 {
        match self {
            Self::Invited => 0,
            Self::Active => 1,
            Self::Removed => 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroupPolicy {
    pub(crate) allow_text: bool,
    pub(crate) allow_images: bool,
    pub(crate) allow_html: bool,
    pub(crate) allow_files: bool,
    pub(crate) offline_ttl_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroupMember {
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) platform: String,
    pub(crate) public_key: String,
    pub(crate) certificate: String,
    pub(crate) joined_at: String,
    pub(crate) state: MemberState,
    pub(crate) direction: SyncDirection,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroupManifest {
    pub(crate) encoding_version: u16,
    pub(crate) group_id: String,
    pub(crate) owner_device_id: String,
    pub(crate) name: String,
    pub(crate) revision: u64,
    pub(crate) membership_epoch: u64,
    pub(crate) policy: GroupPolicy,
    pub(crate) members: Vec<GroupMember>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SignedGroupManifest {
    pub(crate) manifest: GroupManifest,
    pub(crate) signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroupTombstone {
    pub(crate) encoding_version: u16,
    pub(crate) group_id: String,
    pub(crate) owner_device_id: String,
    pub(crate) revision: u64,
    pub(crate) membership_epoch: u64,
    pub(crate) deleted_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SignedGroupTombstone {
    pub(crate) tombstone: GroupTombstone,
    pub(crate) signature: String,
}

impl SignedGroupTombstone {
    pub(crate) fn sign(tombstone: GroupTombstone, identity: &Identity) -> Result<Self, String> {
        if tombstone.encoding_version != GROUP_ENCODING_VERSION
            || tombstone.owner_device_id != identity.device_id()
        {
            return Err("同步组删除声明版本或 Owner 无效".into());
        }
        let signature = identity.sign(&tombstone.canonical_bytes()?).to_bytes();
        Ok(Self {
            tombstone,
            signature: BASE64.encode(&signature),
        })
    }

    pub(crate) fn verify(&self, owner_public_key: &[u8]) -> Result<(), String> {
        let bytes: [u8; 32] = owner_public_key
            .try_into()
            .map_err(|_| "同步组 Owner 公钥长度无效".to_string())?;
        let key =
            VerifyingKey::from_bytes(&bytes).map_err(|_| "同步组 Owner 公钥无效".to_string())?;
        if device_id_for_key(&key) != self.tombstone.owner_device_id {
            return Err("同步组删除声明 Owner 身份不匹配".into());
        }
        let signature = BASE64
            .decode(self.signature.as_bytes())
            .map_err(|_| "同步组删除签名编码无效".to_string())?;
        let signature =
            Signature::from_slice(&signature).map_err(|_| "同步组删除签名长度无效".to_string())?;
        key.verify(&self.tombstone.canonical_bytes()?, &signature)
            .map_err(|_| "同步组删除签名验证失败".to_string())
    }
}

impl GroupTombstone {
    fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        if self.encoding_version != GROUP_ENCODING_VERSION {
            return Err("不支持的同步组删除声明版本".into());
        }
        let mut output = b"localdrop-group-tombstone-v1\0".to_vec();
        put_u16(&mut output, self.encoding_version);
        put_uuid(&mut output, &self.group_id)?;
        put_string(&mut output, &self.owner_device_id)?;
        put_u64(&mut output, self.revision);
        put_u64(&mut output, self.membership_epoch);
        put_string(&mut output, &self.deleted_at)?;
        output.push(1);
        Ok(output)
    }
}

impl SignedGroupManifest {
    pub(crate) fn sign(mut manifest: GroupManifest, identity: &Identity) -> Result<Self, String> {
        normalize_and_validate(&mut manifest)?;
        if manifest.owner_device_id != identity.device_id() {
            return Err("只有同步组 Owner 可以签署清单".into());
        }
        let signature = identity.sign(&manifest.canonical_bytes()?).to_bytes();
        Ok(Self {
            manifest,
            signature: BASE64.encode(&signature),
        })
    }

    pub(crate) fn verify(&self, owner_public_key: &[u8]) -> Result<(), String> {
        let mut normalized = self.manifest.clone();
        normalize_and_validate(&mut normalized)?;
        if normalized.canonical_bytes()? != self.manifest.canonical_bytes()? {
            return Err("同步组清单成员顺序不规范".into());
        }
        let bytes: [u8; 32] = owner_public_key
            .try_into()
            .map_err(|_| "同步组 Owner 公钥长度无效".to_string())?;
        let key =
            VerifyingKey::from_bytes(&bytes).map_err(|_| "同步组 Owner 公钥无效".to_string())?;
        if device_id_for_key(&key) != self.manifest.owner_device_id {
            return Err("同步组 Owner 身份与公钥不匹配".into());
        }
        let signature = BASE64
            .decode(self.signature.as_bytes())
            .map_err(|_| "同步组清单签名编码无效".to_string())?;
        let signature =
            Signature::from_slice(&signature).map_err(|_| "同步组清单签名长度无效".to_string())?;
        key.verify(&self.manifest.canonical_bytes()?, &signature)
            .map_err(|_| "同步组清单签名验证失败".to_string())
    }
}

impl GroupManifest {
    pub(crate) fn active_member(&self, device_id: &str) -> Option<&GroupMember> {
        self.members
            .iter()
            .find(|member| member.device_id == device_id && member.state == MemberState::Active)
    }

    pub(crate) fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        if self.encoding_version != GROUP_ENCODING_VERSION {
            return Err("不支持的同步组清单版本".into());
        }
        let mut output = b"localdrop-group-manifest-v1\0".to_vec();
        put_u16(&mut output, self.encoding_version);
        put_uuid(&mut output, &self.group_id)?;
        put_string(&mut output, &self.owner_device_id)?;
        put_u64(&mut output, self.revision);
        put_u64(&mut output, self.membership_epoch);
        put_string(&mut output, &self.name)?;
        output.push(self.policy.allow_text as u8);
        output.push(self.policy.allow_images as u8);
        output.push(self.policy.allow_html as u8);
        output.push(self.policy.allow_files as u8);
        put_u64(&mut output, self.policy.offline_ttl_seconds);
        put_u32(&mut output, self.members.len())?;
        for member in &self.members {
            put_string(&mut output, &member.device_id)?;
            put_string(&mut output, &member.device_name)?;
            put_string(&mut output, &member.platform)?;
            put_bytes(&mut output, &decode(&member.public_key, "成员公钥")?)?;
            put_bytes(&mut output, &decode(&member.certificate, "成员证书")?)?;
            put_string(&mut output, &member.joined_at)?;
            output.push(member.state.code());
            output.push(member.direction.code());
        }
        output.push(1); // Ed25519
        Ok(output)
    }
}

fn normalize_and_validate(manifest: &mut GroupManifest) -> Result<(), String> {
    if manifest.encoding_version != GROUP_ENCODING_VERSION {
        return Err("不支持的同步组清单版本".into());
    }
    if manifest.name.trim().is_empty() || manifest.name.chars().count() > 64 {
        return Err("同步组名称必须为 1 到 64 个字符".into());
    }
    if manifest.members.is_empty() || manifest.members.len() > MAX_GROUP_MEMBERS {
        return Err("同步组成员数量无效".into());
    }
    manifest
        .members
        .sort_by(|left, right| left.device_id.cmp(&right.device_id));
    if manifest
        .members
        .windows(2)
        .any(|members| members[0].device_id == members[1].device_id)
    {
        return Err("同步组包含重复设备".into());
    }
    let owner = manifest
        .members
        .iter()
        .find(|member| member.device_id == manifest.owner_device_id)
        .ok_or_else(|| "同步组清单缺少 Owner".to_string())?;
    if owner.state != MemberState::Active {
        return Err("同步组 Owner 必须保持 active".into());
    }
    for member in &manifest.members {
        let public_key = decode(&member.public_key, "成员公钥")?;
        let bytes: [u8; 32] = public_key
            .as_slice()
            .try_into()
            .map_err(|_| "成员公钥长度无效".to_string())?;
        let key = VerifyingKey::from_bytes(&bytes).map_err(|_| "成员公钥无效".to_string())?;
        if device_id_for_key(&key) != member.device_id {
            return Err("同步组成员身份与公钥不匹配".into());
        }
        if decode(&member.certificate, "成员证书")?.is_empty() {
            return Err("同步组成员证书为空".into());
        }
    }
    Ok(())
}

fn put_uuid(output: &mut Vec<u8>, value: &str) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| "同步组 ID 无效".to_string())?;
    output.extend_from_slice(uuid.as_bytes());
    Ok(())
}

fn put_string(output: &mut Vec<u8>, value: &str) -> Result<(), String> {
    put_bytes(output, value.as_bytes())
}

fn put_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<(), String> {
    put_u32(output, value.len())?;
    output.extend_from_slice(value);
    Ok(())
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_u32(output: &mut Vec<u8>, value: usize) -> Result<(), String> {
    let length = u32::try_from(value).map_err(|_| "同步组字段过长".to_string())?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn decode(value: &str, label: &str) -> Result<Vec<u8>, String> {
    BASE64
        .decode(value.as_bytes())
        .map_err(|_| format!("{label}编码无效"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_manifest_detects_mutation_and_has_stable_encoding() {
        let directory = std::env::temp_dir().join(format!(
            "airdrop-group-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let identity = Identity::load_or_create(&directory).unwrap();
        let member = GroupMember {
            device_id: identity.device_id().into(),
            device_name: "Owner".into(),
            platform: "linux".into(),
            public_key: BASE64.encode(&identity.public_key_bytes()),
            certificate: BASE64.encode(b"test-certificate"),
            joined_at: "2026-07-13T00:00:00Z".into(),
            state: MemberState::Active,
            direction: SyncDirection::Bidirectional,
        };
        let manifest = GroupManifest {
            encoding_version: GROUP_ENCODING_VERSION,
            group_id: "00112233-4455-6677-8899-aabbccddeeff".into(),
            owner_device_id: identity.device_id().into(),
            name: "Personal".into(),
            revision: 1,
            membership_epoch: 1,
            policy: GroupPolicy {
                allow_text: true,
                allow_images: true,
                allow_html: false,
                allow_files: false,
                offline_ttl_seconds: 86_400,
            },
            members: vec![member],
        };
        let signed = SignedGroupManifest::sign(manifest, &identity).unwrap();
        signed.verify(&identity.public_key_bytes()).unwrap();
        let first = signed.manifest.canonical_bytes().unwrap();
        let second = signed.manifest.canonical_bytes().unwrap();
        assert_eq!(first, second);
        let mut mutated = signed.clone();
        mutated.manifest.name = "Changed".into();
        assert!(mutated.verify(&identity.public_key_bytes()).is_err());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn signed_tombstone_detects_mutation() {
        let directory = std::env::temp_dir().join(format!(
            "airdrop-group-tombstone-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let identity = Identity::load_or_create(&directory).unwrap();
        let tombstone = GroupTombstone {
            encoding_version: GROUP_ENCODING_VERSION,
            group_id: "00112233-4455-6677-8899-aabbccddeeff".into(),
            owner_device_id: identity.device_id().into(),
            revision: 3,
            membership_epoch: 2,
            deleted_at: "2026-07-13T00:00:00Z".into(),
        };
        let signed = SignedGroupTombstone::sign(tombstone, &identity).unwrap();
        signed.verify(&identity.public_key_bytes()).unwrap();
        let mut mutated = signed.clone();
        mutated.tombstone.revision += 1;
        assert!(mutated.verify(&identity.public_key_bytes()).is_err());
        let _ = std::fs::remove_dir_all(directory);
    }
}
