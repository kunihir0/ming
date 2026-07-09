#![allow(clippy::pedantic)]
use axum::{
    Json, Router,
    extract::{
        Path, State, WebSocketUpgrade, Query,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::{get, post},
};
use tower_http::services::ServeDir;
use db::{
    DbPool, establish_connection_pool,
    models::{FcmCredential, PairedServer, PlayerStat},
    schema::{
        fcm_credentials::dsl as fcm_dsl, paired_servers::dsl as ps_dsl,
        player_stats::dsl as stats_dsl,
    },
};
use diesel::prelude::*;
use rustplus::RustPlusClient;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tracing::{error, info};

pub mod auth;

pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

pub struct ApiState {
    pub db_pool: DbPool,
    pub clients: Mutex<HashMap<i32, Arc<RustPlusClient>>>,
    pub events: Mutex<HashMap<i32, broadcast::Sender<rustplus::events::RustEvent>>>,
    pub oauth: OAuthConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("api=debug,rustplus=debug,info")
        .init();
        
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = dotenvy::dotenv();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db_pool = establish_connection_pool(&database_url);

    {
        let mut conn = db_pool.get()?;
        db::run_migrations(&mut conn)?;
        info!("Database migrations applied.");
    }

    let oauth_config = OAuthConfig {
        client_id: std::env::var("DISCORD_CLIENT_ID").expect("DISCORD_CLIENT_ID must be set"),
        client_secret: std::env::var("DISCORD_CLIENT_SECRET")
            .expect("DISCORD_CLIENT_SECRET must be set"),
        redirect_uri: std::env::var("OAUTH_REDIRECT_URI").expect("OAUTH_REDIRECT_URI must be set"),
    };

    let state = Arc::new(ApiState {
        db_pool,
        clients: Mutex::new(HashMap::new()),
        events: Mutex::new(HashMap::new()),
        oauth: oauth_config,
    });

    let app = Router::new()
        .route("/api/auth/discord/login", get(auth::login))
        .route("/api/auth/discord/callback", get(auth::callback))
        .route("/api/auth/me", get(auth::get_me))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/rustplus/link", post(auth::link_rustplus))
        .route("/api/market/ticker", get(get_ticker))
        .route("/api/market/history", get(get_history))
        .route("/api/servers", get(list_servers))
        .route("/api/server/{id}/info", get(get_info))
        .route("/api/server/{id}/map", get(get_map))
        .route("/api/server/{id}/map/meta", get(get_map_meta))
        .route("/api/server/{id}/map/image", get(get_map_image))
        .route("/api/server/{id}/markers", get(get_markers))
        .route("/api/server/{id}/stats", get(get_stats))
        .route("/api/server/{id}/team", get(get_team))
        .route("/api/server/{id}/team/chat", get(get_team_chat))
        .route("/api/server/{id}/team/promote", post(promote_to_leader))
        .route("/api/server/{id}/time", get(get_time))
        .route("/api/server/{id}/chat", post(send_chat))
        .route("/api/server/{id}/entity/{entity_id}", get(get_entity))
        .route(
            "/api/server/{id}/entity/{entity_id}/toggle",
            post(toggle_entity),
        )
        .route(
            "/api/server/{id}/entity/{entity_id}/subscription",
            get(check_subscription),
        )
        .route(
            "/api/server/{id}/entity/{entity_id}/subscription",
            post(set_subscription),
        )
        .route("/api/server/{id}/clan", get(get_clan_info))
        .route("/api/server/{id}/clan/chat", get(get_clan_chat))
        .route("/api/server/{id}/clan/chat", post(send_clan_message))
        .route("/api/server/{id}/clan/motd", post(set_clan_motd))
        .route("/api/server/{id}/nexus/{app_key}", get(get_nexus_auth))
        .route(
            "/api/server/{id}/camera/{camera_id}/subscribe",
            post(camera_subscribe),
        )
        .route(
            "/api/server/{id}/camera/{camera_id}/unsubscribe",
            post(camera_unsubscribe),
        )
        .route(
            "/api/server/{id}/camera/{camera_id}/input",
            post(camera_input),
        )
        .route("/api/server/{id}/camera/{camera_id}/ptz", post(camera_ptz))
        .route(
            "/api/server/{id}/camera/{camera_id}/stream",
            get(camera_stream),
        )
        .route(
            "/api/server/{id}/camera/{camera_id}/stream/raw",
            get(camera_stream_raw),
        )
        .route("/api/server/{id}/ws", get(ws_handler))
        .fallback_service(ServeDir::new(std::env::var("VENDING_MARKET_WEB_ROOT").unwrap_or_else(|_| "../web/dist".to_string())))
        .with_state(state);

    let port: u16 = std::env::var("VENDING_MARKET_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting Unified API & Web server on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(serde::Deserialize)]
pub struct HistoryQuery {
    item: String,
}

async fn get_ticker(State(_state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "data": [] }))
}

async fn get_history(
    State(_state): State<Arc<ApiState>>,
    Query(query): Query<HistoryQuery>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "item": query.item, "data": [] }))
}

async fn get_or_connect_client(
    server_id: i32,
    state: &ApiState,
) -> Option<(
    Arc<RustPlusClient>,
    broadcast::Receiver<rustplus::events::RustEvent>,
)> {
    let mut clients = state.clients.lock().await;
    let mut events = state.events.lock().await;

    if let (Some(client), Some(tx)) = (clients.get(&server_id), events.get(&server_id))
        && client.is_connected()
    {
        return Some((client.clone(), tx.subscribe()));
    }

    let mut conn = match state.db_pool.get() {
        Ok(c) => c,
        Err(e) => {
            error!("DB error: {}", e);
            return None;
        }
    };

    let server: PairedServer = match ps_dsl::paired_servers.find(server_id).first(&mut conn) {
        Ok(s) => s,
        Err(_) => return None,
    };

    let cred: FcmCredential = match fcm_dsl::fcm_credentials
        .find(server.fcm_credential_id)
        .first(&mut conn)
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    let steam_id = cred.steam_id.parse::<u64>().unwrap_or(0);

    let mut client = RustPlusClient::new(
        server.server_ip.clone(),
        u16::try_from(server.server_port).unwrap_or(28082),
        steam_id,
        server.player_token,
        true, // Proxy
    );

    if let Err(e) = client.connect().await {
        error!("Failed to connect to server {} for API: {}", server_id, e);
        return None;
    }

    let rx = client.take_broadcast_receiver();
    let arc_client = Arc::new(client.clone());
    clients.insert(server_id, arc_client.clone());

    let (tx, event_rx) = broadcast::channel(100);
    events.insert(server_id, tx.clone());

    if let Some(mut rx) = rx {
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if let Some(broadcast) = msg.broadcast {
                    let _ = tx_clone.send(rustplus::events::RustEvent::RawBroadcast(Box::new(
                        broadcast,
                    )));
                }
            }
        });
    }

    let mut monitor_loop = rustplus::monitor::MonitorLoop::new(client.clone(), tx.clone());
    monitor_loop.register(Box::new(rustplus::monitors::cargo::CargoMonitor::new()));
    monitor_loop.register(Box::new(
        rustplus::monitors::explosion::ExplosionMonitor::new(),
    ));
    tokio::spawn(monitor_loop.run());

    Some((arc_client, event_rx))
}

async fn list_servers(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let mut conn = match state.db_pool.get() {
        Ok(c) => c,
        Err(_) => return Json(json!({"error": "db"})),
    };

    let servers = match ps_dsl::paired_servers.load::<PairedServer>(&mut conn) {
        Ok(s) => s,
        Err(_) => return Json(json!({"error": "db"})),
    };

    let json_servers: Vec<_> = servers
        .into_iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "ip": s.server_ip,
                "port": s.server_port
            })
        })
        .collect();

    Json(json!(json_servers))
}

async fn get_map(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let (client, _) = match get_or_connect_client(server_id, &state).await {
        Some(c) => c,
        None => return Json(json!({"error": "connection failed"})),
    };

    let info = match client.get_info().await {
        Ok(res) => res.response.and_then(|r| r.info),
        Err(_) => None,
    };

    let markers = match client.get_map_markers().await {
        Ok(res) => res
            .response
            .and_then(|r| r.map_markers)
            .map(|m| m.markers)
            .unwrap_or_default(),
        Err(_) => vec![],
    };

    let monuments = match client.get_map().await {
        Ok(res) => res
            .response
            .and_then(|r| r.map)
            .map(|m| m.monuments)
            .unwrap_or_default(),
        Err(_) => vec![],
    };

    Json(json!({
        "info": info,
        "markers": markers,
        "monuments": monuments
    }))
}

async fn get_stats(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let mut conn = match state.db_pool.get() {
        Ok(c) => c,
        Err(_) => return Json(json!({"error": "db"})),
    };

    let stats: Vec<PlayerStat> = match stats_dsl::player_stats
        .filter(stats_dsl::server_id.eq(server_id))
        .load::<PlayerStat>(&mut conn)
    {
        Ok(s) => s,
        Err(_) => return Json(json!({"error": "db"})),
    };

    // Basic aggregation
    let mut deaths = HashMap::new();
    let mut afks = HashMap::new();

    for stat in stats {
        if stat.event_type == "Death" {
            *deaths.entry(stat.steam_id.clone()).or_insert(0) += 1;
        } else if stat.event_type == "AfkStart" {
            *afks.entry(stat.steam_id.clone()).or_insert(0) += 1;
        }
    }

    Json(json!({
        "deaths": deaths,
        "afk_sessions": afks,
    }))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, server_id))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<ApiState>, server_id: i32) {
    let (_, mut rx) = match get_or_connect_client(server_id, &state).await {
        Some(c) => c,
        None => return,
    };

    while let Ok(event) = rx.recv().await {
        let payload = match event {
            rustplus::events::RustEvent::CargoSpawned => json!({"type": "cargo_spawned"}),
            rustplus::events::RustEvent::MarkerSnapshot(m) => json!({"type": "markers", "data": m}),
            rustplus::events::RustEvent::CameraMotion {
                camera_id,
                player_count,
                names,
            } => {
                json!({
                    "type": "camera_motion",
                    "camera_id": camera_id,
                    "player_count": player_count,
                    "names": names
                })
            }
            rustplus::events::RustEvent::ExplosionOccurred { position } => {
                json!({
                    "type": "explosion",
                    "data": { "x": position.0, "y": position.1 }
                })
            }
            rustplus::events::RustEvent::RawBroadcast(b) => {
                json!({
                    "type": "broadcast",
                    "data": b
                })
            }
            // Fallback for others
            _ => json!({"type": "other"}),
        };

        if let Ok(text) = serde_json::to_string(&payload)
            && socket
                .send(Message::Text(axum::extract::ws::Utf8Bytes::from(text)))
                .await
                .is_err()
        {
            break;
        }
    }
}

#[derive(serde::Deserialize)]
struct CameraInputPayload {
    buttons: i32,
    x: f32,
    y: f32,
}

#[derive(serde::Deserialize)]
struct PtzPayload {
    action: String, // "zoom", "shoot", "reload"
}

async fn get_info(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    let info = match client.get_info().await {
        Ok(res) => res.response.and_then(|r| r.info),
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    Json(json!({"info": info}))
}

async fn get_markers(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    let markers = match client.get_map_markers().await {
        Ok(res) => res
            .response
            .and_then(|r| r.map_markers)
            .map(|m| m.markers)
            .unwrap_or_default(),
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    // Include map metadata so the frontend can compute pixel positions correctly
    let map_meta = match client.get_map().await {
        Ok(res) => res.response.and_then(|r| r.map).map(|m| {
            json!({
                "width": m.width,
                "height": m.height,
                "oceanMargin": m.ocean_margin
            })
        }),
        Err(_) => None,
    };

    let info = match client.get_info().await {
        Ok(res) => res.response.and_then(|r| r.info),
        Err(_) => None,
    };

    // Log first marker for debugging coordinate ranges
    if let Some(first) = markers.first() {
        tracing::debug!(
            x = first.x,
            y = first.y,
            map_size = ?info.as_ref().map(|i| i.map_size),
            "First marker coordinates"
        );
    }

    Json(json!({
        "markers": markers,
        "mapMeta": map_meta,
        "mapSize": info.map(|i| i.map_size)
    }))
}

async fn camera_input(
    State(state): State<Arc<ApiState>>,
    Path((server_id, _camera_id)): Path<(i32, String)>,
    Json(payload): Json<CameraInputPayload>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    // Camera input currently applies globally to whatever camera is subscribed on the client.
    // Ideally, the client has already subscribed via a WS message or another endpoint.
    match client
        .send_camera_input(payload.buttons, payload.x, payload.y)
        .await
    {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn camera_ptz(
    State(state): State<Arc<ApiState>>,
    Path((server_id, camera_id)): Path<(i32, String)>,
    Json(payload): Json<PtzPayload>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    let camera = client.get_camera(&camera_id);
    let res = match payload.action.as_str() {
        "zoom" => camera.zoom().await,
        "shoot" => camera.shoot().await,
        "reload" => camera.reload().await,
        _ => return Json(json!({"error": "invalid action"})),
    };

    match res {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

#[derive(serde::Deserialize)]
struct ChatPayload {
    message: String,
}

#[derive(serde::Deserialize)]
struct TogglePayload {
    value: bool,
}

async fn get_team(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    let team = match client.get_team_info().await {
        Ok(res) => res.response.and_then(|r| r.team_info),
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    Json(json!({"team": team}))
}

async fn get_time(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    let time = match client.get_time().await {
        Ok(res) => res.response.and_then(|r| r.time),
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    Json(json!({"time": time}))
}

async fn send_chat(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
    Json(payload): Json<ChatPayload>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.send_team_message(&payload.message).await {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn get_entity(
    State(state): State<Arc<ApiState>>,
    Path((server_id, entity_id)): Path<(i32, u32)>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    let entity = match client.get_entity_info(entity_id).await {
        Ok(res) => res.response.and_then(|r| r.entity_info),
        Err(e) => return Json(json!({"error": e.to_string()})),
    };

    Json(json!({"entity": entity}))
}

async fn toggle_entity(
    State(state): State<Arc<ApiState>>,
    Path((server_id, entity_id)): Path<(i32, u32)>,
    Json(payload): Json<TogglePayload>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.set_entity_value(entity_id, payload.value).await {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn camera_subscribe(
    State(state): State<Arc<ApiState>>,
    Path((server_id, camera_id)): Path<(i32, String)>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.subscribe_to_camera(&camera_id).await {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn camera_unsubscribe(
    State(state): State<Arc<ApiState>>,
    Path((server_id, _camera_id)): Path<(i32, String)>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.unsubscribe_from_camera().await {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

#[derive(serde::Deserialize)]
struct PromotePayload {
    steam_id: u64,
}

async fn get_team_chat(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.get_team_chat().await {
        Ok(res) => Json(
            json!({"messages": res.response.and_then(|r| r.team_chat).map(|tc| tc.messages).unwrap_or_default()}),
        ),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn promote_to_leader(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
    Json(payload): Json<PromotePayload>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.promote_to_leader(payload.steam_id).await {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn check_subscription(
    State(state): State<Arc<ApiState>>,
    Path((server_id, entity_id)): Path<(i32, u32)>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.check_subscription(entity_id).await {
        Ok(res) => Json(
            json!({"is_subscribed": res.response.and_then(|r| r.flag).map(|f| f.value).unwrap_or_default()}),
        ),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn set_subscription(
    State(state): State<Arc<ApiState>>,
    Path((server_id, entity_id)): Path<(i32, u32)>,
    Json(payload): Json<TogglePayload>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.set_subscription(entity_id, payload.value).await {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn get_clan_info(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.get_clan_info().await {
        Ok(res) => Json(json!({"clan_info": res.response.and_then(|r| r.clan_info)})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn get_clan_chat(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.get_clan_chat().await {
        Ok(res) => Json(
            json!({"messages": res.response.and_then(|r| r.clan_chat).map(|tc| tc.messages).unwrap_or_default()}),
        ),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn send_clan_message(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
    Json(payload): Json<ChatPayload>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.send_clan_message(&payload.message).await {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn set_clan_motd(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
    Json(payload): Json<ChatPayload>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.set_clan_motd(&payload.message).await {
        Ok(_) => Json(json!({"success": true})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn get_nexus_auth(
    State(state): State<Arc<ApiState>>,
    Path((server_id, app_key)): Path<(i32, String)>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"}));
    };

    match client.get_nexus_auth(&app_key).await {
        Ok(res) => Json(json!({"auth": res.response.and_then(|r| r.nexus_auth)})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

async fn get_map_image(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, vec![]).into_response();
    };

    let map = match client.get_map().await {
        Ok(res) => match res.response.and_then(|r| r.map) {
            Some(m) => m,
            None => return (axum::http::StatusCode::NOT_FOUND, vec![]).into_response(),
        },
        Err(_) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, vec![]).into_response(),
    };

    (
        [(axum::http::header::CONTENT_TYPE, "image/jpeg")],
        map.jpg_image,
    )
        .into_response()
}

async fn get_map_meta(
    State(state): State<Arc<ApiState>>,
    Path(server_id): Path<i32>,
) -> impl IntoResponse {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return Json(json!({"error": "connection failed"})).into_response();
    };

    let map = match client.get_map().await {
        Ok(res) => match res.response.and_then(|r| r.map) {
            Some(m) => m,
            None => return Json(json!({"error": "not found"})).into_response(),
        },
        Err(e) => return Json(json!({"error": e.to_string()})).into_response(),
    };

    Json(json!({
        "width": map.width,
        "height": map.height,
        "margin": map.ocean_margin
    }))
    .into_response()
}

async fn camera_stream(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    Path((server_id, camera_id)): Path<(i32, String)>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_camera_socket(socket, state, server_id, camera_id))
}

async fn handle_camera_socket(
    mut socket: WebSocket,
    state: Arc<ApiState>,
    server_id: i32,
    camera_id: String,
) {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return;
    };

    let mut camera = client.get_camera(&camera_id);
    let _ = camera.subscribe().await; // Ensure it's subscribed
    let mut rx = camera.subscribe_frames();

    while let Ok(frame_data) = rx.recv().await {
        // Create an axum body Bytes from the frame_data Vec<u8>
        if socket
            .send(Message::Binary(axum::body::Bytes::from(frame_data)))
            .await
            .is_err()
        {
            break;
        }
    }
}

async fn camera_stream_raw(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ApiState>>,
    Path((server_id, camera_id)): Path<(i32, String)>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_camera_socket_raw(socket, state, server_id, camera_id))
}

async fn handle_camera_socket_raw(
    mut socket: WebSocket,
    state: Arc<ApiState>,
    server_id: i32,
    camera_id: String,
) {
    let Some((client, _)) = get_or_connect_client(server_id, &state).await else {
        return;
    };

    let mut camera = client.get_camera(&camera_id);
    let _ = camera.subscribe().await; // Ensure it's subscribed

    let Some(mut rx) = client.take_broadcast_receiver() else {
        return;
    };

    while let Ok(msg) = rx.recv().await {
        if let Some(broadcast) = msg.broadcast
            && let Some(rays) = broadcast.camera_rays
            && let Ok(text) = serde_json::to_string(&rays)
            && socket
                .send(Message::Text(axum::extract::ws::Utf8Bytes::from(text)))
                .await
                .is_err()
        {
            break;
        }
    }
}
