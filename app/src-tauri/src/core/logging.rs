use std::{fs, path::Path, time::Duration};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

pub(crate) struct LogGuard {
    _guard: WorkerGuard,
}

pub(crate) fn initialize(data_dir: &Path) -> Result<LogGuard, String> {
    let log_dir = data_dir.join("logs");
    fs::create_dir_all(&log_dir).map_err(|error| format!("无法创建日志目录：{error}"))?;
    remove_expired_logs(&log_dir);
    let file = tracing_appender::rolling::daily(log_dir, "airdrop.log");
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("airdrop_app_lib=info,airdrop_app=info,warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_target(true)
        .with_writer(writer)
        .try_init()
        .map_err(|error| format!("无法初始化日志：{error}"))?;
    Ok(LogGuard { _guard: guard })
}

fn remove_expired_logs(log_dir: &Path) {
    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let expired = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
            .is_ok_and(|age| age > Duration::from_secs(14 * 24 * 60 * 60));
        if expired && path.is_file() {
            let _ = fs::remove_file(path);
        }
    }
}
