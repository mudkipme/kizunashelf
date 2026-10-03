//! Shared provider HTTP policy: connection pooling, pacing, bounded retries,
//! token acquisition, and response errors. Provider adapters and import sources
//! use this transport so network policy does not depend on the calling workflow.

use crate::api::error::ApiError;
use crate::api::state::{base_http_client, AppState, CachedAccessToken};
use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const EXTERNAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared, connection-pooled HTTP client for outbound provider requests. Built
/// once on first use so repeated searches reuse keep-alive connections instead
/// of paying a fresh TLS handshake per request.
pub(in crate::api) fn external_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        base_http_client()
            .timeout(EXTERNAL_REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// Spacing floor between consecutive requests to one provider host. Most of the
/// APIs we call throttle around one request per second (Jikan, MusicBrainz,
/// AniList, MAL), so that's the default; [`host_spacing`] adjusts hosts with
/// documented different allowances.
const DEFAULT_HOST_SPACING: Duration = Duration::from_secs(1);

/// Ceiling on how long a `Retry-After` may hold a request; a longer ask gives up
/// and surfaces the `429` instead of hanging the caller.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

/// Retries after a `429` before surfacing it (the initial attempt not counted).
const RATE_LIMIT_RETRIES: u32 = 2;

fn host_spacing(host: &str) -> Duration {
    match host {
        // TMDB allows ~50 req/s, IGDB 4 req/s, and Bangumi defaults to 3000
        // requests per 10 minutes (5 req/s); the 1 s default would make their
        // per-season/episode pagination needlessly slow.
        "api.themoviedb.org" | "api.igdb.com" | "api.bgm.tv" => Duration::from_millis(250),
        // The iTunes search/lookup API is documented at roughly 20 calls/minute.
        "itunes.apple.com" => Duration::from_secs(3),
        _ => DEFAULT_HOST_SPACING,
    }
}

type HostSlot = Arc<tokio::sync::Mutex<tokio::time::Instant>>;

/// Per-host "earliest next request" slots for [`send_limited`]. The slot mutex is
/// held across the pacing sleep, so concurrent requests to one host queue behind
/// each other instead of racing through the same gap.
fn host_slots() -> &'static tokio::sync::Mutex<HashMap<String, HostSlot>> {
    static SLOTS: OnceLock<tokio::sync::Mutex<HashMap<String, HostSlot>>> = OnceLock::new();
    SLOTS.get_or_init(Default::default)
}

async fn host_slot(host: &str) -> HostSlot {
    let mut slots = host_slots().lock().await;
    Arc::clone(
        slots
            .entry(host.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now()))),
    )
}

/// Waits for `host`'s pacing slot, then claims the next one.
async fn pace_host(host: &str) {
    let slot = host_slot(host).await;
    let mut next = slot.lock().await;
    tokio::time::sleep_until(*next).await;
    *next = tokio::time::Instant::now() + host_spacing(host);
}

/// Pushes `host`'s next slot out by at least `delay`, so after a `429` every
/// queued request to the host holds off, not just the one being retried.
async fn penalize_host(host: &str, delay: Duration) {
    let slot = host_slot(host).await;
    let mut next = slot.lock().await;
    *next = (*next).max(tokio::time::Instant::now() + delay);
}

/// The wait a `Retry-After` header value asks for: delta-seconds or an HTTP-date
/// (a past date clamps to zero). `None` when unparseable.
fn parse_retry_after(raw: &str) -> Option<Duration> {
    let raw = raw.trim();
    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = httpdate::parse_http_date(raw).ok()?;
    Some(
        date.duration_since(std::time::SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

fn retry_after_delay(response: &reqwest::Response) -> Option<Duration> {
    let raw = response.headers().get(reqwest::header::RETRY_AFTER)?;
    parse_retry_after(raw.to_str().ok()?)
}

/// Sends a provider request through the shared per-host rate limiter: waits for
/// the host's pacing slot, and on a `429` honors `Retry-After` (bounded retries,
/// capped waits) before surfacing the response to the caller. Every outbound
/// provider/import request goes through here — interactive searches pay nothing
/// (their first request never waits), while bursts like episode pagination and
/// import loops are spaced under provider limits. A request whose body can't be
/// cloned is still paced but a `429` returns as-is.
pub(in crate::api) async fn send_limited(
    builder: reqwest::RequestBuilder,
) -> reqwest::Result<reqwest::Response> {
    let host = builder
        .try_clone()
        .and_then(|clone| clone.build().ok())
        .and_then(|request| request.url().host_str().map(str::to_string));
    let mut attempt = 0;
    loop {
        if let Some(host) = host.as_deref() {
            pace_host(host).await;
        }
        let retryable = attempt < RATE_LIMIT_RETRIES;
        let Some(current) = retryable.then(|| builder.try_clone()).flatten() else {
            return builder.send().await;
        };
        let response = current.send().await?;
        if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Ok(response);
        }
        let delay = retry_after_delay(&response)
            .unwrap_or_else(|| Duration::from_secs(2 * u64::from(attempt + 1)));
        if delay > MAX_RETRY_AFTER {
            return Ok(response);
        }
        match host.as_deref() {
            Some(host) => penalize_host(host, delay).await,
            None => tokio::time::sleep(delay).await,
        }
        attempt += 1;
    }
}

/// Single-flighted cached-token acquisition shared by the OAuth/login providers
/// (IGDB, TheTVDB). Returns the cached access token when still fresh;
/// otherwise takes the per-provider lock, re-checks the cache, and on a miss runs
/// `fetch` to mint a token, stores it, and returns its access token. `force_refresh`
/// skips the first cache check (used after a 401); `label` names the provider in
/// the cache-failure error. This owns the cache/lock/store pattern that each token
/// function otherwise repeated verbatim.
pub(in crate::api) async fn cached_or_fetch_token<F, Fut>(
    state: &AppState,
    key: &str,
    label: &str,
    force_refresh: bool,
    fetch: F,
) -> Result<String, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<CachedAccessToken, ApiError>>,
{
    if !force_refresh {
        if let Some(token) = state.cached_access_token(key).await {
            return Ok(token.access_token);
        }
    }
    // Single-flight the fetch: under concurrent searches a cold cache would
    // otherwise stampede the provider's token endpoint and risk rate limits.
    let fetch_lock = state.token_fetch_lock(key).await;
    let _guard = fetch_lock.lock().await;
    // Another task may have populated the cache while we waited for the lock.
    if let Some(token) = state.cached_access_token(key).await {
        return Ok(token.access_token);
    }
    let token = fetch().await?;
    let access_token = token.access_token.clone();
    state
        .store_access_token(key, token)
        .await
        .map_err(|error| {
            ApiError::bad_request(&format!("failed to cache {label} token: {error}"))
        })?;
    Ok(access_token)
}

/// Sends a bearer-authenticated request, refreshing the token once on a `401`.
/// `build` produces the request for a given access token (so the retry
/// re-authorizes with the fresh token); `refresh` mints a new token after the
/// cached one is invalidated. Returns the raw response (the caller checks status).
pub(in crate::api) async fn send_with_token_retry<B, R, RFut>(
    state: &AppState,
    key: &str,
    token: &str,
    build: B,
    refresh: R,
) -> Result<reqwest::Response, ApiError>
where
    B: Fn(&str) -> reqwest::RequestBuilder,
    R: FnOnce() -> RFut,
    RFut: Future<Output = Result<String, ApiError>>,
{
    let response = send_limited(build(token)).await.map_err(provider_error)?;
    if response.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(response);
    }
    state.invalidate_access_token(key).await;
    let token = refresh().await?;
    send_limited(build(&token)).await.map_err(provider_error)
}

pub(super) fn provider_error(error: reqwest::Error) -> ApiError {
    // The full source chain can include transport/TLS/DNS internals and request
    // URLs (which may carry credentials), so it is logged server-side only and
    // never returned to the client.
    let mut detail = format!("External provider request failed: {error}");
    let mut source = error.source();
    while let Some(inner) = source {
        detail.push_str(&format!(": {inner}"));
        source = inner.source();
    }
    tracing::warn!(detail, "external provider request failed");
    // Upstream failures are not the caller's fault: surface them as gateway
    // errors so clients can distinguish a flaky provider from a bad request.
    if error.is_timeout() {
        ApiError::gateway_timeout("The external provider timed out")
    } else {
        ApiError::bad_gateway("The external provider request failed")
    }
}

/// The most body we fold into the error message. Provider error payloads are
/// usually a short JSON object (`{"status_message":"Invalid API key"}`); this
/// caps a runaway HTML error page from bloating a toast.
const MAX_ERROR_BODY: usize = 500;

/// Builds a gateway error carrying the exact upstream HTTP status and a
/// truncated copy of the provider's response body, so the concrete reason
/// (`401 Unauthorized`, a rate-limit message, an "invalid api key" payload)
/// reaches the user instead of a generic "request failed". Unlike the request
/// URL, the response body is the provider's own error text and safe to surface.
fn provider_status_error(status: reqwest::StatusCode, body: &str) -> ApiError {
    let mut message = match status.canonical_reason() {
        Some(reason) => format!(
            "The external provider returned HTTP {} {reason}",
            status.as_u16()
        ),
        None => format!("The external provider returned HTTP {}", status.as_u16()),
    };
    let body = body.trim();
    if !body.is_empty() {
        let snippet: String = body.chars().take(MAX_ERROR_BODY).collect();
        message.push_str(": ");
        message.push_str(&snippet);
        if body.chars().count() > MAX_ERROR_BODY {
            message.push('…');
        }
    }
    tracing::warn!(
        status = status.as_u16(),
        message,
        "external provider returned an error"
    );
    ApiError::bad_gateway(&message)
}

/// Extension on [`reqwest::Response`] that, unlike
/// [`reqwest::Response::error_for_status`], reads the response body on a
/// non-success status so the concrete status + body can be surfaced to the user
/// (see [`provider_status_error`]). Use this in provider request paths in place
/// of `error_for_status().map_err(provider_error)`.
pub(in crate::api) trait ProviderResponseExt: Sized {
    async fn error_for_status_body(self) -> Result<reqwest::Response, ApiError>;
}

impl ProviderResponseExt for reqwest::Response {
    async fn error_for_status_body(self) -> Result<reqwest::Response, ApiError> {
        let status = self.status();
        if status.is_success() {
            return Ok(self);
        }
        // Read the body before discarding the response; `error_for_status`
        // would drop it, losing the provider's own explanation.
        let body = self.text().await.unwrap_or_default();
        Err(provider_status_error(status, &body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_retry_after_forms() {
        assert_eq!(parse_retry_after("3"), Some(Duration::from_secs(3)));
        assert_eq!(parse_retry_after(" 10 "), Some(Duration::from_secs(10)));
        assert_eq!(parse_retry_after("soon"), None);
        assert_eq!(parse_retry_after(""), None);

        let future = std::time::SystemTime::now() + Duration::from_secs(300);
        let delay =
            parse_retry_after(&httpdate::fmt_http_date(future)).expect("http-date should parse");
        assert!(delay > Duration::from_secs(290) && delay <= Duration::from_secs(300));

        let past = std::time::SystemTime::now() - Duration::from_secs(300);
        assert_eq!(
            parse_retry_after(&httpdate::fmt_http_date(past)),
            Some(Duration::ZERO)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn pacing_spaces_requests_to_one_host() {
        // Hosts here are unique to this test: the slot map is process-global.
        let start = tokio::time::Instant::now();
        pace_host("pace-test-a.invalid").await;
        assert_eq!(tokio::time::Instant::now(), start, "first request is free");
        pace_host("pace-test-a.invalid").await;
        assert!(tokio::time::Instant::now() - start >= DEFAULT_HOST_SPACING);
        // A different host is paced independently.
        let before = tokio::time::Instant::now();
        pace_host("pace-test-b.invalid").await;
        assert_eq!(tokio::time::Instant::now(), before);
    }

    #[tokio::test(start_paused = true)]
    async fn penalize_pushes_the_next_slot_out() {
        let start = tokio::time::Instant::now();
        pace_host("pace-test-penalty.invalid").await;
        penalize_host("pace-test-penalty.invalid", Duration::from_secs(30)).await;
        pace_host("pace-test-penalty.invalid").await;
        assert!(tokio::time::Instant::now() - start >= Duration::from_secs(30));
    }
}
