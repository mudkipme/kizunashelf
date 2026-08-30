//! Optional single-user authentication for the network-facing web runtime.
//!
//! This wraps the already-built router instead of becoming part of it. Desktop
//! and iOS therefore keep driving the shared router directly, and these host
//! routes do not become part of the generated OpenAPI contract.

use argon2::password_hash::phc::PasswordHash;
use argon2::{Argon2, PasswordVerifier};
use axum::body::Body;
use axum::extract::{ConnectInfo, Form, FromRequestParts, Request, State};
use axum::http::header::{
    ACCEPT, CACHE_CONTROL, CONTENT_SECURITY_POLICY, COOKIE, REFERRER_POLICY, RETRY_AFTER,
    SET_COOKIE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use getrandom::fill;
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

const COOKIE_NAME: &str = "__Host-kizunashelf";
const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
const MAX_LOGIN_ATTEMPTS: usize = 5;
const MAX_PASSWORD_BYTES: usize = 1024;
/// How many client buckets the throttle will hold at once.
///
/// Entries expire with the window, so this only binds under a source-rotating
/// flood. When it is reached the least recently seen bucket is dropped, which
/// can only ever *relax* the throttle for whoever owned it — never lock anyone
/// out. That is the safe direction: an attacker who can rotate addresses is
/// already past an address-keyed throttle, while the owner must never be.
const MAX_TRACKED_CLIENTS: usize = 4096;
/// How many password verifications may run at once.
///
/// Argon2id is memory-hard on purpose — roughly 19 MiB and a core per check.
/// The throttle above caps a *single* client's rate but no longer caps the
/// total, so without this a spread-out attacker could turn the login form into
/// a memory-exhaustion lever. Waiting for a permit costs a real login
/// milliseconds and never becomes a lockout.
const MAX_CONCURRENT_VERIFICATIONS: usize = 4;

#[derive(Clone)]
pub struct WebAuth {
    inner: Arc<WebAuthInner>,
}

struct WebAuthInner {
    password_hash: String,
    sessions: Mutex<HashMap<String, Instant>>,
    /// Recent attempts per client. Keyed by [`ClientAddr`], whose `None` is the
    /// shared bucket for requests whose origin could not be established.
    login_attempts: Mutex<HashMap<ClientAddr, VecDeque<Instant>>>,
    verifications: Semaphore,
}

impl WebAuth {
    pub fn new(password_hash: String) -> anyhow::Result<Self> {
        let parsed = PasswordHash::new(password_hash.trim())
            .map_err(|error| anyhow::anyhow!("invalid KIZUNASHELF_AUTH_PASSWORD_HASH: {error}"))?;
        if parsed.algorithm.as_str() != "argon2id" {
            anyhow::bail!("KIZUNASHELF_AUTH_PASSWORD_HASH must use Argon2id");
        }

        Ok(Self {
            inner: Arc::new(WebAuthInner {
                password_hash: password_hash.trim().to_string(),
                sessions: Mutex::new(HashMap::new()),
                login_attempts: Mutex::new(HashMap::new()),
                verifications: Semaphore::new(MAX_CONCURRENT_VERIFICATIONS),
            }),
        })
    }

    /// Charges one login attempt to `client`, or reports that its allowance for
    /// the current window is spent.
    ///
    /// Per client, not per server: a single-password deployment has exactly one
    /// legitimate user, so a shared counter let anyone who could reach the login
    /// form lock that user out for a minute at a time by guessing badly on
    /// purpose.
    fn reserve_login_attempt(&self, client: ClientAddr) -> bool {
        let now = Instant::now();
        let mut clients = self
            .inner
            .login_attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Sweep every bucket, not just this client's: expiry is what keeps the
        // map proportional to "clients that tried in the last minute" rather
        // than to every address ever seen.
        clients.retain(|_, attempts| {
            while attempts
                .front()
                .is_some_and(|attempt| now.duration_since(*attempt) >= LOGIN_WINDOW)
            {
                attempts.pop_front();
            }
            !attempts.is_empty()
        });

        let attempts = clients.entry(client).or_default();
        if attempts.len() >= MAX_LOGIN_ATTEMPTS {
            return false;
        }
        attempts.push_back(now);

        if clients.len() > MAX_TRACKED_CLIENTS {
            evict_least_recent(&mut clients, client);
        }
        true
    }

    fn clear_login_attempts(&self, client: ClientAddr) {
        self.inner
            .login_attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&client);
    }

    async fn verify(&self, password: String) -> bool {
        if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
            return false;
        }
        // See `MAX_CONCURRENT_VERIFICATIONS`. A closed semaphore is not
        // reachable (nothing closes it); were it ever to be, running unbounded
        // is the safer failure than refusing every login.
        let _permit = self.inner.verifications.acquire().await.ok();
        let password_hash = self.inner.password_hash.clone();
        tokio::task::spawn_blocking(move || {
            PasswordHash::new(&password_hash).is_ok_and(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
        })
        .await
        .unwrap_or(false)
    }

    fn create_session(&self) -> String {
        let mut bytes = [0_u8; 32];
        fill(&mut bytes).expect("OS RNG unavailable");
        let token = URL_SAFE_NO_PAD.encode(bytes);
        self.inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(token.clone(), Instant::now() + SESSION_LIFETIME);
        token
    }

    fn is_authenticated(&self, headers: &HeaderMap) -> bool {
        let Some(token) = cookie_value(headers, COOKIE_NAME) else {
            return false;
        };
        let now = Instant::now();
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sessions.retain(|_, expires_at| *expires_at > now);
        sessions.contains_key(token)
    }

    fn remove_session(&self, headers: &HeaderMap) {
        if let Some(token) = cookie_value(headers, COOKIE_NAME) {
            self.inner
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(token);
        }
    }
}

/// Wrap the network server's router with opt-in authentication routes and a
/// default-deny middleware. Native runtimes never call this function.
pub fn protect_web_router(router: Router, auth: WebAuth) -> Router {
    let protected = router.layer(middleware::from_fn_with_state(
        auth.clone(),
        require_authentication,
    ));
    let auth_routes = Router::new()
        .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
        .route("/_auth/login", get(login_page).post(login))
        .route("/_auth/logout", get(logout_page).post(logout))
        .with_state(auth);
    auth_routes.merge(protected)
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

async fn login_page(State(auth): State<WebAuth>, headers: HeaderMap) -> Response {
    if auth.is_authenticated(&headers) {
        return Redirect::to("/").into_response();
    }
    login_html(StatusCode::OK, None)
}

async fn login(
    State(auth): State<WebAuth>,
    client: ClientAddr,
    Form(form): Form<LoginForm>,
) -> Response {
    if !auth.reserve_login_attempt(client) {
        let mut response = login_html(
            StatusCode::TOO_MANY_REQUESTS,
            Some("Too many attempts. Try again in a minute."),
        );
        response
            .headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("60"));
        return response;
    }
    if !auth.verify(form.password).await {
        return login_html(StatusCode::UNAUTHORIZED, Some("The password is incorrect."));
    }

    auth.clear_login_attempts(client);
    let token = auth.create_session();
    let mut response = Redirect::to("/").into_response();
    let Ok(cookie) = HeaderValue::from_str(&session_cookie(&token)) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create an authentication session",
        )
            .into_response();
    };
    response.headers_mut().insert(SET_COOKIE, cookie);
    no_store(&mut response);
    response
}

async fn logout(State(auth): State<WebAuth>, headers: HeaderMap) -> Response {
    auth.remove_session(&headers);
    let mut response = Redirect::to("/_auth/login").into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_static(
            "__Host-kizunashelf=; Path=/; Max-Age=0; Secure; HttpOnly; SameSite=Strict",
        ),
    );
    no_store(&mut response);
    response
}

async fn logout_page(State(auth): State<WebAuth>, headers: HeaderMap) -> Response {
    if !auth.is_authenticated(&headers) {
        return Redirect::to("/_auth/login").into_response();
    }
    let body = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Sign out — KizunaShelf</title>
</head>
<body>
  <main>
    <h1>Sign out of KizunaShelf?</h1>
    <form method="post" action="/_auth/logout"><button type="submit">Sign out</button></form>
    <p><a href="/">Return to KizunaShelf</a></p>
  </main>
</body>
</html>"#;
    secure_html(StatusCode::OK, body.to_string())
}

async fn require_authentication(
    State(auth): State<WebAuth>,
    request: Request,
    next: Next,
) -> Response {
    if auth.is_authenticated(request.headers()) {
        let sensitive = request.uri().path() == "/api" || request.uri().path().starts_with("/api/");
        let mut response = next.run(request).await;
        if sensitive {
            no_store(&mut response);
        }
        return response;
    }

    if matches!(request.method(), &Method::GET | &Method::HEAD) && accepts_html(request.headers()) {
        let mut response = Redirect::to("/_auth/login").into_response();
        no_store(&mut response);
        return response;
    }

    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "Authentication required" })),
    )
        .into_response();
    no_store(&mut response);
    response
}

/// The address a login attempt is charged to.
///
/// `None` means the origin could not be established — no peer address was
/// recorded, because the host served the router without
/// `into_make_service_with_connect_info`. Those requests share one bucket,
/// which is the pre-existing global behavior and the only safe fallback: the
/// alternative is an unthrottled login form.
///
/// Extraction cannot fail, so a misconfigured host degrades to that shared
/// bucket instead of turning every login into a 500.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
struct ClientAddr(Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientAddr {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| address.ip());
        Ok(Self(client_ip(peer, &parts.headers)))
    }
}

/// Resolves the client an attempt belongs to, from the socket peer and the
/// headers a proxy may have added.
///
/// The documented deployment puts KizunaShelf behind a reverse proxy on
/// loopback or a private network, so the peer address alone would be the same
/// for every visitor and the throttle would still be effectively global. The
/// forwarded headers are therefore honored — but only when the connection
/// itself came from inside that boundary. A request arriving straight from a
/// public address is charged to that address no matter what it claims, so the
/// headers cannot be used to shed identity and out-run the throttle.
fn client_ip(peer: Option<IpAddr>, headers: &HeaderMap) -> Option<IpAddr> {
    let peer = peer?;
    let client = if is_inside_deployment(peer) {
        forwarded_client(headers).unwrap_or(peer)
    } else {
        peer
    };
    Some(throttle_bucket(client))
}

/// The client address reported by a proxy we have decided to believe.
fn forwarded_client(headers: &HeaderMap) -> Option<IpAddr> {
    if let Some(chain) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    {
        // Right to left: the rightmost entry was appended by the nearest proxy
        // and is the least forgeable, since anything a client sends arrives to
        // its left. Entries that are themselves inside the deployment are
        // further proxies in the chain, so keep walking past them.
        let mut innermost = None;
        for entry in chain.rsplit(',') {
            let Some(address) = parse_forwarded_ip(entry) else {
                break;
            };
            if !is_inside_deployment(address) {
                return Some(address);
            }
            innermost = Some(address);
        }
        // An entirely private chain is a LAN-only deployment; the leftmost
        // entry still tells LAN clients apart, which is the whole point.
        if innermost.is_some() {
            return innermost;
        }
    }
    // nginx's `proxy_set_header X-Real-IP` is common enough that ignoring it
    // would silently leave those deployments on one shared bucket.
    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_forwarded_ip)
}

fn parse_forwarded_ip(value: &str) -> Option<IpAddr> {
    let value = value.trim();
    if let Ok(address) = value.parse::<IpAddr>() {
        return Some(address);
    }
    // Some proxies append the source port: `203.0.113.7:54321`, `[2001:db8::1]:443`.
    if let Ok(address) = value.parse::<SocketAddr>() {
        return Some(address.ip());
    }
    value
        .strip_prefix('[')?
        .split(']')
        .next()?
        .parse::<IpAddr>()
        .ok()
}

/// Whether an address can only belong to something already inside the
/// deployment boundary — the reverse proxy, a container network, a LAN or
/// tailnet peer. Reaching the app from one of these already implies more access
/// than spoofing a header would grant.
fn is_inside_deployment(address: IpAddr) -> bool {
    match normalize_mapped(address) {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                // 100.64.0.0/10, carrier-grade NAT — also what Tailscale hands
                // out, which is a common way to reach a self-hosted app.
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
        }
        IpAddr::V6(v6) => {
            let leading = v6.segments()[0];
            // `is_unique_local` and `is_unicast_link_local` are still unstable,
            // so the prefixes are matched directly: fc00::/7 and fe80::/10.
            v6.is_loopback() || (leading & 0xfe00) == 0xfc00 || (leading & 0xffc0) == 0xfe80
        }
    }
}

/// The address collapsed to the unit a throttle should count.
///
/// A single machine is routinely handed an entire IPv6 /64, so counting full
/// addresses would let one host present a fresh identity for every attempt.
fn throttle_bucket(address: IpAddr) -> IpAddr {
    match normalize_mapped(address) {
        IpAddr::V4(v4) => IpAddr::V4(v4),
        IpAddr::V6(v6) => {
            let [a, b, c, d, ..] = v6.segments();
            IpAddr::V6(Ipv6Addr::new(a, b, c, d, 0, 0, 0, 0))
        }
    }
}

/// A dual-stack listener reports IPv4 peers as `::ffff:a.b.c.d`; classify and
/// bucket those as the IPv4 addresses they are.
fn normalize_mapped(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(address, IpAddr::V4),
        other => other,
    }
}

/// Drops the bucket whose most recent attempt is oldest, never `keep`.
///
/// Only ever relaxes the throttle for the dropped client, so it cannot be used
/// to lock anyone out — and resetting a bucket this way costs an attacker more
/// than simply waiting out the window.
fn evict_least_recent(clients: &mut HashMap<ClientAddr, VecDeque<Instant>>, keep: ClientAddr) {
    let victim = clients
        .iter()
        .filter(|(client, _)| **client != keep)
        .min_by_key(|(_, attempts)| attempts.back().copied())
        .map(|(client, _)| *client);
    if let Some(victim) = victim {
        clients.remove(&victim);
    }
}

fn accepts_html(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().starts_with("text/html"))
        })
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(cookie_name, value)| (cookie_name == name).then_some(value))
}

fn session_cookie(token: &str) -> String {
    format!(
        "{COOKIE_NAME}={token}; Path=/; Max-Age={}; Secure; HttpOnly; SameSite=Strict",
        SESSION_LIFETIME.as_secs()
    )
}

fn login_html(status: StatusCode, error: Option<&str>) -> Response {
    let error = error
        .map(|message| format!(r#"<p class="error" role="alert">{message}</p>"#))
        .unwrap_or_default();
    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Sign in — KizunaShelf</title>
  <style>
    :root {{ color-scheme: light dark; font-family: ui-sans-serif, system-ui, sans-serif; }}
    body {{ min-height: 100vh; margin: 0; display: grid; place-items: center; background: #f5f5f4; color: #1c1917; }}
    main {{ width: min(22rem, calc(100vw - 3rem)); padding: 2rem; border: 1px solid #d6d3d1; border-radius: 1rem; background: #fff; box-shadow: 0 1rem 3rem rgb(28 25 23 / 8%); }}
    h1 {{ margin: 0 0 .35rem; font-size: 1.5rem; }}
    p {{ margin: 0 0 1.5rem; color: #57534e; }}
    label {{ display: block; margin-bottom: .5rem; font-size: .875rem; font-weight: 600; }}
    input, button {{ box-sizing: border-box; width: 100%; min-height: 2.75rem; border-radius: .55rem; font: inherit; }}
    input {{ padding: .65rem .75rem; border: 1px solid #a8a29e; background: transparent; color: inherit; }}
    button {{ margin-top: 1rem; border: 0; background: #292524; color: #fff; font-weight: 600; cursor: pointer; }}
    .error {{ margin: 1rem 0 0; color: #b91c1c; font-size: .875rem; }}
    @media (prefers-color-scheme: dark) {{
      body {{ background: #0c0a09; color: #fafaf9; }}
      main {{ border-color: #44403c; background: #1c1917; }}
      p {{ color: #a8a29e; }}
      input {{ border-color: #57534e; }}
      button {{ background: #f5f5f4; color: #1c1917; }}
      .error {{ color: #fca5a5; }}
    }}
  </style>
</head>
<body>
  <main>
    <h1>KizunaShelf</h1>
    <p>Enter your password to continue.</p>
    <form method="post" action="/_auth/login">
      <label for="password">Password</label>
      <input id="password" name="password" type="password" autocomplete="current-password" maxlength="1024" required autofocus>
      <button type="submit">Sign in</button>
    </form>
    {error}
  </main>
</body>
</html>"#
    );
    secure_html(status, body)
}

fn secure_html(status: StatusCode, body: String) -> Response {
    let mut response = (status, Html(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    no_store(&mut response);
    response
}

fn no_store(response: &mut Response<Body>) {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::PasswordHasher;
    use axum::body::to_bytes;
    use axum::http::header::{CONTENT_TYPE, LOCATION};
    use axum::http::Request as HttpRequest;
    use axum::routing::get;
    use tower::ServiceExt;

    fn test_auth() -> WebAuth {
        let hash = Argon2::default()
            .hash_password_with_salt(b"correct horse battery staple", b"test salt for auth")
            .unwrap()
            .to_string();
        WebAuth::new(hash).unwrap()
    }

    fn app() -> Router {
        protect_web_router(
            Router::new()
                .route("/secret", get(|| async { "secret" }))
                .route("/api/secret", get(|| async { "secret" })),
            test_auth(),
        )
    }

    #[test]
    fn rejects_missing_or_non_argon2id_hashes() {
        assert!(WebAuth::new(String::new()).is_err());
        assert!(WebAuth::new(
            "$argon2i$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg".to_string()
        )
        .is_err());
    }

    #[tokio::test]
    async fn redirects_unauthenticated_document_requests() {
        let response = app()
            .oneshot(
                HttpRequest::builder()
                    .uri("/secret")
                    .header(ACCEPT, "text/html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get(LOCATION).unwrap(), "/_auth/login");
    }

    #[tokio::test]
    async fn rejects_unauthenticated_api_requests() {
        let response = app()
            .oneshot(HttpRequest::get("/secret").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    }

    #[tokio::test]
    async fn exposes_only_the_minimal_health_check_without_authentication() {
        let response = app()
            .oneshot(HttpRequest::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(to_bytes(response.into_body(), 1).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn login_cookie_authenticates_and_logout_invalidates_it() {
        let app = app();
        let login = app
            .clone()
            .oneshot(
                HttpRequest::post("/_auth/login")
                    .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("password=correct+horse+battery+staple"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::SEE_OTHER);
        let cookie = login
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let set_cookie = login.headers().get(SET_COOKIE).unwrap().to_str().unwrap();
        assert!(set_cookie.contains("Secure"));
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("SameSite=Strict"));
        assert!(set_cookie.contains("Max-Age=2592000"));

        let authenticated = app
            .clone()
            .oneshot(
                HttpRequest::get("/secret")
                    .header(COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(authenticated.into_body(), 1024).await.unwrap(),
            "secret"
        );

        let sensitive = app
            .clone()
            .oneshot(
                HttpRequest::get("/api/secret")
                    .header(COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(sensitive.status(), StatusCode::OK);
        assert_eq!(sensitive.headers().get(CACHE_CONTROL).unwrap(), "no-store");

        let logout = app
            .clone()
            .oneshot(
                HttpRequest::post("/_auth/logout")
                    .header(COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logout.status(), StatusCode::SEE_OTHER);

        let after_logout = app
            .oneshot(
                HttpRequest::get("/secret")
                    .header(COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(after_logout.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_wrong_password() {
        let response = app()
            .oneshot(
                HttpRequest::post("/_auth/login")
                    .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("password=wrong"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().get(SET_COOKIE).is_none());
    }

    /// A login POST from `peer`, optionally carrying proxy headers.
    fn login_request(peer: &str, headers: &[(&str, &str)], password: &str) -> HttpRequest<Body> {
        let mut builder = HttpRequest::post("/_auth/login")
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let mut request = builder
            .body(Body::from(format!("password={password}")))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        request
    }

    async fn spend_allowance(app: &Router, peer: &str, headers: &[(&str, &str)]) {
        for _ in 0..MAX_LOGIN_ATTEMPTS {
            let response = app
                .clone()
                .oneshot(login_request(peer, headers, "wrong"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn throttles_repeated_login_attempts() {
        let app = app();
        spend_allowance(&app, "203.0.113.7:40000", &[]).await;

        let throttled = app
            .oneshot(login_request(
                "203.0.113.7:40001",
                &[],
                "correct+horse+battery+staple",
            ))
            .await
            .unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(throttled.headers().get(RETRY_AFTER).unwrap(), "60");
    }

    #[tokio::test]
    async fn throttling_one_client_leaves_every_other_client_alone() {
        let app = app();
        spend_allowance(&app, "203.0.113.7:40000", &[]).await;

        // The single legitimate user of a single-password deployment must not be
        // lockable out by anyone who can reach the form.
        let other = app
            .clone()
            .oneshot(login_request(
                "198.51.100.4:40000",
                &[],
                "correct+horse+battery+staple",
            ))
            .await
            .unwrap();
        assert_eq!(other.status(), StatusCode::SEE_OTHER);
        assert!(other.headers().get(SET_COOKIE).is_some());

        // ...and the client that spent its allowance is still throttled.
        let throttled = app
            .oneshot(login_request("203.0.113.7:40000", &[], "wrong"))
            .await
            .unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn charges_the_forwarded_client_when_the_peer_is_inside_the_deployment() {
        let app = app();
        let proxy = "127.0.0.1:40000";
        spend_allowance(&app, proxy, &[("x-forwarded-for", "203.0.113.7")]).await;

        // Same proxy, different visitor: the documented deployment puts every
        // request behind one loopback peer, so without this the throttle would
        // still be global in practice.
        let other = app
            .clone()
            .oneshot(login_request(
                proxy,
                &[("x-forwarded-for", "198.51.100.4")],
                "wrong",
            ))
            .await
            .unwrap();
        assert_eq!(other.status(), StatusCode::UNAUTHORIZED);

        let throttled = app
            .oneshot(login_request(
                proxy,
                &[("x-forwarded-for", "203.0.113.7")],
                "wrong",
            ))
            .await
            .unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn a_public_peer_cannot_shed_its_identity_with_a_forwarded_header() {
        let app = app();
        spend_allowance(
            &app,
            "203.0.113.7:40000",
            &[("x-forwarded-for", "10.0.0.1")],
        )
        .await;

        // Rotating the header buys nothing: the connection did not come from
        // inside the deployment, so it is charged to the address it came from.
        let throttled = app
            .oneshot(login_request(
                "203.0.113.7:40000",
                &[("x-forwarded-for", "192.0.2.99")],
                "wrong",
            ))
            .await
            .unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn a_successful_login_only_clears_its_own_client() {
        let app = app();
        spend_allowance(&app, "203.0.113.7:40000", &[]).await;

        let success = app
            .clone()
            .oneshot(login_request(
                "198.51.100.4:40000",
                &[],
                "correct+horse+battery+staple",
            ))
            .await
            .unwrap();
        assert_eq!(success.status(), StatusCode::SEE_OTHER);

        let still_throttled = app
            .oneshot(login_request("203.0.113.7:40000", &[], "wrong"))
            .await
            .unwrap();
        assert_eq!(still_throttled.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn requests_with_no_recorded_peer_share_one_bucket() {
        // A host that serves the router without `into_make_service_with_connect_info`
        // has no way to tell clients apart. Falling back to the old shared
        // counter keeps the form throttled at all, which beats leaving it open.
        let app = app();
        for _ in 0..MAX_LOGIN_ATTEMPTS {
            let response = app
                .clone()
                .oneshot(
                    HttpRequest::post("/_auth/login")
                        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                        .body(Body::from("password=wrong"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }

        let throttled = app
            .oneshot(
                HttpRequest::post("/_auth/login")
                    .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("password=wrong"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    fn headers(entries: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in entries {
            map.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn resolves_the_client_behind_a_trusted_proxy() {
        let from_proxy = |value: &str| {
            client_ip(
                Some(ip("127.0.0.1")),
                &headers(&[("x-forwarded-for", value)]),
            )
        };

        assert_eq!(from_proxy("203.0.113.7"), Some(ip("203.0.113.7")));
        // Chained proxies: walk past the hops that are themselves inside the
        // deployment to the outermost address that is not.
        assert_eq!(
            from_proxy("203.0.113.7, 10.0.0.5, 172.17.0.2"),
            Some(ip("203.0.113.7"))
        );
        // A LAN-only deployment has no public entry at all; the innermost one
        // still tells LAN clients apart.
        assert_eq!(from_proxy("192.168.1.50"), Some(ip("192.168.1.50")));
        // Some proxies append the source port.
        assert_eq!(from_proxy("203.0.113.7:54321"), Some(ip("203.0.113.7")));
        assert_eq!(from_proxy("[2001:db8::1]:443"), Some(ip("2001:db8::")));

        // nginx's X-Real-IP, when no forwarded chain was set.
        assert_eq!(
            client_ip(
                Some(ip("127.0.0.1")),
                &headers(&[("x-real-ip", "203.0.113.7")])
            ),
            Some(ip("203.0.113.7"))
        );
    }

    #[test]
    fn does_not_believe_a_forwarded_header_from_outside() {
        assert_eq!(
            client_ip(
                Some(ip("203.0.113.7")),
                &headers(&[("x-forwarded-for", "10.0.0.1")])
            ),
            Some(ip("203.0.113.7"))
        );
    }

    #[test]
    fn falls_back_to_the_peer_when_the_chain_is_unusable() {
        let peer = Some(ip("127.0.0.1"));
        // An unparseable nearest entry means the closest proxy wrote something
        // we do not understand; entries further left are more attacker-
        // controlled, so they are not read instead.
        assert_eq!(
            client_ip(
                peer,
                &headers(&[("x-forwarded-for", "203.0.113.7, unknown")])
            ),
            peer
        );
        assert_eq!(client_ip(peer, &HeaderMap::new()), peer);
        assert_eq!(
            client_ip(None, &headers(&[("x-forwarded-for", "203.0.113.7")])),
            None
        );
    }

    #[test]
    fn buckets_ipv6_clients_by_prefix() {
        // One machine is routinely handed a whole /64, so counting full
        // addresses would let it present a fresh identity per attempt.
        assert_eq!(throttle_bucket(ip("2001:db8:1:2::1")), ip("2001:db8:1:2::"));
        assert_eq!(
            throttle_bucket(ip("2001:db8:1:2:ffff::9")),
            throttle_bucket(ip("2001:db8:1:2::1"))
        );
        assert_ne!(
            throttle_bucket(ip("2001:db8:1:3::1")),
            throttle_bucket(ip("2001:db8:1:2::1"))
        );
        // A dual-stack listener reports IPv4 peers as `::ffff:a.b.c.d`.
        assert_eq!(throttle_bucket(ip("::ffff:203.0.113.7")), ip("203.0.113.7"));
        assert!(is_inside_deployment(ip("::ffff:127.0.0.1")));
    }

    #[test]
    fn classifies_the_deployment_boundary() {
        for inside in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.4",
            "172.16.0.1",
            "169.254.1.1",
            "100.100.1.1",
            "::1",
            "fd00::1",
            "fe80::1",
        ] {
            assert!(
                is_inside_deployment(ip(inside)),
                "{inside} should be inside"
            );
        }
        for outside in ["203.0.113.7", "8.8.8.8", "100.128.0.1", "2001:db8::1"] {
            assert!(
                !is_inside_deployment(ip(outside)),
                "{outside} should be outside"
            );
        }
    }
}
