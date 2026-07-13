use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

pub(crate) const MAX_FILE_BUNDLE_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_FILE_ENTRIES: usize = 512;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileEntry {
    pub(crate) relative_path: String,
    pub(crate) size: u64,
    pub(crate) sha256: String,
    pub(crate) is_directory: bool,
}

pub(crate) struct StagedFileBundle {
    pub(crate) root: PathBuf,
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) total_size: u64,
}

impl Drop for StagedFileBundle {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) struct ReceivedFileBundle {
    root: PathBuf,
    entries: Vec<FileEntry>,
}

impl ReceivedFileBundle {
    pub(crate) fn new(root: PathBuf, entries: Vec<FileEntry>) -> Self {
        Self { root, entries }
    }

    pub(crate) fn clipboard_paths(&self) -> Vec<String> {
        let nested = self
            .entries
            .iter()
            .map(|entry| Path::new(&entry.relative_path))
            .filter_map(|path| path.components().next())
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name.to_owned()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut paths = nested
            .into_iter()
            .map(|name| self.root.join(name).to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    pub(crate) fn display_names(&self) -> Vec<String> {
        self.clipboard_paths()
            .into_iter()
            .filter_map(|path| {
                Path::new(&path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .collect()
    }
}

impl Drop for ReceivedFileBundle {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn stage_file_bundle(
    sources: &[String],
    cache_root: &Path,
    sequence: u64,
) -> Result<StagedFileBundle, String> {
    if sources.is_empty() {
        return Err("文件剪贴板为空".into());
    }
    create_private_dir_all(cache_root)?;
    let root = cache_root.join(format!(
        "outgoing-{sequence}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    create_private_dir_all(&root)?;
    let result = stage_sources(sources, &root);
    match result {
        Ok((entries, total_size)) => Ok(StagedFileBundle {
            root,
            entries,
            total_size,
        }),
        Err(error) => {
            let _ = fs::remove_dir_all(root);
            Err(error)
        }
    }
}

fn stage_sources(sources: &[String], root: &Path) -> Result<(Vec<FileEntry>, u64), String> {
    let mut entries = Vec::new();
    let mut total_size = 0_u64;
    let mut root_names = HashSet::new();
    for source in sources {
        let source = Path::new(source);
        let metadata = fs::symlink_metadata(source)
            .map_err(|error| format!("无法读取待同步文件 {}：{error}", source.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "为避免路径逃逸，不同步符号链接：{}",
                source.display()
            ));
        }
        let name = source
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("文件名不是有效 UTF-8：{}", source.display()))?;
        let name = unique_name(&portable_name(name), &mut root_names);
        stage_path(
            source,
            Path::new(&name),
            root,
            &mut entries,
            &mut total_size,
        )?;
    }
    Ok((entries, total_size))
}

fn unique_name(name: &str, used: &mut HashSet<String>) -> String {
    if used.insert(name.to_string()) {
        return name.to_string();
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 2..=u32::MAX {
        let candidate = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("u32 root name space exhausted")
}

fn stage_path(
    source: &Path,
    relative: &Path,
    root: &Path,
    entries: &mut Vec<FileEntry>,
    total_size: &mut u64,
) -> Result<(), String> {
    if entries.len() >= MAX_FILE_ENTRIES {
        return Err(format!("文件条目超过 {MAX_FILE_ENTRIES} 个"));
    }
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("无法读取待同步文件 {}：{error}", source.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "为避免路径逃逸，不同步符号链接：{}",
            source.display()
        ));
    }
    let relative_path = relative_to_string(relative)?;
    let destination = root.join(relative);
    if metadata.is_dir() {
        create_private_dir_all(&destination)
            .map_err(|error| format!("无法暂存目录 {}：{error}", source.display()))?;
        entries.push(FileEntry {
            relative_path,
            size: 0,
            sha256: String::new(),
            is_directory: true,
        });
        let mut children = fs::read_dir(source)
            .map_err(|error| format!("无法读取目录 {}：{error}", source.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("无法遍历目录 {}：{error}", source.display()))?;
        children.sort_by_key(fs::DirEntry::file_name);
        let mut child_names = HashSet::new();
        for child in children {
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| format!("目录包含非 UTF-8 文件名：{}", source.display()))?;
            let name = unique_name(&portable_name(&name), &mut child_names);
            stage_path(
                &child.path(),
                &relative.join(name),
                root,
                entries,
                total_size,
            )?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(format!("不支持的文件类型：{}", source.display()));
    }
    *total_size = total_size
        .checked_add(metadata.len())
        .ok_or_else(|| "文件总大小溢出".to_string())?;
    if *total_size > MAX_FILE_BUNDLE_BYTES {
        return Err(format!(
            "文件剪贴板超过 {} MiB",
            MAX_FILE_BUNDLE_BYTES / 1024 / 1024
        ));
    }
    let mut input = fs::File::open(source)
        .map_err(|error| format!("无法打开待同步文件 {}：{error}", source.display()))?;
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(&destination)
        .map_err(|error| format!("无法暂存文件 {}：{error}", source.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("无法读取待同步文件 {}：{error}", source.display()))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|error| format!("无法暂存文件 {}：{error}", source.display()))?;
    }
    output
        .sync_all()
        .map_err(|error| format!("无法提交暂存文件 {}：{error}", source.display()))?;
    entries.push(FileEntry {
        relative_path,
        size: metadata.len(),
        sha256: HEXLOWER.encode(&hash.finalize()),
        is_directory: false,
    });
    Ok(())
}

pub(crate) fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.len() > 1024 || value.starts_with('/') || value.contains('\\') {
        return Err("文件清单路径无效".into());
    }
    let components = value.split('/').collect::<Vec<_>>();
    if components.len() > 32 {
        return Err("文件清单路径层级过深".into());
    }
    let mut path = PathBuf::new();
    for component in components {
        if component.is_empty() || component == "." || component == ".." || component.len() > 255 {
            return Err("文件清单包含路径逃逸".into());
        }
        path.push(component);
    }
    Ok(path)
}

fn relative_to_string(path: &Path) -> Result<String, String> {
    let value = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| "文件相对路径不是有效 UTF-8".to_string()),
            _ => Err("文件相对路径无效".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    safe_relative_path(&value)?;
    Ok(value)
}

fn portable_name(name: &str) -> String {
    let mut value = name
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    value = value.trim_end_matches([' ', '.']).to_string();
    if value.is_empty() {
        value.push('_');
    }
    let stem = Path::new(&value)
        .file_stem()
        .and_then(|part| part.to_str())
        .unwrap_or("")
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        value.insert(0, '_');
    }
    value
}

pub(crate) fn prepare_file_cache(root: &Path) {
    let _ = create_private_dir_all(root);
    let incoming = root.join("incoming");
    let outgoing = root.join("outgoing");
    let _ = create_private_dir_all(&incoming);
    let _ = create_private_dir_all(&outgoing);
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(24 * 60 * 60))
        .unwrap_or(std::time::UNIX_EPOCH);
    prune_expired_cache(&incoming, cutoff, 2);
    prune_expired_cache(&outgoing, cutoff, 1);
}

fn prune_expired_cache(root: &Path, cutoff: std::time::SystemTime, depth: u8) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.modified().is_ok_and(|modified| modified < cutoff) {
            if metadata.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        } else if depth > 0 && metadata.is_dir() {
            prune_expired_cache(&path, cutoff, depth - 1);
        }
    }
}

fn create_private_dir_all(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| format!("无法创建私有文件缓存目录：{error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("无法限制文件缓存目录权限：{error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_escaping_relative_paths() {
        assert!(safe_relative_path("folder/file.txt").is_ok());
        assert!(safe_relative_path("../secret").is_err());
        assert!(safe_relative_path("/absolute").is_err());
        assert!(safe_relative_path("folder/./file.txt").is_err());
    }

    #[test]
    fn stages_files_and_directories_with_verified_manifest() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("airdrop-file-bundle-{nonce}"));
        let source = root.join("source");
        let cache = root.join("cache");
        fs::create_dir_all(source.join("folder")).unwrap();
        fs::write(source.join("hello.txt"), b"hello").unwrap();
        fs::write(source.join("folder").join("world.txt"), b"world").unwrap();
        let bundle = stage_file_bundle(
            &[
                source.join("hello.txt").to_string_lossy().into_owned(),
                source.join("folder").to_string_lossy().into_owned(),
            ],
            &cache,
            7,
        )
        .unwrap();
        assert_eq!(bundle.total_size, 10);
        assert_eq!(bundle.entries.len(), 3);
        assert!(bundle
            .entries
            .iter()
            .all(|entry| entry.is_directory || entry.sha256.len() == 64));
        let staged_root = bundle.root.clone();
        drop(bundle);
        assert!(!staged_root.exists());
        let _ = fs::remove_dir_all(root);
    }
}
