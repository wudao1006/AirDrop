use super::SystemClipboardContent;
use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContent, ClipboardContext, ContentFormat,
};

pub(crate) struct ExtendedClipboard {
    context: ClipboardContext,
}

impl ExtendedClipboard {
    pub(crate) fn new() -> Result<Self, String> {
        ClipboardContext::new()
            .map(|context| Self { context })
            .map_err(|error| format!("无法初始化系统富文本剪贴板：{error}"))
    }

    pub(crate) fn read_content(&self) -> Result<SystemClipboardContent, String> {
        let formats = self
            .context
            .available_formats()
            .map_err(|error| format!("无法读取系统剪贴板格式：{error}"))?;

        let text = if self.context.has(ContentFormat::Text) {
            self.context
                .get_text()
                .ok()
                .filter(|value| !value.trim().is_empty())
        } else {
            None
        };
        let html = if self.context.has(ContentFormat::Html) {
            self.context
                .get_html()
                .ok()
                .filter(|value| !value.trim().is_empty())
        } else {
            None
        };
        let rtf = if self.context.has(ContentFormat::Rtf) {
            self.context
                .get_rich_text()
                .ok()
                .filter(|value| !value.trim().is_empty())
        } else {
            None
        };
        let rich = (html.is_some() || rtf.is_some())
            .then(|| (text.clone().unwrap_or_default(), html, rtf));
        let image = if self.context.has(ContentFormat::Image) {
            self.context.get_image().ok().and_then(|image| {
                let (width, height) = image.get_size();
                image
                    .get_dynamic_image()
                    .ok()
                    .map(|image| (image.to_rgba8().into_raw(), width, height))
            })
        } else {
            None
        };
        let mut files = if self.context.has(ContentFormat::Files) {
            self.context.get_files().unwrap_or_default()
        } else {
            Vec::new()
        }
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
        if !formats.is_empty()
            && text.is_none()
            && rich.is_none()
            && image.is_none()
            && files.is_empty()
        {
            return Err("系统剪贴板包含暂不支持或暂时无法解码的格式".into());
        }
        Ok(SystemClipboardContent {
            text,
            rich,
            image,
            files,
        })
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
