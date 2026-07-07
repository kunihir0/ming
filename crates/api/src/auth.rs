use crate::ApiState;
use axum::{
    Json,
    extract::{FromRequestParts, Query, State},
    http::request::Parts,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use db::{
    models::{NewSession, NewUser, NewUserRustplusCredential, Session, User},
    schema::{sessions, user_rustplus_credentials, users},
};
use diesel::prelude::*;
use rand::{Rng, distributions::Alphanumeric};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct AuthQuery {
    code: String,
}

#[derive(Serialize, Deserialize)]
struct DiscordTokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    refresh_token: Option<String>,
    scope: String,
}

#[derive(Serialize, Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    avatar: Option<String>,
}

pub struct AuthenticatedUser {
    pub user: User,
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    Arc<ApiState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = axum::http::StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = Arc::from_ref(state);
        let jar = CookieJar::from_headers(&parts.headers);

        let token = jar
            .get("session_id")
            .map(|c| c.value().to_string())
            .ok_or(axum::http::StatusCode::UNAUTHORIZED)?;

        let mut conn = state
            .db_pool
            .get()
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

        let now = chrono::Utc::now().naive_utc();

        let session_data: (Session, User) = sessions::table
            .inner_join(users::table)
            .filter(sessions::token.eq(token))
            .filter(sessions::expires_at.gt(now))
            .first::<(Session, User)>(&mut conn)
            .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?;

        Ok(AuthenticatedUser {
            user: session_data.1,
        })
    }
}

// Helper trait for State extraction in FromRequestParts
pub trait FromRef<S> {
    fn from_ref(state: &S) -> Self;
}

impl FromRef<Arc<ApiState>> for Arc<ApiState> {
    fn from_ref(state: &Arc<ApiState>) -> Self {
        state.clone()
    }
}

pub async fn login(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let url = format!(
        "https://discord.com/api/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&scope=identify",
        state.oauth.client_id,
        urlencoding::encode(&state.oauth.redirect_uri)
    );
    Redirect::to(&url)
}

pub async fn callback(
    State(state): State<Arc<ApiState>>,
    jar: CookieJar,
    Query(query): Query<AuthQuery>,
) -> impl IntoResponse {
    let client = reqwest::Client::new();

    // 1. Exchange code for token
    let params = [
        ("client_id", &state.oauth.client_id),
        ("client_secret", &state.oauth.client_secret),
        ("grant_type", &"authorization_code".to_string()),
        ("code", &query.code),
        ("redirect_uri", &state.oauth.redirect_uri),
    ];

    let res = client
        .post("https://discord.com/api/oauth2/token")
        .form(&params)
        .send()
        .await;

    let token_res = match res {
        Ok(r) => {
            if !r.status().is_success() {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    format!("Discord API Error ({}): {}", status, text),
                )
                    .into_response();
            }
            let raw_json = r.text().await.unwrap_or_default();
            match serde_json::from_str::<DiscordTokenResponse>(&raw_json) {
                Ok(t) => t,
                Err(e) => {
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!(
                            "Failed to parse token. Error: {}. Raw JSON: {}",
                            e, raw_json
                        ),
                    )
                        .into_response();
                }
            }
        }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to exchange code: {}", e),
            )
                .into_response();
        }
    };

    // 2. Fetch user profile
    let user_res = client
        .get("https://discord.com/api/users/@me")
        .bearer_auth(token_res.access_token)
        .send()
        .await;

    let discord_user = match user_res {
        Ok(r) => match r.json::<DiscordUser>().await {
            Ok(u) => u,
            Err(e) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to parse user: {}", e),
                )
                    .into_response();
            }
        },
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch user: {}", e),
            )
                .into_response();
        }
    };

    // 3. Upsert user and create session
    let mut conn = match state.db_pool.get() {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let new_user = NewUser {
        discord_id: discord_user.id.clone(),
        username: discord_user.username,
        avatar: discord_user.avatar,
    };

    if let Err(e) = diesel::insert_into(users::table)
        .values(&new_user)
        .on_conflict(users::discord_id)
        .do_update()
        .set((
            users::username.eq(&new_user.username),
            users::avatar.eq(&new_user.avatar),
        ))
        .execute(&mut conn)
    {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error user: {}", e),
        )
            .into_response();
    }

    let token: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::days(7);

    let new_session = NewSession {
        token: token.clone(),
        discord_id: discord_user.id,
        expires_at,
    };

    if let Err(e) = diesel::insert_into(sessions::table)
        .values(&new_session)
        .execute(&mut conn)
    {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error session: {}", e),
        )
            .into_response();
    }

    // 4. Set cookie and redirect to dashboard
    let cookie = Cookie::build(("session_id", token))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .expires(
            time::OffsetDateTime::from_unix_timestamp(expires_at.and_utc().timestamp()).unwrap(),
        );

    (jar.add(cookie), Redirect::to("/")).into_response()
}

pub async fn get_me(auth: AuthenticatedUser) -> impl IntoResponse {
    Json(auth.user)
}

pub async fn logout(jar: CookieJar, State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    if let Some(cookie) = jar.get("session_id") {
        let token = cookie.value().to_string();
        if let Ok(mut conn) = state.db_pool.get() {
            let _ = diesel::delete(sessions::table.filter(sessions::token.eq(token)))
                .execute(&mut conn);
        }
    }

    let mut cookie = Cookie::build(("session_id", "")).path("/").build();
    cookie.make_removal();

    (jar.add(cookie), Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize)]
pub struct LinkRustPlusPayload {
    fcm_credentials: FcmCreds,
    expo_push_token: String,
    rustplus_auth_token: String,
}

#[derive(Deserialize)]
pub struct FcmCreds {
    gcm: Gcm,
}

#[derive(Deserialize)]
pub struct Gcm {
    android_id: u64,
    security_token: u64,
}

pub async fn link_rustplus(
    auth: AuthenticatedUser,
    State(state): State<Arc<ApiState>>,
    Json(payload): Json<LinkRustPlusPayload>,
) -> impl IntoResponse {
    let mut conn = match state.db_pool.get() {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let new_cred = NewUserRustplusCredential {
        discord_id: auth.user.discord_id.clone(),
        gcm_android_id: payload.fcm_credentials.gcm.android_id.to_string(),
        gcm_security_token: payload.fcm_credentials.gcm.security_token.to_string(),
        expo_push_token: payload.expo_push_token,
        rustplus_auth_token: payload.rustplus_auth_token,
    };

    if let Err(e) = diesel::insert_into(user_rustplus_credentials::table)
        .values(&new_cred)
        .on_conflict(user_rustplus_credentials::discord_id)
        .do_update()
        .set((
            user_rustplus_credentials::gcm_android_id.eq(&new_cred.gcm_android_id),
            user_rustplus_credentials::gcm_security_token.eq(&new_cred.gcm_security_token),
            user_rustplus_credentials::expo_push_token.eq(&new_cred.expo_push_token),
            user_rustplus_credentials::rustplus_auth_token.eq(&new_cred.rustplus_auth_token),
        ))
        .execute(&mut conn)
    {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("DB error: {}", e),
        )
            .into_response();
    }

    Json(serde_json::json!({"success": true})).into_response()
}
