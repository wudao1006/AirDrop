use super::service::AppSettings;
use rusqlite::{params, Connection, OptionalExtension};
use std::{fs, path::Path, sync::Mutex};

const SCHEMA_VERSION: i64 = 2;

#[derive(Clone, Debug)]
pub(crate) struct TrustedDevice {
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) platform: String,
    pub(crate) public_key: Vec<u8>,
    pub(crate) certificate_der: Vec<u8>,
    pub(crate) paired_at: String,
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
                 );",
            )
            .map_err(|error| format!("无法初始化本地数据库：{error}"))?;
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
                    })
                },
            )
            .map_err(|error| format!("待确认配对不存在：{error}"))?;
        transaction
            .execute(
                "INSERT INTO trusted_devices(
                   device_id, device_name, platform, public_key, certificate_der, paired_at, revoked
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, 0)
                 ON CONFLICT(device_id) DO UPDATE SET
                   device_name = excluded.device_name,
                   platform = excluded.platform,
                   public_key = excluded.public_key,
                   certificate_der = excluded.certificate_der,
                   paired_at = excluded.paired_at,
                   revoked = 0",
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
                "DELETE FROM pending_pairings WHERE pairing_id = ?1",
                params![pairing_id],
            )
            .map_err(|error| format!("无法清理待确认配对：{error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("无法提交可信设备：{error}"))?;
        Ok(device)
    }

    pub(crate) fn trusted_device(&self, device_id: &str) -> Result<Option<TrustedDevice>, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "本地数据库锁已损坏".to_string())?;
        connection
            .query_row(
                "SELECT device_id, device_name, platform, public_key, certificate_der, paired_at
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
                "SELECT device_id, device_name, platform, public_key, certificate_der, paired_at
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
                })
            })
            .map_err(|error| format!("无法读取可信设备：{error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("可信设备记录已损坏：{error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut settings = AppSettings::default();
        settings.allow_images = false;
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
        let _ = fs::remove_dir_all(directory);
    }
}
