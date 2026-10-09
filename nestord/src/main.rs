//! nestord : daemon backend de "Nestor", l'assistant vocal local pour Claude Code.
//! Orchestre l'audio local, pilote un sous-processus `claude` headless et
//! expose une API WebSocket a l'UI web et a l'appli mobile sur `127.0.0.1:8340/ws`.

#[cfg(feature = "full-audio")]
mod audio;
mod auth;
mod brain;
mod claude_process;
mod clock;
mod config;
mod connectors;
mod dashboard;
mod judge;
mod mcp;
mod memory;
mod mission;
mod onboard;
mod passkey;
mod protocol;
mod settings;
mod sleep;
mod calendar;
mod capture;
mod location;
mod power;
mod proactive;
mod todo;
mod usage;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::broadcast;

#[cfg(not(feature = "full-audio"))]
use protocol::DaemonStatus;
use protocol::ServerEvent;
use ws::AppState;

pub const LISTEN_ADDR: &str = "127.0.0.1:8340";

/// Config MCP passee au CLI `claude` : elle pointe vers notre propre serveur
/// HTTP, d'ou l'obligation d'ecouter avant de spawner le sous-processus.
fn write_mcp_config(mcp_secret: &str) -> anyhow::Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let path = std::env::temp_dir().join("nestord-mcp.json");
    // `/mcp` exige ce secret, tire au hasard a chaque demarrage. Il n'ouvre que les outils :
    // ni `/ws`, ni l'approbation d'une ecriture (cf. `auth.rs`).
    let config = serde_json::json!({
        "mcpServers": {
            "nestor": {
                "type": "http",
                "url": format!("http://{LISTEN_ADDR}/mcp"),
                "headers": { "Authorization": format!("Bearer {mcp_secret}") },
            }
        }
    });
    let _ = std::fs::remove_file(&path);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
    file.write_all(&serde_json::to_vec_pretty(&config)?)?;
    Ok(path)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("onboard") {
        if args.get(1).map(String::as_str) == Some("--google") {
            return calendar::onboard(&config::Config::load()).await;
        }
        return onboard::run(&args[1..]);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nestord=info")),
        )
        .init();

    let (events_tx, _) = broadcast::channel::<ServerEvent>(256);
    let barge_in_gen = Arc::new(AtomicU64::new(0));
    // Valeur de `barge_in_gen` au debut du dernier tour utilisateur (cf. ws.rs).
    let turn_started_gen = Arc::new(AtomicU64::new(0));
    let speaking_until_ms = Arc::new(AtomicU64::new(0));
    let usage = Arc::new(usage::UsageState::default());
    let config = Arc::new(config::Config::load());
    settings::init(&config);
    passkey::init();
    if config.auth.is_none() && !passkey::has_any() {
        tracing::warn!(
            "aucune authentification configuree : /ws accepte toute connexion venant de cette machine. \
Creez une passkey (`nestord onboard --passkey`) ou un jeton (`nestord onboard`) avant toute exposition reseau."
        );
    }
    judge::spawn_warmup(settings::get().judge_config(&config.judge));
    let todos = Arc::new(todo::TodoStore::open_default()?);
    memory::init(Arc::new(memory::MemoryStore::open_default()?));

    // La session conversationnelle n'existe pas encore : elle est renseignee
    // apres le demarrage du serveur HTTP, cf. plus bas.
    let claude_cell: Arc<claude_process::ClaudeSlot> = Arc::new(claude_process::ClaudeSlot::new());

    #[cfg(feature = "full-audio")]
    let (tts_tx, tts_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    #[cfg(not(feature = "full-audio"))]
    let tts_tx: Option<tokio::sync::mpsc::UnboundedSender<String>> = None;

    let brain = Arc::new(brain::NestorBrain::new(
        claude_cell.clone(),
        events_tx.clone(),
        #[cfg(feature = "full-audio")]
        Some(tts_tx.clone()),
        #[cfg(not(feature = "full-audio"))]
        tts_tx.clone(),
        usage.clone(),
        config.clone(),
        barge_in_gen.clone(),
        turn_started_gen.clone(),
    ));

    // Audio micro (front -> serveur), en frames binaires PCM16LE. Existe
    // meme sans `full-audio` (cout negligeable) pour eviter de feature-gater
    // `AppState`. L'audio TTS (serveur -> front) transite en JSON, cf. ws.rs.
    let (mic_tx, mic_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

    let missions = Arc::new(mission::MissionManager::new(
        events_tx.clone(),
        claude_cell.clone(),
        usage.clone(),
    ));

    let wake_active_until_ms = Arc::new(AtomicU64::new(0));

    let mcp_secret = auth::random_hex(32)?;
    auth::set_secret_env_vars(connectors::referenced_env_vars(&config.mcp_servers));

    let state = Arc::new(AppState {
        mcp_secret: mcp_secret.clone(),
        events_tx: events_tx.clone(),
        config: config.clone(),
        todos: todos.clone(),
        current_place: Arc::new(std::sync::Mutex::new(None)),
        barge_in_gen: barge_in_gen.clone(),
        turn_started_gen: turn_started_gen.clone(),
        mic_tx,
        speaking_until_ms: speaking_until_ms.clone(),
        wake_active_until_ms: wake_active_until_ms.clone(),
        missions,
        usage: usage.clone(),
        brain: brain.clone(),
    });
    connectors::init(&config.mcp_servers, config.auth.is_some() || passkey::has_any(), events_tx.clone());
    dashboard::spawn_ticker(events_tx.clone(), config.clone(), state.current_place.clone());
    power::spawn(events_tx.clone(), config.clone(), state.missions.clone(), state.current_place.clone());
    calendar::spawn(events_tx.clone(), config.clone(), state.current_place.clone());
    sleep::spawn(events_tx.clone(), config.clone(), state.current_place.clone());
    capture::spawn(events_tx.clone(), config.clone());
    proactive::spawn(
        events_tx.clone(),
        brain.clone(),
        config.clone(),
        todos.clone(),
        state.missions.clone(),
        usage.clone(),
        state.current_place.clone(),
    );

    // Les points d'acces de connexion par passkey sont appeles par la page de l'interface
    // (autre port, donc autre origine) : CORS limite aux origines que `/ws` accepte.
    let allowed_origins = config.allowed_origins.clone();
    let auth_cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(move |origin, _| {
            auth::origin_allowed(origin.to_str().ok(), &allowed_origins)
        }))
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE]);
    let auth_routes = Router::new()
        .route("/auth/status", get(passkey::status_handler))
        .route("/auth/register/options", post(passkey::register_options_handler))
        .route("/auth/register/finish", post(passkey::register_finish_handler))
        .route("/auth/login/options", post(passkey::login_options_handler))
        .route("/auth/login/finish", post(passkey::login_finish_handler))
        .route("/auth/logout", post(passkey::logout_handler))
        .route("/auth/passkeys", post(passkey::passkeys_list_handler))
        .route("/auth/passkeys/revoke", post(passkey::passkey_revoke_handler))
        .layer(auth_cors);

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .route("/mcp", post(mcp::mcp_handler))
        // Position envoyee par l'appli mobile hors appel (cf. `location.rs`).
        .route("/location", post(location::location_handler))
        // Appele par le timer de reveil (cf. `sleep.rs`), avec un secret tire au demarrage.
        .route("/wake", post(sleep::wake_handler))
        .merge(auth_routes)
        .with_state(state);

    let addr: SocketAddr = LISTEN_ADDR.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "nestord demarre, WebSocket sur /ws et MCP sur /mcp");
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    // Le serveur ecoute : le CLI peut maintenant se connecter a notre MCP.
    let mcp_config = write_mcp_config(&mcp_secret)?;

    let spawn_params = claude_process::SpawnParams {
        events_tx: events_tx.clone(),
        #[cfg(feature = "full-audio")]
        tts_tx: Some(tts_tx.clone()),
        #[cfg(not(feature = "full-audio"))]
        tts_tx,
        usage: usage.clone(),
        mcp_config_path: Some(mcp_config.clone()),
        brain: brain.clone(),
        config: config.clone(),
        slot: claude_cell.clone(),
    };
    // Le superviseur garde ces parametres pour relancer la session (plantage, sortie de veille).
    claude_process::install(spawn_params.clone());
    let claude = claude_process::spawn(&spawn_params, None)?;
    claude_cell.set(claude);

    #[cfg(feature = "full-audio")]
    audio::spawn(
        events_tx.clone(),
        brain.clone(),
        mic_rx,
        tts_rx,
        tts_tx,
        barge_in_gen,
        turn_started_gen,
        speaking_until_ms,
        wake_active_until_ms,
        config.clone(),
    )?;
    #[cfg(not(feature = "full-audio"))]
    drop(mic_rx);

    // En mode texte seul (sans `full-audio`), l'etat de repos par defaut est
    // Idle ; le pipeline audio (quand actif) bascule lui-meme sur Listening.
    #[cfg(not(feature = "full-audio"))]
    let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Idle });

    server.await??;

    Ok(())
}
