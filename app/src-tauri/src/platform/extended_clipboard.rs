use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, ContentFormat};

pub(crate) struct ExtendedClipboard {
    context: ClipboardContext,
}

impl ExtendedClipboard {
    pub(crate) fn new() -> Result<Self, String> {
        ClipboardContext::new()
            .map(|context| Self { context })
            .map_err(|error| format!("无法初始化系统富文本剪贴板：{error}"))
    }

    pub(crate) fn read_rich(&self) -> Option<(String, Option<String>, Option<String>)> {
        let html = self
            .context
            .has(ContentFormat::Html)
            .then(|| self.context.get_html().ok())
            .flatten()
            .filter(|value| !value.trim().is_empty());
        let rtf = self
            .context
            .has(ContentFormat::Rtf)
            .then(|| self.context.get_rich_text().ok())
            .flatten()
            .filter(|value| !value.trim().is_empty());
        if html.is_none() && rtf.is_none() {
            return None;
        }
        let text = self.context.get_text().unwrap_or_default();
        Some((text, html, rtf))
    }

    pub(crate) fn read_files(&self) -> Vec<String> {
        if !self.context.has(ContentFormat::Files) {
            return Vec::new();
        }
        let mut files = self
            .context
            .get_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| {
                if value.starts_with("file:") {
                    url::Url::parse(&value)
                        .ok()?
                        .to_file_path()
                        .ok()
                        .map(|path| path.to_string_lossy().into_owned())
                } else {
                    Some(value)
                }
            })
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        files
    }
}

pub(crate) fn write_rich_clipboard(
    text: String,
    html: Option<String>,
    rtf: Option<String>,
) -> Result<(), String> {
    let context =
        ClipboardContext::new().map_err(|error| format!("无法初始化系统富文本剪贴板：{error}"))?;
    let mut contents = Vec::with_capacity(3);
    if !text.is_empty() {
        contents.push(ClipboardContent::Text(text));
    }
    if let Some(html) = html.filter(|value| !value.is_empty()) {
        contents.push(ClipboardContent::Html(html));
    }
    if let Some(rtf) = rtf.filter(|value| !value.is_empty()) {
        contents.push(ClipboardContent::Rtf(rtf));
    }
    if contents.is_empty() {
        return Err("富文本剪贴板内容为空".into());
    }
    context
        .set(contents)
        .map_err(|error| format!("无法写入系统富文本剪贴板：{error}"))
}

pub(crate) fn write_file_clipboard(files: Vec<String>) -> Result<(), String> {
    if files.is_empty() {
        return Err("文件剪贴板内容为空".into());
    }
    let context =
        ClipboardContext::new().map_err(|error| format!("无法初始化系统文件剪贴板：{error}"))?;
    context
        .set_files(files)
        .map_err(|error| format!("无法写入系统文件剪贴板：{error}"))
}
