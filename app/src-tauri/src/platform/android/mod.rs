use std::time::Duration;

// Android clipboard access is foreground-scoped and resumes with the app lifecycle.
pub(super) fn clipboard_poll_interval() -> Duration {
    Duration::from_millis(900)
}

pub(super) fn platform_name() -> &'static str {
    "android"
}

pub(super) fn device_name() -> String {
    let properties = android_system_properties::AndroidSystemProperties::new();
    properties
        .get("ro.product.marketname")
        .or_else(|| properties.get("ro.product.model"))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Android Device".into())
}
