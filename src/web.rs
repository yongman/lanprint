use crate::{
    AppState,
    model::{Config, file_kind},
};
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Multipart, Path, Query, Request, State,
        multipart::{MultipartError, MultipartRejection},
    },
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
use tokio::io::AsyncWriteExt;

pub const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

type ApiResult = Result<Json<Value>, ApiError>;
#[derive(Debug)]
pub struct ApiError(StatusCode, String);
impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        anyhow::Error::from(error).into()
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(%error,"API error");
        Self(StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}"))
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
fn invalid(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/settings",
            get(|| async { Html(include_str!("../web/settings.html")) }),
        )
        .route(
            "/settings.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/settings.js"),
                )
            }),
        )
        .route(
            "/i18n.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/i18n.js"),
                )
            }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/style.css"),
                )
            }),
        )
        .route("/api/config", get(get_config).put(save_config))
        .route("/api/status", get(status))
        .route("/api/monitor/start", post(start))
        .route("/api/monitor/pause", post(pause))
        .route("/api/jobs", get(history))
        .route("/api/stats/weekly", get(weekly_stats))
        .route(
            "/api/uploads",
            post(upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES + 16 * 1024)),
        )
        .route("/api/jobs/{id}/retry", post(retry))
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), protect))
        .with_state(state)
}

async fn protect(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok());
    if !host.is_some_and(|h| state.allowed_hosts.iter().any(|allowed| allowed == h)) {
        return (StatusCode::FORBIDDEN, "Invalid Host header").into_response();
    }
    if !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        if request
            .headers()
            .get("x-lanprint-request")
            .and_then(|h| h.to_str().ok())
            != Some("1")
        {
            return (StatusCode::FORBIDDEN, "Missing X-Lanprint-Request header").into_response();
        }
        if let Some(origin) = request.headers().get(header::ORIGIN) {
            let expected = format!("http://{}", host.unwrap());
            if origin.to_str().ok() != Some(&expected) {
                return (StatusCode::FORBIDDEN, "Cross-origin request denied").into_response();
            }
        }
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
            .parse()
            .unwrap(),
    );
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

async fn get_config(State(state): State<Arc<AppState>>) -> ApiResult {
    let config = state.store.lock().unwrap().config()?;
    Ok(Json(
        json!({"restart_required":config != state.active_config,
        "config":config,"active_config":state.active_config}),
    ))
}
async fn save_config(
    State(state): State<Arc<AppState>>,
    Json(mut config): Json<Config>,
) -> ApiResult {
    let control = state.control.lock().unwrap();
    if control.monitoring {
        return Err(invalid("Pause monitoring before changing configuration"));
    }
    if control.busy {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Wait for the active scan or print job to finish before changing configuration".into(),
        ));
    }
    config
        .validate(&state.data_dir)
        .map_err(|e| invalid(format!("{e:#}")))?;
    state.store.lock().unwrap().save_config(&config)?;
    Ok(Json(
        json!({"restart_required":Some(&config) != state.active_config.as_ref(),
        "config":config,"active_config":state.active_config}),
    ))
}
async fn status(State(state): State<Arc<AppState>>) -> ApiResult {
    let control = state.control.lock().unwrap();
    let store = state.store.lock().unwrap();
    Ok(Json(
        json!({"monitoring":control.monitoring,"busy":control.busy,
            "configured":state.active_config.is_some(),"active_config":state.active_config,
            "restart_required":store.config()? != state.active_config,
            "max_upload_bytes":MAX_UPLOAD_BYTES,"counts":store.counts()?,"runtime":*state.runtime.lock().unwrap()}),
    ))
}
async fn start(State(state): State<Arc<AppState>>) -> ApiResult {
    let mut control = state.control.lock().unwrap();
    if control.busy {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Wait for the active scan or print job to finish before starting monitoring".into(),
        ));
    }
    let mut config = state.active_config.clone().ok_or_else(|| {
        ApiError(
            StatusCode::CONFLICT,
            "Save configuration and restart the service first".into(),
        )
    })?;
    config
        .validate(&state.data_dir)
        .map_err(|e| invalid(format!("{e:#}")))?;
    control.monitoring = true;
    Ok(Json(json!({"monitoring":true})))
}
async fn pause(State(state): State<Arc<AppState>>) -> ApiResult {
    state.control.lock().unwrap().monitoring = false;
    Ok(Json(
        json!({"monitoring":false,"message":"An active scan finishes its current snapshot and discards it only if it has not been queued. Already queued directory jobs wait for Start; uploads continue automatically. Saving settings requires a service restart to take effect and preserves queued jobs with their original configuration, also across restarts. Until restart, monitoring and new uploads use the active startup configuration. An active print job continues. Wait until busy is false before saving configuration or starting"}),
    ))
}

fn multipart_error(error: MultipartError) -> ApiError {
    ApiError(error.status(), error.body_text())
}

fn upload_name(name: &str) -> Result<&str, ApiError> {
    // Browsers can send Windows fake paths or arbitrary client paths. Only keep
    // the leaf as display text; the disk name is always generated by the server.
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or("");
    if leaf.is_empty() || leaf.len() > 255 || leaf.chars().any(char::is_control) {
        return Err(invalid("Invalid upload file name"));
    }
    if file_kind(std::path::Path::new(leaf)).is_none() {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Only PDF, DOC, DOCX, PNG, JPG and JPEG files are supported".into(),
        ));
    }
    Ok(leaf)
}

async fn upload(
    State(state): State<Arc<AppState>>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let mut multipart = multipart.map_err(|error| invalid(error.body_text()))?;
    let config = state.active_config.as_ref().ok_or_else(|| {
        ApiError(
            StatusCode::CONFLICT,
            "Save a printer configuration and restart the service before uploading".into(),
        )
    })?;
    let mut field = multipart
        .next_field()
        .await
        .map_err(multipart_error)?
        .ok_or_else(|| invalid("One file field is required"))?;
    if field.name() != Some("file") {
        return Err(invalid("Expected one multipart field named file"));
    }
    let name = upload_name(
        field
            .file_name()
            .ok_or_else(|| invalid("Missing file name"))?,
    )?
    .to_owned();
    let extension = std::path::Path::new(&name)
        .extension()
        .unwrap()
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    let temp = tempfile::Builder::new()
        .prefix("upload-")
        .suffix(&format!(".{extension}"))
        .tempfile_in(&state.spool)?;
    let mut file = tokio::fs::File::from_std(temp.reopen()?);
    let mut size = 0;
    let mut hash = Sha256::new();
    while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
        size += chunk.len();
        if size > MAX_UPLOAD_BYTES {
            return Err(ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "File exceeds the 50 MiB upload limit".into(),
            ));
        }
        hash.update(&chunk);
        file.write_all(&chunk).await?;
    }
    if size == 0 {
        return Err(invalid("Empty files cannot be printed"));
    }
    drop(field);
    if multipart
        .next_field()
        .await
        .map_err(multipart_error)?
        .is_some()
    {
        return Err(invalid("Upload exactly one file per request"));
    }
    file.sync_all().await?;
    drop(file);
    tokio::fs::File::open(&state.spool)
        .await?
        .sync_all()
        .await?;
    // The startup configuration remains fixed throughout the upload, even if
    // settings are saved while receiving it. No lock spans request/file I/O.
    let _control = state.control.lock().unwrap();
    let store = state.store.lock().unwrap();
    let (_, path) = temp.keep().map_err(|error| anyhow::anyhow!(error))?;
    let queued = store.enqueue_upload(&name, &format!("{:x}", hash.finalize()), &path, config);
    let job = match queued {
        Ok(job) => job,
        Err(error) => {
            let _ = std::fs::remove_file(path);
            return Err(error.into());
        }
    };
    tracing::info!(job=job.id, file=%name, "queued uploaded file");
    Ok((StatusCode::ACCEPTED, Json(json!({"job":job}))))
}
#[derive(Deserialize)]
struct Page {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
}
fn default_limit() -> u32 {
    50
}
async fn history(State(state): State<Arc<AppState>>, Query(page): Query<Page>) -> ApiResult {
    let jobs = state
        .store
        .lock()
        .unwrap()
        .history(page.limit.clamp(1, 200), page.offset)?;
    Ok(Json(json!({"jobs":jobs})))
}
async fn weekly_stats(State(state): State<Arc<AppState>>) -> ApiResult {
    Ok(Json(json!(state.store.lock().unwrap().weekly_stats()?)))
}
async fn retry(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult {
    if query.get("confirm").map(String::as_str) != Some("checked_queue") {
        return Err(invalid(
            "Check the CUPS queue first; retry can duplicate an already accepted job. Use confirm=checked_queue",
        ));
    }
    state
        .store
        .lock()
        .unwrap()
        .retry(id)
        .map_err(|e| ApiError(StatusCode::CONFLICT, e.to_string()))?;
    Ok(Json(json!({"queued":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[tokio::test]
    async fn home_settings_and_weekly_api_are_separate_and_protected() {
        let dir = tempfile::tempdir().unwrap();
        let app = router(AppState::open(dir.path(), vec!["localhost:8787".into()]).unwrap());
        for (path, required, absent) in [
            ("/", "id=\"weekly-chart\"", "id=\"config\""),
            ("/settings", "id=\"config\"", "id=\"upload\""),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("host", "localhost:8787")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let html = std::str::from_utf8(&body).unwrap();
            assert!(html.contains(required));
            assert!(!html.contains(absent));
            assert!(html.contains("href=\"/\"") && html.contains("href=\"/settings\""));
            if path == "/" {
                assert!(!html.contains("id=\"status\"") && !html.contains("id=\"start\""));
            }
        }
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/stats/weekly")
                    .header("host", "localhost:8787")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let stats: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(stats["metric"], "submitted_to_cups");
        assert_eq!(stats["timezone"], "service_local");
        assert_eq!(stats["days"].as_array().unwrap().len(), 7);
        for day in stats["days"].as_array().unwrap() {
            assert_eq!(day["submitted_files"], 0);
            assert_eq!(day["pages"], 0);
        }
        for path in ["/settings", "/settings.js", "/api/stats/weekly"] {
            assert_eq!(
                app.clone()
                    .oneshot(
                        Request::builder()
                            .uri(path)
                            .header("host", "evil.test")
                            .body(Body::empty())
                            .unwrap()
                    )
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
    }
    #[test]
    fn upload_names_are_display_only_and_extensions_are_exact() {
        for (name, expected) in [
            ("../../report.PDF", "report.PDF"),
            (r"C:\fakepath\letter.docx", "letter.docx"),
            ("<script>.doc", "<script>.doc"),
        ] {
            assert_eq!(upload_name(name).unwrap(), expected);
        }
        for name in ["", "../", "..", "x.pdf.exe", "x.docm", "x\n.pdf"] {
            assert!(upload_name(name).is_err());
        }
        assert!(upload_name(&format!("{}.pdf", "x".repeat(252))).is_err());
    }

    #[tokio::test]
    async fn host_and_csrf_guards_protect_local_admin() {
        let dir = tempfile::tempdir().unwrap();
        let app = router(AppState::open(dir.path(), vec!["127.0.0.1:8787".into()]).unwrap());
        let req = |method: &str, path: &str, host: &str, csrf: bool| {
            let mut req = Request::builder()
                .method(method)
                .uri(path)
                .header("host", host);
            if csrf {
                req = req.header("x-lanprint-request", "1");
            }
            req.body(Body::empty()).unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(req("GET", "/api/status", "evil.example:8787", false))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            app.clone()
                .oneshot(req("POST", "/api/monitor/pause", "127.0.0.1:8787", false))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            app.clone()
                .oneshot(req("POST", "/api/monitor/pause", "127.0.0.1:8787", true))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.oneshot(req("POST", "/api/monitor/start", "127.0.0.1:8787", true))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn lan_host_keeps_host_origin_and_csrf_guards() {
        let dir = tempfile::tempdir().unwrap();
        let host = "192.168.1.20:8787";
        let app = router(AppState::open(dir.path(), vec![host.into()]).unwrap());
        for (request_host, expected) in [
            (Some(host), StatusCode::OK),
            (None, StatusCode::FORBIDDEN),
            (Some("192.168.1.21:8787"), StatusCode::FORBIDDEN),
            (Some("192.168.1.20:8788"), StatusCode::FORBIDDEN),
            (Some("192.168.1.20"), StatusCode::FORBIDDEN),
            (
                Some("192.168.1.20.evil.example:8787"),
                StatusCode::FORBIDDEN,
            ),
            (Some("evil.example:8787"), StatusCode::FORBIDDEN),
        ] {
            for path in ["/", "/api/status"] {
                let mut request = Request::builder().uri(path);
                if let Some(host) = request_host {
                    request = request.header(header::HOST, host);
                }
                assert_eq!(
                    app.clone()
                        .oneshot(request.body(Body::empty()).unwrap())
                        .await
                        .unwrap()
                        .status(),
                    expected,
                    "Host {request_host:?}, path {path}"
                );
            }
        }
        for (csrf, origin, expected) in [
            (
                false,
                Some("http://192.168.1.20:8787"),
                StatusCode::FORBIDDEN,
            ),
            (true, None, StatusCode::OK),
            (true, Some("http://192.168.1.20:8787"), StatusCode::OK),
            (
                true,
                Some("http://192.168.1.21:8787"),
                StatusCode::FORBIDDEN,
            ),
            (
                true,
                Some("http://192.168.1.20:8788"),
                StatusCode::FORBIDDEN,
            ),
            (
                true,
                Some("http://evil.example:8787"),
                StatusCode::FORBIDDEN,
            ),
            (
                true,
                Some("https://192.168.1.20:8787"),
                StatusCode::FORBIDDEN,
            ),
            (true, Some("null"), StatusCode::FORBIDDEN),
        ] {
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/monitor/pause")
                .header(header::HOST, host);
            if csrf {
                request = request.header("x-lanprint-request", "1");
            }
            if let Some(origin) = origin {
                request = request.header(header::ORIGIN, origin);
            }
            assert_eq!(
                app.clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                expected,
                "CSRF {csrf}, Origin {origin:?}"
            );
        }
    }
}
