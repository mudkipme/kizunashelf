use axum::body::{self, Body};
use axum::http::{header, Method, Request};
use axum::Router;
use kizunashelf::api::{router, ApiOptions};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{Manager, State};
use tower::ServiceExt;

struct DesktopState {
    api: Router,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopApiResponse {
    status: u16,
    body: String,
    content_type: Option<String>,
}

#[tauri::command]
async fn api_request(
    state: State<'_, DesktopState>,
    method: String,
    url: String,
    body: Option<String>,
) -> Result<DesktopApiResponse, String> {
    let method = method
        .parse::<Method>()
        .map_err(|error| format!("Invalid method {method}: {error}"))?;
    let body = body.unwrap_or_default();
    let response = state
        .api
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(url)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .map_err(|error| error.to_string())?,
        )
        .await
        .map_err(|error| error.to_string())?;
    let (parts, body) = response.into_parts();
    let status = parts.status;
    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = body::to_bytes(body, usize::MAX)
        .await
        .map_err(|error| error.to_string())?;
    let body = String::from_utf8(bytes.to_vec())
        .map_err(|error| format!("API returned non-UTF-8 response: {error}"))?;

    Ok(DesktopApiResponse {
        status: status.as_u16(),
        body,
        content_type,
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_path = discover_config_path();
            let cache_ttl = env::var("KIZUNASHELF_CACHE_TTL_MS")
                .ok()
                .and_then(|ttl| ttl.parse::<u64>().ok())
                .unwrap_or(10_000);
            app.manage(DesktopState {
                api: router(ApiOptions {
                    config_path,
                    cache_ttl: Duration::from_millis(cache_ttl),
                    web_dist_path: None,
                    settings_writable: true,
                }),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api_request])
        .run(tauri::generate_context!())
        .expect("failed to run KizunaShelf desktop app");
}

fn discover_config_path() -> PathBuf {
    let candidates = config_candidates();
    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .or_else(|| candidates.first().cloned())
        .unwrap_or_else(|| PathBuf::from("kizunashelf.config.json"))
}

fn config_candidates() -> Vec<PathBuf> {
    config_candidates_from_values(
        env::var_os("KIZUNASHELF_CONFIG").as_deref(),
        env::var_os("HOME").as_deref(),
        env::var_os("XDG_CONFIG_HOME").as_deref(),
        env::var_os("XDG_CONFIG_DIR").as_deref(),
        env::var_os("XDG_CONFIG_DIRS").as_deref(),
        cfg!(target_os = "macos"),
    )
}

fn config_candidates_from_values(
    explicit: Option<&OsStr>,
    home: Option<&OsStr>,
    xdg_config_home: Option<&OsStr>,
    xdg_config_dir: Option<&OsStr>,
    xdg_config_dirs: Option<&OsStr>,
    include_macos_application_support: bool,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit.filter(|value| !value.is_empty()) {
        candidates.push(PathBuf::from(path));
    }
    if let Some(path) = xdg_config_home.filter(|value| !value.is_empty()) {
        candidates.push(Path::new(path).join("kizunashelf.config.json"));
    }
    if let Some(home) = home.filter(|value| !value.is_empty()) {
        let home = Path::new(home);
        candidates.push(home.join(".config/kizunashelf.config.json"));
        if include_macos_application_support {
            candidates.push(home.join("Library/Application Support/kizunashelf.config.json"));
            candidates
                .push(home.join("Library/Application Support/KizunaShelf/kizunashelf.config.json"));
        }
    }
    if let Some(path) = xdg_config_dir.filter(|value| !value.is_empty()) {
        candidates.push(Path::new(path).join("kizunashelf.config.json"));
    }
    if let Some(paths) = xdg_config_dirs.and_then(|value| value.to_str()) {
        for path in paths.split(':').filter(|path| !path.is_empty()) {
            candidates.push(Path::new(path).join("kizunashelf.config.json"));
        }
    }
    dedupe_paths(candidates)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.contains(&path) {
            deduped.push(path);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::config_candidates_from_values;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn config_candidates_include_xdg_home_and_config_dirs() {
        let candidates = config_candidates_from_values(
            Some(OsStr::new("/custom/config.json")),
            Some(OsStr::new("/home/mudkip")),
            Some(OsStr::new("/tmp/xdg")),
            Some(OsStr::new("/etc/xdg-single")),
            Some(OsStr::new("/etc/xdg:/usr/local/etc/xdg")),
            false,
        );

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/custom/config.json"),
                PathBuf::from("/tmp/xdg/kizunashelf.config.json"),
                PathBuf::from("/home/mudkip/.config/kizunashelf.config.json"),
                PathBuf::from("/etc/xdg-single/kizunashelf.config.json"),
                PathBuf::from("/etc/xdg/kizunashelf.config.json"),
                PathBuf::from("/usr/local/etc/xdg/kizunashelf.config.json"),
            ]
        );
    }

    #[test]
    fn config_candidates_include_macos_application_support() {
        let candidates = config_candidates_from_values(
            None,
            Some(OsStr::new("/Users/mudkip")),
            None,
            None,
            None,
            true,
        );

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/Users/mudkip/.config/kizunashelf.config.json"),
                PathBuf::from("/Users/mudkip/Library/Application Support/kizunashelf.config.json"),
                PathBuf::from(
                    "/Users/mudkip/Library/Application Support/KizunaShelf/kizunashelf.config.json"
                ),
            ]
        );
    }
}
