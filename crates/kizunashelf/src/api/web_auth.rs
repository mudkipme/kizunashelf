//! Optional single-user authentication for the network-facing web runtime.
//!
//! This wraps the already-built router instead of becoming part of it. Desktop
//! and iOS therefore keep driving the shared router directly, and these host
//! routes do not become part of the generated OpenAPI contract.

use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;
use axum::body::Body;
use axum::extract::{Form, Request, State};
use axum::http::header::{
    ACCEPT, CACHE_CONTROL, CONTENT_SECURITY_POLICY, COOKIE, REFERRER_POLICY, RETRY_AFTER,
    SET_COOKIE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand_core::{OsRng, RngCore};
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const COOKIE_NAME: &str = "__Host-kizunashelf";
const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
const MAX_LOGIN_ATTEMPTS: usize = 5;
const MAX_PASSWORD_BYTES: usize = 1024;

#[derive(Clone)]
pub struct WebAuth {
    inner: Arc<WebAuthInner>,
}

struct WebAuthInner {
    password_hash: String,
    sessions: Mutex<HashMap<String, Instant>>,
    login_attempts: Mutex<VecDeque<Instant>>,
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
                login_attempts: Mutex::new(VecDeque::new()),
            }),
        })
    }

    fn reserve_login_attempt(&self) -> bool {
        let now = Instant::now();
        let mut attempts = self
            .inner
            .login_attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while attempts
            .front()
            .is_some_and(|attempt| now.duration_since(*attempt) >= LOGIN_WINDOW)
        {
            attempts.pop_front();
        }
        if attempts.len() >= MAX_LOGIN_ATTEMPTS {
            return false;
        }
        attempts.push_back(now);
        true
    }

    fn clear_login_attempts(&self) {
        self.inner
            .login_attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    async fn verify(&self, password: String) -> bool {
        if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
            return false;
        }
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
        OsRng.fill_bytes(&mut bytes);
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

async fn login(State(auth): State<WebAuth>, Form(form): Form<LoginForm>) -> Response {
    if !auth.reserve_login_attempt() {
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

    auth.clear_login_attempts();
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
    use argon2::password_hash::{PasswordHasher, SaltString};
    use axum::body::to_bytes;
    use axum::http::header::{CONTENT_TYPE, LOCATION};
    use axum::http::Request as HttpRequest;
    use axum::routing::get;
    use tower::ServiceExt;

    fn test_auth() -> WebAuth {
        let salt = SaltString::encode_b64(b"test salt for auth").unwrap();
        let hash = Argon2::default()
            .hash_password(b"correct horse battery staple", &salt)
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

    #[tokio::test]
    async fn throttles_repeated_login_attempts() {
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
                    .body(Body::from("password=correct+horse+battery+staple"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(throttled.headers().get(RETRY_AFTER).unwrap(), "60");
    }
}
