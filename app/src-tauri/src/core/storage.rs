use super::{
    group::{SignedGroupManifest, SignedGroupTombstone},
    service::AppSettings,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{fs, path::Path, sync::Mutex};

const SCHEMA_VERSION: i64 = 8;

#[derive(Clone, Debug)]
pub(crate) struct TrustedDevice {
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) platform: String,
    pub(crate) public_key: Vec<u8>,
    pub(crate) certificate_der: Vec<u8>,
    pub(crate) paired_at: String,
    pub(crate) sync_enabled: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CachedSlotMetadata {
    pub(crate) device_id: String,
    pub(crate) sequence: u64,
    pub(crate) object_name: String,
    pub(crate) expires_at_unix: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct StoredGroupInvite {
    pub(crate) invite_id: String,
    pub(crate) target_device_id: String,
    pub(crate) expires_at: String,
    pub(crate) status: String,
    pub(crate) manifest: SignedGroupManifest,
}

#[derive(Clone, Debug)]
pub(crate) struct StoredGroupLeave {
    pub(crate) group_id: String,
    pub(crate) member_device_id: String,
    pub(crate) owner_device_id: String,
    pub(crate) leave_id: String,
    pub(crate) status: String,
}

pub(crate) struct StoredRuntime {
    pub(crate) publish_paused: bool,
    pub(crate) subscribe_paused: bool,
}

pub(crate) struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    pub(crate) fn open(data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_dir).map_err(|error| format!("无法创建应用数据目录：{error}"))?;
        let connection = Connection::open(data_dir.join("airdrop.sqlite3"))
            .map_err(|error| format!("无法打开本地数据库：{error}"))?;
        let previous_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| format!("无法读取数据库版本：{error}"))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA synchronous = NORMAL;
                 CREATE TABLE IF NOT EXISTS app_settings (
                   singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                   json TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS runtime_state (
                   singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                   publish_paused INTEGER NOT NULL,
                   subscribe_paused INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS trusted_devices (
                   device_id TEXT PRIMARY KEY,
                   device_name TEXT NOT NULL,
                   platform TEXT NOT NULL,
                   public_key BLOB NOT NULL,
                   certificate_der BLOB NOT NULL,
                   paired_at TEXT NOT NULL,
                   revoked INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE IF NOT EXISTS pending_pairings (
                   pairing_id TEXT PRIMARY KEY,
                   device_id TEXT NOT NULL,
                   device_name TEXT NOT NULL,
                   platform TEXT NOT NULL,
                   public_key BLOB NOT NULL,
                   certificate_der BLOB NOT NULL,
                   expires_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS sequence_state (
                   singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                   next_origin_sequence INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS local_revocations (
                   device_id TEXT PRIMARY KEY,
                   revoked_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS clipboard_slots (
                   device_id TEXT PRIMARY KEY,
                   sequence INTEGER NOT NULL,
                   object_name TEXT NOT NULL,
                   expires_at_unix INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS sync_groups (
                   group_id TEXT PRIMARY KEY,
                   owner_device_id TEXT NOT NULL,
                   revision INTEGER NOT NULL,
                   membership_epoch INTEGER NOT NULL,
                   manifest_json TEXT NOT NULL,
                   local_state TEXT NOT NULL,
                   updated_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS group_invites (
                   invite_id TEXT PRIMARY KEY,
                   group_id TEXT NOT NULL,
                   target_device_id TEXT NOT NULL,
                   expires_at TEXT NOT NULL,
                   status TEXT NOT NULL,
                   manifest_json TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS group_leaves (
                   group_id TEXT NOT NULL,
                   member_device_id TEXT NOT NULL,
                   owner_device_id TEXT NOT NULL,
                   leave_id TEXT NOT NULL,
                   status TEXT NOT NULL,
                   PRIMARY KEY(group_id, member_device_id)
                 );
                 CREATE TABLE IF NOT EXISTS group_tombstones (
                   group_id TEXT PRIMARY KEY,
                   revision INTEGER NOT NULL,
                   tombstone_json TEXT NOT NULL
                 );",
            )
            .map_err(|error| format!("无法初始化本地数据库：{error}"))?;
        if previous_version < 3 && !column_exists(&connection, "trusted_devices", "sync_enabled")? {
            connection
                .execute(
                    "ALTER TABLE trusted_devices ADD COLUMN sync_enabled INTEGER NOT NULL DEFAULT 1",
                    [],
                )
                .map_err(|error| format!("无法升级可信设备策略：{error}"))?;
        }
        if previous_version < 6 {
            connection
                .execute_batch(
                    "CREATE TABLE clipboard_slots_v6 (
                       device_id TEXT PRIMARY KEY,
                       sequence INTEGER NOT NULL,
                       object_name TEXT NOT NULL,
                       expires_at_unix INTEGER NOT NULL
                     );
                     INSERT OR REPLACE INTO clipboard_slots_v6
                       SELECT device_id, sequence, object_name, expires_at_unix FROM clipboard_slots;
                     DROP TABLE clipboard_slots;
                     ALTER TABLE clipboard_slots_v6 RENAME TO clipboard_slots;",
                )
                .map_err(|error| format!("无法升级剪贴板缓存授权结构：{error}"))?;
        }
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|error| format!("无法更新数据库版本：{error}"))?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn load_settings(&self) -> Result<Option<AppSettings>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let json = connection
            .query_row(
                "SELECT json FROM app_settings WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("无法读取设置：{error}"))?;
        json.map(|value| {
            serde_json::from_str(&value).map_err(|error| format!("设置数据已损坏：{error}"))
        })
        .transpose()
    }

    pub(crate) fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        let json =
            serde_json::to_string(settings).map_err(|error| format!("无法序列化设置：{error}"))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "INSERT INTO app_settings(singleton, json) VALUES(1, ?1)
                 ON CONFLICT(singleton) DO UPDATE SET json = excluded.json",
                params![json],
            )
            .map_err(|error| format!("无法保存设置：{error}"))?;
        Ok(())
    }

    pub(crate) fn load_runtime(&self) -> Result<Option<StoredRuntime>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .query_row(
                "SELECT publish_paused, subscribe_paused FROM runtime_state WHERE singleton = 1",
                [],
                |row| {
                    Ok(StoredRuntime {
                        publish_paused: row.get(0)?,
                        subscribe_paused: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("无法读取运行设置：{error}"))
    }

    pub(crate) fn save_runtime(
        &self,
        publish_paused: bool,
        subscribe_paused: bool,
    ) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "INSERT INTO runtime_state(singleton, publish_paused, subscribe_paused) VALUES(1, ?1, ?2)
                 ON CONFLICT(singleton) DO UPDATE SET
                   publish_paused = excluded.publish_paused,
                   subscribe_paused = excluded.subscribe_paused",
                params![publish_paused, subscribe_paused],
            )
            .map_err(|error| format!("无法保存运行设置：{error}"))?;
        Ok(())
    }

    pub(crate) fn next_origin_sequence(&self) -> Result<u64, String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("无法开始序号事务：{error}"))?;
        let next: i64 = transaction
            .query_row(
                "INSERT INTO sequence_state(singleton, next_origin_sequence) VALUES(1, 2)
                 ON CONFLICT(singleton) DO UPDATE SET next_origin_sequence = next_origin_sequence + 1
                 RETURNING next_origin_sequence - 1",
                [],
                |row| row.get(0),
            )
            .map_err(|error| format!("无法分配剪贴板序号：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("无法提交剪贴板序号：{error}"))?;
        u64::try_from(next).map_err(|_| "剪贴板序号状态无效".to_string())
    }

    pub(crate) fn save_pending_pairing(
        &self,
        pairing_id: &str,
        device: &TrustedDevice,
        expires_at: &str,
    ) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "INSERT OR REPLACE INTO pending_pairings(
                   pairing_id, device_id, device_name, platform, public_key, certificate_der, expires_at
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    pairing_id,
                    device.device_id,
                    device.device_name,
                    device.platform,
                    device.public_key,
                    device.certificate_der,
                    expires_at
                ],
            )
            .map_err(|error| format!("无法保存待确认配对：{error}"))?;
        Ok(())
    }

    pub(crate) fn promote_trusted_device(
        &self,
        pairing_id: &str,
        paired_at: &str,
    ) -> Result<TrustedDevice, String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("无法开始配对事务：{error}"))?;
        let device = transaction
            .query_row(
                "SELECT device_id, device_name, platform, public_key, certificate_der, ?2
                 FROM pending_pairings WHERE pairing_id = ?1",
                params![pairing_id, paired_at],
                |row| {
                    Ok(TrustedDevice {
                        device_id: row.get(0)?,
                        device_name: row.get(1)?,
                        platform: row.get(2)?,
                        public_key: row.get(3)?,
                        certificate_der: row.get(4)?,
                        paired_at: row.get(5)?,
                        sync_enabled: true,
                    })
                },
            )
            .map_err(|error| format!("待确认配对不存在：{error}"))?;
        transaction
            .execute(
                "INSERT INTO trusted_devices(
                   device_id, device_name, platform, public_key, certificate_der, paired_at, revoked, sync_enabled
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, 0, 1)
                 ON CONFLICT(device_id) DO UPDATE SET
                   device_name = excluded.device_name,
                   platform = excluded.platform,
                   public_key = excluded.public_key,
                   certificate_der = excluded.certificate_der,
                   paired_at = excluded.paired_at,
                   revoked = 0,
                   sync_enabled = 1",
                params![
                    device.device_id,
                    device.device_name,
                    device.platform,
                    device.public_key,
                    device.certificate_der,
                    device.paired_at
                ],
            )
            .map_err(|error| format!("无法保存可信设备：{error}"))?;
        transaction
            .execute(
                "DELETE FROM local_revocations WHERE device_id = ?1",
                params![device.device_id],
            )
            .map_err(|error| format!("无法清除旧撤销记录：{error}"))?;
        transaction
            .execute(
                "DELETE FROM pending_pairings WHERE pairing_id = ?1",
                params![pairing_id],
            )
            .map_err(|error| format!("无法清理待确认配对：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("无法提交可信设备：{error}"))?;
        Ok(device)
    }

    pub(crate) fn remove_pending_pairing(&self, pairing_id: &str) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "DELETE FROM pending_pairings WHERE pairing_id = ?1",
                params![pairing_id],
            )
            .map_err(|error| format!("无法清理待确认配对：{error}"))?;
        Ok(())
    }

    pub(crate) fn trusted_device(&self, device_id: &str) -> Result<Option<TrustedDevice>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .query_row(
                "SELECT device_id, device_name, platform, public_key, certificate_der, paired_at, sync_enabled
                 FROM trusted_devices WHERE device_id = ?1 AND revoked = 0",
                params![device_id],
                |row| {
                    Ok(TrustedDevice {
                        device_id: row.get(0)?,
                        device_name: row.get(1)?,
                        platform: row.get(2)?,
                        public_key: row.get(3)?,
                        certificate_der: row.get(4)?,
                        paired_at: row.get(5)?,
                        sync_enabled: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(|error| format!("无法读取可信设备：{error}"))
    }

    pub(crate) fn trusted_devices(&self) -> Result<Vec<TrustedDevice>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT device_id, device_name, platform, public_key, certificate_der, paired_at, sync_enabled
                 FROM trusted_devices WHERE revoked = 0 ORDER BY device_name",
            )
            .map_err(|error| format!("无法查询可信设备：{error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok(TrustedDevice {
                    device_id: row.get(0)?,
                    device_name: row.get(1)?,
                    platform: row.get(2)?,
                    public_key: row.get(3)?,
                    certificate_der: row.get(4)?,
                    paired_at: row.get(5)?,
                    sync_enabled: row.get(6)?,
                })
            })
            .map_err(|error| format!("无法读取可信设备：{error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("可信设备记录已损坏：{error}"))
    }

    pub(crate) fn is_device_revoked(&self, device_id: &str) -> Result<bool, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .query_row(
                "SELECT 1 FROM local_revocations WHERE device_id = ?1",
                params![device_id],
                |_| Ok(()),
            )
            .optional()
            .map(|value| value.is_some())
            .map_err(|error| format!("无法读取设备撤销状态：{error}"))
    }

    pub(crate) fn is_device_sync_allowed(&self, device_id: &str) -> Result<bool, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .query_row(
                "SELECT sync_enabled FROM trusted_devices
                 WHERE device_id = ?1 AND revoked = 0",
                params![device_id],
                |row| row.get::<_, bool>(0),
            )
            .optional()
            .map(|value| value.unwrap_or(true))
            .map_err(|error| format!("无法读取设备同步策略：{error}"))
    }

    pub(crate) fn set_device_sync_enabled(
        &self,
        device_id: &str,
        enabled: bool,
    ) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let changed = connection
            .execute(
                "UPDATE trusted_devices SET sync_enabled = ?2
                 WHERE device_id = ?1 AND revoked = 0",
                params![device_id, enabled],
            )
            .map_err(|error| format!("无法保存设备同步策略：{error}"))?;
        if changed != 1 {
            return Err("可信设备不存在或已经撤销".into());
        }
        Ok(())
    }

    pub(crate) fn revoke_device(&self, device_id: &str, revoked_at: &str) -> Result<(), String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("无法开始设备撤销事务：{error}"))?;
        let changed = transaction
            .execute(
                "UPDATE trusted_devices SET revoked = 1, sync_enabled = 0 WHERE device_id = ?1",
                params![device_id],
            )
            .map_err(|error| format!("无法撤销可信设备：{error}"))?;
        if changed != 1 {
            return Err("可信设备不存在".into());
        }
        transaction
            .execute(
                "INSERT INTO local_revocations(device_id, revoked_at) VALUES(?1, ?2)
                 ON CONFLICT(device_id) DO UPDATE SET revoked_at = excluded.revoked_at",
                params![device_id, revoked_at],
            )
            .map_err(|error| format!("无法保存设备撤销记录：{error}"))?;
        transaction
            .execute(
                "DELETE FROM pending_pairings WHERE device_id = ?1",
                params![device_id],
            )
            .map_err(|error| format!("无法清理设备配对会话：{error}"))?;
        transaction
            .execute(
                "DELETE FROM clipboard_slots WHERE device_id = ?1",
                params![device_id],
            )
            .map_err(|error| format!("无法清理设备槽位元数据：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("无法提交设备撤销：{error}"))
    }

    pub(crate) fn save_cached_slot(
        &self,
        metadata: &CachedSlotMetadata,
    ) -> Result<Option<String>, String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("无法开始槽位缓存事务：{error}"))?;
        let previous = transaction
            .query_row(
                "SELECT object_name FROM clipboard_slots WHERE device_id = ?1",
                params![metadata.device_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("无法读取旧槽位缓存：{error}"))?;
        transaction
            .execute(
                "INSERT INTO clipboard_slots(device_id, sequence, object_name, expires_at_unix)
                 VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(device_id) DO UPDATE SET
                   sequence = excluded.sequence,
                   object_name = excluded.object_name,
                   expires_at_unix = excluded.expires_at_unix",
                params![
                    metadata.device_id,
                    metadata.sequence,
                    metadata.object_name,
                    metadata.expires_at_unix
                ],
            )
            .map_err(|error| format!("无法保存槽位缓存元数据：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("无法提交槽位缓存元数据：{error}"))?;
        Ok(previous)
    }

    pub(crate) fn cached_slots(&self, now_unix: u64) -> Result<Vec<CachedSlotMetadata>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "DELETE FROM clipboard_slots WHERE expires_at_unix <= ?1",
                params![now_unix],
            )
            .map_err(|error| format!("无法清理过期槽位元数据：{error}"))?;
        let mut statement = connection
            .prepare(
                "SELECT device_id, sequence, object_name, expires_at_unix
                 FROM clipboard_slots WHERE expires_at_unix > ?1",
            )
            .map_err(|error| format!("无法查询槽位缓存：{error}"))?;
        let rows = statement
            .query_map(params![now_unix], |row| {
                Ok(CachedSlotMetadata {
                    device_id: row.get(0)?,
                    sequence: row.get(1)?,
                    object_name: row.get(2)?,
                    expires_at_unix: row.get(3)?,
                })
            })
            .map_err(|error| format!("无法读取槽位缓存：{error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("槽位缓存元数据已损坏：{error}"))
    }

    pub(crate) fn remove_cached_slot(&self, device_id: &str) -> Result<Option<String>, String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("无法开始清理槽位事务：{error}"))?;
        let object = transaction
            .query_row(
                "SELECT object_name FROM clipboard_slots WHERE device_id = ?1",
                params![device_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("无法读取槽位缓存：{error}"))?;
        transaction
            .execute(
                "DELETE FROM clipboard_slots WHERE device_id = ?1",
                params![device_id],
            )
            .map_err(|error| format!("无法删除槽位缓存元数据：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("无法提交槽位清理：{error}"))?;
        Ok(object)
    }

    pub(crate) fn save_group_manifest(
        &self,
        manifest: &SignedGroupManifest,
        local_state: &str,
        updated_at: &str,
    ) -> Result<bool, String> {
        let json = serde_json::to_string(manifest)
            .map_err(|error| format!("无法编码同步组清单：{error}"))?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("无法开始同步组事务：{error}"))?;
        let tombstone_revision = transaction
            .query_row(
                "SELECT revision FROM group_tombstones WHERE group_id = ?1",
                params![manifest.manifest.group_id],
                |row| row.get::<_, u64>(0),
            )
            .optional()
            .map_err(|error| format!("无法读取同步组删除状态：{error}"))?;
        if tombstone_revision.is_some() {
            return Err("同步组已由删除声明永久终结".into());
        }
        let current: Option<(u64, String)> = transaction
            .query_row(
                "SELECT revision, manifest_json FROM sync_groups WHERE group_id = ?1",
                params![manifest.manifest.group_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| format!("无法读取同步组版本：{error}"))?;
        if let Some((revision, current_json)) = current {
            if manifest.manifest.revision < revision {
                return Err("拒绝回滚同步组清单".into());
            }
            if manifest.manifest.revision == revision {
                if current_json == json {
                    return Ok(false);
                }
                return Err("相同同步组版本出现不同内容".into());
            }
        }
        transaction
            .execute(
                "INSERT INTO sync_groups(
                   group_id, owner_device_id, revision, membership_epoch, manifest_json, local_state, updated_at
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(group_id) DO UPDATE SET
                   owner_device_id = excluded.owner_device_id,
                   revision = excluded.revision,
                   membership_epoch = excluded.membership_epoch,
                   manifest_json = excluded.manifest_json,
                   local_state = excluded.local_state,
                   updated_at = excluded.updated_at",
                params![
                    manifest.manifest.group_id,
                    manifest.manifest.owner_device_id,
                    manifest.manifest.revision,
                    manifest.manifest.membership_epoch,
                    json,
                    local_state,
                    updated_at
                ],
            )
            .map_err(|error| format!("无法保存同步组清单：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("无法提交同步组清单：{error}"))?;
        Ok(true)
    }

    pub(crate) fn group_manifests(&self) -> Result<Vec<SignedGroupManifest>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let mut statement = connection
            .prepare("SELECT manifest_json FROM sync_groups WHERE local_state = 'active'")
            .map_err(|error| format!("无法查询同步组：{error}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| format!("无法读取同步组：{error}"))?;
        rows.map(|row| {
            let json = row.map_err(|error| format!("同步组记录已损坏：{error}"))?;
            serde_json::from_str(&json).map_err(|error| format!("同步组清单已损坏：{error}"))
        })
        .collect()
    }

    pub(crate) fn group_manifest(
        &self,
        group_id: &str,
    ) -> Result<Option<SignedGroupManifest>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let json = connection
            .query_row(
                "SELECT manifest_json FROM sync_groups WHERE group_id = ?1 AND local_state = 'active'",
                params![group_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("无法读取同步组：{error}"))?;
        json.map(|value| {
            serde_json::from_str(&value).map_err(|error| format!("同步组清单已损坏：{error}"))
        })
        .transpose()
    }

    pub(crate) fn group_manifest_any(
        &self,
        group_id: &str,
    ) -> Result<Option<SignedGroupManifest>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let json = connection
            .query_row(
                "SELECT manifest_json FROM sync_groups WHERE group_id = ?1",
                params![group_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("无法读取同步组历史清单：{error}"))?;
        json.map(|value| {
            serde_json::from_str(&value).map_err(|error| format!("同步组历史清单已损坏：{error}"))
        })
        .transpose()
    }

    pub(crate) fn set_group_local_state(
        &self,
        group_id: &str,
        local_state: &str,
        updated_at: &str,
    ) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "UPDATE sync_groups SET local_state = ?2, updated_at = ?3 WHERE group_id = ?1",
                params![group_id, local_state, updated_at],
            )
            .map_err(|error| format!("无法更新同步组本地状态：{error}"))?;
        Ok(())
    }

    pub(crate) fn save_group_tombstone(
        &self,
        tombstone: &SignedGroupTombstone,
    ) -> Result<bool, String> {
        let json = serde_json::to_string(tombstone)
            .map_err(|error| format!("无法编码同步组删除声明：{error}"))?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("无法开始同步组删除事务：{error}"))?;
        let current = transaction
            .query_row(
                "SELECT revision, tombstone_json FROM group_tombstones WHERE group_id = ?1",
                params![tombstone.tombstone.group_id],
                |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| format!("无法读取同步组删除版本：{error}"))?;
        if let Some((revision, current_json)) = current {
            if tombstone.tombstone.revision < revision {
                return Err("拒绝回滚同步组删除声明".into());
            }
            if tombstone.tombstone.revision == revision {
                if current_json == json {
                    return Ok(false);
                }
                return Err("相同同步组删除版本出现不同内容".into());
            }
        }
        transaction
            .execute(
                "INSERT INTO group_tombstones(group_id, revision, tombstone_json)
                 VALUES(?1, ?2, ?3)
                 ON CONFLICT(group_id) DO UPDATE SET
                   revision = excluded.revision,
                   tombstone_json = excluded.tombstone_json",
                params![
                    tombstone.tombstone.group_id,
                    tombstone.tombstone.revision,
                    json
                ],
            )
            .map_err(|error| format!("无法保存同步组删除声明：{error}"))?;
        transaction
            .execute(
                "UPDATE sync_groups SET local_state = 'deleted'
                 WHERE group_id = ?1 AND revision < ?2",
                params![tombstone.tombstone.group_id, tombstone.tombstone.revision],
            )
            .map_err(|error| format!("无法应用同步组删除声明：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("无法提交同步组删除声明：{error}"))?;
        Ok(true)
    }

    pub(crate) fn group_tombstones_for_member(
        &self,
        device_id: &str,
    ) -> Result<Vec<SignedGroupTombstone>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT tombstone_json, manifest_json
                 FROM group_tombstones
                 INNER JOIN sync_groups USING(group_id)",
            )
            .map_err(|error| format!("无法查询同步组删除声明：{error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| format!("无法读取同步组删除声明：{error}"))?;
        let mut tombstones = Vec::new();
        for row in rows {
            let (tombstone_json, manifest_json) =
                row.map_err(|error| format!("同步组删除记录已损坏：{error}"))?;
            let manifest: SignedGroupManifest = serde_json::from_str(&manifest_json)
                .map_err(|error| format!("同步组删除历史清单已损坏：{error}"))?;
            if manifest
                .manifest
                .members
                .iter()
                .any(|member| member.device_id == device_id)
            {
                tombstones.push(
                    serde_json::from_str(&tombstone_json)
                        .map_err(|error| format!("同步组删除声明已损坏：{error}"))?,
                );
            }
        }
        Ok(tombstones)
    }

    pub(crate) fn save_group_invite(&self, invite: &StoredGroupInvite) -> Result<(), String> {
        let json = serde_json::to_string(&invite.manifest)
            .map_err(|error| format!("无法编码同步组邀请：{error}"))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let existing = connection
            .query_row(
                "SELECT group_id, target_device_id, expires_at, manifest_json
                 FROM group_invites WHERE invite_id = ?1",
                params![invite.invite_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("无法检查同步组邀请重放：{error}"))?;
        if let Some((group_id, target_device_id, expires_at, existing_json)) = existing {
            if group_id == invite.manifest.manifest.group_id
                && target_device_id == invite.target_device_id
                && expires_at == invite.expires_at
                && existing_json == json
            {
                return Ok(());
            }
            return Err("相同邀请 ID 出现不同内容".into());
        }
        connection
            .execute(
                "INSERT INTO group_invites(
                   invite_id, group_id, target_device_id, expires_at, status, manifest_json
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    invite.invite_id,
                    invite.manifest.manifest.group_id,
                    invite.target_device_id,
                    invite.expires_at,
                    invite.status,
                    json
                ],
            )
            .map_err(|error| format!("无法保存同步组邀请：{error}"))?;
        Ok(())
    }

    pub(crate) fn group_invites(&self, now: &str) -> Result<Vec<StoredGroupInvite>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "DELETE FROM group_invites WHERE expires_at <= ?1 AND status = 'pending'",
                params![now],
            )
            .map_err(|error| format!("无法清理过期同步组邀请：{error}"))?;
        let mut statement = connection
            .prepare(
                "SELECT invite_id, target_device_id, expires_at, status, manifest_json
                 FROM group_invites WHERE status = 'pending' AND expires_at > ?1",
            )
            .map_err(|error| format!("无法查询同步组邀请：{error}"))?;
        let rows = statement
            .query_map(params![now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|error| format!("无法读取同步组邀请：{error}"))?;
        rows.map(|row| {
            let (invite_id, target_device_id, expires_at, status, json) =
                row.map_err(|error| format!("同步组邀请已损坏：{error}"))?;
            Ok(StoredGroupInvite {
                invite_id,
                target_device_id,
                expires_at,
                status,
                manifest: serde_json::from_str(&json)
                    .map_err(|error| format!("同步组邀请清单已损坏：{error}"))?,
            })
        })
        .collect()
    }

    pub(crate) fn set_group_invite_status(
        &self,
        invite_id: &str,
        status: &str,
    ) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "UPDATE group_invites SET status = ?2 WHERE invite_id = ?1",
                params![invite_id, status],
            )
            .map_err(|error| format!("无法更新同步组邀请：{error}"))?;
        Ok(())
    }

    pub(crate) fn save_group_leave(&self, leave: &StoredGroupLeave) -> Result<bool, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let existing = connection
            .query_row(
                "SELECT owner_device_id, leave_id, status FROM group_leaves
                 WHERE group_id = ?1 AND member_device_id = ?2",
                params![leave.group_id, leave.member_device_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("无法读取同步组退出记录：{error}"))?;
        if let Some((owner, leave_id, _status)) = existing {
            if owner == leave.owner_device_id && leave_id == leave.leave_id {
                return Ok(false);
            }
            return Err("相同成员存在冲突的同步组退出记录".into());
        }
        connection
            .execute(
                "INSERT INTO group_leaves(
                   group_id, member_device_id, owner_device_id, leave_id, status
                 ) VALUES(?1, ?2, ?3, ?4, ?5)",
                params![
                    leave.group_id,
                    leave.member_device_id,
                    leave.owner_device_id,
                    leave.leave_id,
                    leave.status
                ],
            )
            .map_err(|error| format!("无法保存同步组退出记录：{error}"))?;
        Ok(true)
    }

    pub(crate) fn group_leaves_for_owner(
        &self,
        owner_device_id: &str,
    ) -> Result<Vec<StoredGroupLeave>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT group_id, member_device_id, leave_id, status FROM group_leaves
                 WHERE owner_device_id = ?1 AND status = 'pending'",
            )
            .map_err(|error| format!("无法查询待发送同步组退出通知：{error}"))?;
        let rows = statement
            .query_map(params![owner_device_id], |row| {
                Ok(StoredGroupLeave {
                    group_id: row.get(0)?,
                    member_device_id: row.get(1)?,
                    owner_device_id: owner_device_id.to_string(),
                    leave_id: row.get(2)?,
                    status: row.get(3)?,
                })
            })
            .map_err(|error| format!("无法读取同步组退出通知：{error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("同步组退出记录已损坏：{error}"))
    }

    pub(crate) fn set_group_leave_status(
        &self,
        group_id: &str,
        member_device_id: &str,
        status: &str,
    ) -> Result<(), String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .execute(
                "UPDATE group_leaves SET status = ?3
                 WHERE group_id = ?1 AND member_device_id = ?2",
                params![group_id, member_device_id, status],
            )
            .map_err(|error| format!("无法更新同步组退出状态：{error}"))?;
        Ok(())
    }

    pub(crate) fn group_invite(
        &self,
        invite_id: &str,
    ) -> Result<Option<StoredGroupInvite>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let row = connection
            .query_row(
                "SELECT target_device_id, expires_at, status, manifest_json
                 FROM group_invites WHERE invite_id = ?1",
                params![invite_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| format!("无法读取同步组邀请：{error}"))?;
        row.map(|(target_device_id, expires_at, status, json)| {
            Ok(StoredGroupInvite {
                invite_id: invite_id.to_string(),
                target_device_id,
                expires_at,
                status,
                manifest: serde_json::from_str(&json)
                    .map_err(|error| format!("同步组邀请清单已损坏：{error}"))?,
            })
        })
        .transpose()
    }

    pub(crate) fn group_invites_for_target(
        &self,
        device_id: &str,
        now: &str,
    ) -> Result<Vec<StoredGroupInvite>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT invite_id, expires_at, status, manifest_json
                 FROM group_invites
                 WHERE target_device_id = ?1 AND status = 'sent' AND expires_at > ?2",
            )
            .map_err(|error| format!("无法查询待发送同步组邀请：{error}"))?;
        let rows = statement
            .query_map(params![device_id, now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|error| format!("无法读取待发送同步组邀请：{error}"))?;
        rows.map(|row| {
            let (invite_id, expires_at, status, json) =
                row.map_err(|error| format!("同步组邀请已损坏：{error}"))?;
            Ok(StoredGroupInvite {
                invite_id,
                target_device_id: device_id.to_string(),
                expires_at,
                status,
                manifest: serde_json::from_str(&json)
                    .map_err(|error| format!("同步组邀请清单已损坏：{error}"))?,
            })
        })
        .collect()
    }

    pub(crate) fn group_invite_responses_for_owner(
        &self,
        owner_device_id: &str,
    ) -> Result<Vec<StoredGroupInvite>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT invite_id, target_device_id, expires_at, status, manifest_json
                 FROM group_invites WHERE status IN ('accepted', 'rejected')",
            )
            .map_err(|error| format!("无法查询同步组邀请响应：{error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|error| format!("无法读取同步组邀请响应：{error}"))?;
        let mut invites = Vec::new();
        for row in rows {
            let (invite_id, target_device_id, expires_at, status, json) =
                row.map_err(|error| format!("同步组邀请响应已损坏：{error}"))?;
            let manifest: SignedGroupManifest = serde_json::from_str(&json)
                .map_err(|error| format!("同步组邀请响应清单已损坏：{error}"))?;
            if manifest.manifest.owner_device_id == owner_device_id {
                invites.push(StoredGroupInvite {
                    invite_id,
                    target_device_id,
                    expires_at,
                    status,
                    manifest,
                });
            }
        }
        Ok(invites)
    }

    pub(crate) fn has_accepted_group_invite(
        &self,
        group_id: &str,
        target_device_id: &str,
    ) -> Result<bool, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .query_row(
                "SELECT 1 FROM group_invites
                 WHERE group_id = ?1 AND target_device_id = ?2 AND status = 'accepted'",
                params![group_id, target_device_id],
                |_| Ok(()),
            )
            .optional()
            .map(|value| value.is_some())
            .map_err(|error| format!("无法读取同步组邀请接受状态：{error}"))
    }
}

fn column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("无法检查数据库结构：{error}"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("无法读取数据库结构：{error}"))?;
    for name in names {
        if name.map_err(|error| format!("数据库结构已损坏：{error}"))? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        group::{
            GroupManifest, GroupMember, GroupPolicy, GroupTombstone, MemberState,
            SignedGroupTombstone, SyncDirection, GROUP_ENCODING_VERSION,
        },
        identity::Identity,
    };
    use data_encoding::BASE64;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("airdrop-store-{nonce}"))
    }

    #[test]
    fn persists_settings_and_pause_state() {
        let directory = temporary_directory();
        let store = Store::open(&directory).unwrap();
        let settings = AppSettings {
            allow_images: false,
            ..AppSettings::default()
        };
        store.save_settings(&settings).unwrap();
        store.save_runtime(true, false).unwrap();

        let reopened = Store::open(&directory).unwrap();
        assert!(!reopened.load_settings().unwrap().unwrap().allow_images);
        let runtime = reopened.load_runtime().unwrap().unwrap();
        assert!(runtime.publish_paused);
        assert!(!runtime.subscribe_paused);
        assert_eq!(reopened.next_origin_sequence().unwrap(), 1);
        assert_eq!(reopened.next_origin_sequence().unwrap(), 2);

        let pending = TrustedDevice {
            device_id: "ld1_test".into(),
            device_name: "Test PC".into(),
            platform: "windows".into(),
            public_key: vec![7; 32],
            certificate_der: vec![8; 64],
            paired_at: "2026-07-13T00:00:00Z".into(),
            sync_enabled: true,
        };
        reopened
            .save_pending_pairing("pair-1", &pending, "2026-07-13T00:02:00Z")
            .unwrap();
        let trusted = reopened
            .promote_trusted_device("pair-1", "2026-07-13T00:01:00Z")
            .unwrap();
        assert_eq!(trusted.device_name, "Test PC");
        assert_eq!(
            reopened
                .trusted_device("ld1_test")
                .unwrap()
                .unwrap()
                .certificate_der,
            vec![8; 64]
        );
        reopened
            .save_cached_slot(&CachedSlotMetadata {
                device_id: "ld1_test".into(),
                sequence: 9,
                object_name: "slot.ldcache".into(),
                expires_at_unix: u64::MAX / 2,
            })
            .unwrap();
        assert_eq!(reopened.cached_slots(1).unwrap()[0].sequence, 9);
        reopened.set_device_sync_enabled("ld1_test", false).unwrap();
        assert!(
            !reopened
                .trusted_device("ld1_test")
                .unwrap()
                .unwrap()
                .sync_enabled
        );
        reopened
            .revoke_device("ld1_test", "2026-07-13T00:03:00Z")
            .unwrap();
        assert!(reopened.trusted_device("ld1_test").unwrap().is_none());
        assert!(reopened.cached_slots(1).unwrap().is_empty());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn persists_signed_groups_and_rejects_revision_conflicts() {
        let directory = temporary_directory();
        let identity = Identity::load_or_create(&directory).unwrap();
        let store = Store::open(&directory).unwrap();
        let member = GroupMember {
            device_id: identity.device_id().into(),
            device_name: "Owner".into(),
            platform: "linux".into(),
            public_key: BASE64.encode(&identity.public_key_bytes()),
            certificate: BASE64.encode(b"certificate"),
            joined_at: "2026-07-13T00:00:00Z".into(),
            state: MemberState::Active,
            direction: SyncDirection::Bidirectional,
        };
        let make_manifest = |revision| {
            SignedGroupManifest::sign(
                GroupManifest {
                    encoding_version: GROUP_ENCODING_VERSION,
                    group_id: "00112233-4455-6677-8899-aabbccddeeff".into(),
                    owner_device_id: identity.device_id().into(),
                    name: "Test Group".into(),
                    revision,
                    membership_epoch: revision,
                    policy: GroupPolicy {
                        allow_text: true,
                        allow_images: true,
                        allow_html: false,
                        allow_files: false,
                        offline_ttl_seconds: 86_400,
                    },
                    members: vec![member.clone()],
                },
                &identity,
            )
            .unwrap()
        };
        let revision_two = make_manifest(2);
        assert!(store
            .save_group_manifest(&revision_two, "active", "2026-07-13T00:00:00Z")
            .unwrap());
        assert!(!store
            .save_group_manifest(&revision_two, "active", "2026-07-13T00:00:01Z")
            .unwrap());
        assert!(store
            .save_group_manifest(&make_manifest(1), "active", "2026-07-13T00:00:02Z")
            .is_err());
        assert_eq!(store.group_manifests().unwrap()[0].manifest.revision, 2);

        let invite = StoredGroupInvite {
            invite_id: "invite-1".into(),
            target_device_id: identity.device_id().into(),
            expires_at: "2026-07-13T00:10:00Z".into(),
            status: "sent".into(),
            manifest: revision_two.clone(),
        };
        store.save_group_invite(&invite).unwrap();
        store.save_group_invite(&invite).unwrap();
        assert_eq!(
            store
                .group_invites_for_target(identity.device_id(), "2026-07-13T00:05:00Z")
                .unwrap()
                .len(),
            1
        );
        store
            .set_group_invite_status("invite-1", "accepted")
            .unwrap();
        assert_eq!(
            store
                .group_invite_responses_for_owner(identity.device_id())
                .unwrap()[0]
                .status,
            "accepted"
        );
        let conflicting_invite = StoredGroupInvite {
            expires_at: "2026-07-13T00:11:00Z".into(),
            ..invite
        };
        assert!(store.save_group_invite(&conflicting_invite).is_err());

        let leave = StoredGroupLeave {
            group_id: revision_two.manifest.group_id.clone(),
            member_device_id: identity.device_id().into(),
            owner_device_id: identity.device_id().into(),
            leave_id: "leave-1".into(),
            status: "pending".into(),
        };
        assert!(store.save_group_leave(&leave).unwrap());
        assert!(!store.save_group_leave(&leave).unwrap());

        let tombstone = SignedGroupTombstone::sign(
            GroupTombstone {
                encoding_version: GROUP_ENCODING_VERSION,
                group_id: revision_two.manifest.group_id.clone(),
                owner_device_id: identity.device_id().into(),
                revision: 3,
                membership_epoch: 3,
                deleted_at: "2026-07-13T00:00:03Z".into(),
            },
            &identity,
        )
        .unwrap();
        assert!(store.save_group_tombstone(&tombstone).unwrap());
        assert!(!store.save_group_tombstone(&tombstone).unwrap());
        assert_eq!(
            store
                .group_tombstones_for_member(identity.device_id())
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .group_manifest(&revision_two.manifest.group_id)
            .unwrap()
            .is_none());
        assert!(store
            .save_group_manifest(&make_manifest(4), "active", "2026-07-13T00:00:04Z")
            .is_err());
        let _ = fs::remove_dir_all(directory);
    }
}
