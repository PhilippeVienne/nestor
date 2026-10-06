//! Connexion de l'interface web par passkey (WebAuthn).
//!
//! Parcours :
//! 1. `nestord onboard --passkey` ecrit un code d'enrolement a usage unique
//!    (empreinte seule, valable 10 minutes) et affiche un lien vers l'interface.
//! 2. L'interface ouverte par ce lien cree une passkey et l'enregistre ici
//!    (`/auth/register/*`). Le code est consomme.
//! 3. A chaque connexion, l'interface prouve qu'elle detient la passkey
//!    (`/auth/login/*`) et recoit un jeton de session, valable pour `/ws`.
//!
//! La cle privee ne quitte jamais l'authentificateur de l'utilisateur ; nestord ne
//! stocke que des cles publiques (`~/.config/nestord/passkeys.json`). Les jetons de
//! session ne vivent qu'en memoire : un redemarrage du daemon redemande la passkey.
//!
//! Une passkey est liee a un nom de domaine : l'interface doit etre ouverte sur
//! `http://localhost:…` ou sur un nom de domaine en HTTPS, pas sur une adresse IP.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use webauthn_rs::prelude::*;

use crate::auth::{constant_time_eq, random_hex};

/// Duree de validite d'un code d'enrolement.
pub const ENROLL_TTL: Duration = Duration::from_secs(10 * 60);
/// Duree de vie d'une ceremonie WebAuthn entamee.
const CEREMONY_TTL: Duration = Duration::from_secs(5 * 60);
/// Duree de vie d'un jeton de session.
const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Clone, Serialize, Deserialize)]
struct StoredPasskey {
    /// Domaine auquel la passkey est liee.
    rp_id: String,
    created_at_ms: u64,
    passkey: Passkey,
}

#[derive(Default, Serialize, Deserialize)]
struct StoreFile {
    /// Identifiant WebAuthn de l'utilisateur (unique : Nestor n'a qu'un proprietaire).
    user_id: String,
    passkeys: Vec<StoredPasskey>,
}

enum Ceremony {
    Register { state: PasskeyRegistration, rp_id: String },
    Login { state: PasskeyAuthentication, rp_id: String },
}

pub struct PasskeyAuth {
    store_path: PathBuf,
    enroll_path: PathBuf,
    store: Mutex<StoreFile>,
    ceremonies: Mutex<HashMap<String, (Instant, Ceremony)>>,
    /// Empreinte du jeton de session -> expiration.
    sessions: Mutex<HashMap<[u8; 32], Instant>>,
}

/// Erreur renvoyee a l'interface : code HTTP et message lisible.
pub struct AuthError(pub u16, pub String);

impl AuthError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self(status, message.into())
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/nestord")
}

/// Fichier du code d'enrolement ecrit par `nestord onboard --passkey`.
pub fn enroll_code_path() -> PathBuf {
    config_dir().join("enroll_code")
}

fn write_private(path: &PathBuf, content: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.write_all(content)?;
    Ok(())
}

/// Cree un code d'enrolement a usage unique et n'en enregistre que l'empreinte.
pub fn create_enroll_code() -> Result<String> {
    let code = random_hex(16)?;
    let record = json!({
        "sha256": Sha256::digest(code.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "expires_at_ms": now_ms() + ENROLL_TTL.as_millis() as u64,
    });
    write_private(&enroll_code_path(), record.to_string().as_bytes()).context("ecriture du code d'enrolement")?;
    Ok(code)
}

/// Construit le verificateur WebAuthn pour l'origine de la page. Le domaine de la
/// page devient l'identifiant de partie de confiance (RP ID) de la passkey.
fn webauthn_for(origin: &str) -> Result<(Webauthn, String), AuthError> {
    let url = Url::parse(origin).map_err(|_| AuthError::new(400, "origine illisible"))?;
    let Some(rp_id) = url.domain().map(str::to_string) else {
        return Err(AuthError::new(
            400,
            "une passkey est liee a un nom de domaine : ouvrez l'interface sur http://localhost (et non sur une adresse IP), ou sur son adresse en HTTPS",
        ));
    };
    let webauthn = WebauthnBuilder::new(&rp_id, &url)
        .and_then(|builder| builder.rp_name("Nestor").build())
        .map_err(|err| AuthError::new(400, format!("origine refusee par WebAuthn : {err}")))?;
    Ok((webauthn, rp_id))
}

impl PasskeyAuth {
    pub fn load() -> Arc<Self> {
        Self::load_from(config_dir().join("passkeys.json"), enroll_code_path())
    }

    fn load_from(store_path: PathBuf, enroll_path: PathBuf) -> Arc<Self> {
        let mut store: StoreFile = std::fs::read_to_string(&store_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        if store.user_id.len() != 32 {
            store.user_id = random_hex(16).unwrap_or_else(|_| "0".repeat(32));
        }
        Arc::new(Self {
            store_path,
            enroll_path,
            store: Mutex::new(store),
            ceremonies: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
        })
    }

    fn save(&self, store: &StoreFile) {
        let saved = serde_json::to_vec_pretty(store).map_err(anyhow::Error::from).and_then(|bytes| write_private(&self.store_path, &bytes));
        if let Err(err) = saved {
            tracing::error!(?err, "enregistrement des passkeys impossible");
        }
    }

    /// Au moins une passkey est enregistree : `/ws` exige alors une authentification.
    pub fn has_any(&self) -> bool {
        !self.store.lock().unwrap().passkeys.is_empty()
    }

    fn count_for(&self, rp_id: &str) -> usize {
        self.store.lock().unwrap().passkeys.iter().filter(|p| p.rp_id == rp_id).count()
    }

    fn user_uuid(&self) -> Uuid {
        let hex = self.store.lock().unwrap().user_id.clone();
        let mut bytes = [0u8; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(2 * i..2 * i + 2).unwrap_or("00"), 16).unwrap_or(0);
        }
        Uuid::from_bytes(bytes)
    }

    /// Le code presente est-il le code d'enrolement en cours ? `consume` le detruit.
    fn check_enroll_code(&self, code: &str, consume: bool) -> bool {
        let Ok(raw) = std::fs::read_to_string(&self.enroll_path) else { return false };
        let Ok(record) = serde_json::from_str::<Value>(&raw) else { return false };
        let expected = record.get("sha256").and_then(Value::as_str).unwrap_or_default();
        let expires = record.get("expires_at_ms").and_then(Value::as_u64).unwrap_or(0);
        if now_ms() > expires {
            let _ = std::fs::remove_file(&self.enroll_path);
            return false;
        }
        let presented: String = Sha256::digest(code.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        let ok = constant_time_eq(presented.as_bytes(), expected.as_bytes());
        if ok && consume {
            let _ = std::fs::remove_file(&self.enroll_path);
        }
        ok
    }

    fn remember(&self, ceremony: Ceremony) -> Result<String, AuthError> {
        let id = random_hex(16).map_err(|_| AuthError::new(500, "alea indisponible"))?;
        let mut ceremonies = self.ceremonies.lock().unwrap();
        ceremonies.retain(|_, (at, _)| at.elapsed() < CEREMONY_TTL);
        ceremonies.insert(id.clone(), (Instant::now(), ceremony));
        Ok(id)
    }

    fn take(&self, id: &str) -> Option<Ceremony> {
        let (at, ceremony) = self.ceremonies.lock().unwrap().remove(id)?;
        (at.elapsed() < CEREMONY_TTL).then_some(ceremony)
    }

    fn open_session(&self) -> Result<String, AuthError> {
        let token = random_hex(32).map_err(|_| AuthError::new(500, "alea indisponible"))?;
        let mut sessions = self.sessions.lock().unwrap();
        let now = Instant::now();
        sessions.retain(|_, expires| *expires > now);
        sessions.insert(Sha256::digest(token.as_bytes()).into(), now + SESSION_TTL);
        Ok(token)
    }

    /// Le jeton presente est-il une session ouverte par passkey ?
    pub fn session_valid(&self, token: &str) -> bool {
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        self.sessions.lock().unwrap().get(&digest).is_some_and(|expires| *expires > Instant::now())
    }

    /// Etat pour l'interface : y a-t-il une passkey pour ce domaine, un enrolement est-il possible ?
    pub fn status(&self, origin: &str) -> Value {
        match webauthn_for(origin) {
            Ok((_, rp_id)) => json!({ "rp_id": rp_id, "passkeys": self.count_for(&rp_id), "domain_ok": true }),
            Err(AuthError(_, message)) => json!({ "passkeys": 0, "domain_ok": false, "detail": message }),
        }
    }

    /// Etape 1 de l'enrolement : verifie le code et prepare la creation de la passkey.
    pub fn register_options(&self, origin: &str, code: &str) -> Result<Value, AuthError> {
        if !self.check_enroll_code(code, false) {
            return Err(AuthError::new(403, "lien d'enrolement invalide ou expire : relancez `nestord onboard --passkey`"));
        }
        let (webauthn, rp_id) = webauthn_for(origin)?;
        let existing: Vec<CredentialID> = self
            .store
            .lock()
            .unwrap()
            .passkeys
            .iter()
            .filter(|p| p.rp_id == rp_id)
            .map(|p| p.passkey.cred_id().clone())
            .collect();
        let (options, state) = webauthn
            .start_passkey_registration(self.user_uuid(), "nestor", "Nestor", Some(existing))
            .map_err(|err| AuthError::new(500, format!("preparation de la passkey : {err}")))?;
        let id = self.remember(Ceremony::Register { state, rp_id })?;
        Ok(json!({ "id": id, "options": options }))
    }

    /// Etape 2 : verifie la passkey creee, consomme le code, l'enregistre et ouvre une session.
    pub fn register_finish(&self, origin: &str, id: &str, code: &str, credential: Value) -> Result<String, AuthError> {
        let Some(Ceremony::Register { state, rp_id }) = self.take(id) else {
            return Err(AuthError::new(400, "enrolement expire, recommencez"));
        };
        let (webauthn, origin_rp) = webauthn_for(origin)?;
        if origin_rp != rp_id {
            return Err(AuthError::new(400, "l'origine a change en cours d'enrolement"));
        }
        let credential: RegisterPublicKeyCredential =
            serde_json::from_value(credential).map_err(|err| AuthError::new(400, format!("reponse illisible : {err}")))?;
        let passkey = webauthn
            .finish_passkey_registration(&credential, &state)
            .map_err(|err| AuthError::new(403, format!("passkey refusee : {err}")))?;
        // Le code n'est consomme qu'une fois la passkey verifiee, et ne sert qu'une fois.
        if !self.check_enroll_code(code, true) {
            return Err(AuthError::new(403, "lien d'enrolement invalide ou expire"));
        }
        {
            let mut store = self.store.lock().unwrap();
            store.passkeys.push(StoredPasskey { rp_id: rp_id.clone(), created_at_ms: now_ms(), passkey });
            self.save(&store);
        }
        tracing::info!(%rp_id, "passkey enregistree");
        self.open_session()
    }

    /// Etape 1 de la connexion : prepare le defi a signer par la passkey.
    pub fn login_options(&self, origin: &str) -> Result<Value, AuthError> {
        let (webauthn, rp_id) = webauthn_for(origin)?;
        let passkeys: Vec<Passkey> =
            self.store.lock().unwrap().passkeys.iter().filter(|p| p.rp_id == rp_id).map(|p| p.passkey.clone()).collect();
        if passkeys.is_empty() {
            return Err(AuthError::new(404, "aucune passkey enregistree pour cette adresse"));
        }
        let (options, state) = webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|err| AuthError::new(500, format!("preparation de la connexion : {err}")))?;
        let id = self.remember(Ceremony::Login { state, rp_id })?;
        Ok(json!({ "id": id, "options": options }))
    }

    /// Etape 2 : verifie la signature et ouvre une session.
    pub fn login_finish(&self, origin: &str, id: &str, credential: Value) -> Result<String, AuthError> {
        let Some(Ceremony::Login { state, rp_id }) = self.take(id) else {
            return Err(AuthError::new(400, "connexion expiree, recommencez"));
        };
        let (webauthn, origin_rp) = webauthn_for(origin)?;
        if origin_rp != rp_id {
            return Err(AuthError::new(400, "l'origine a change en cours de connexion"));
        }
        let credential: PublicKeyCredential =
            serde_json::from_value(credential).map_err(|err| AuthError::new(400, format!("reponse illisible : {err}")))?;
        let result = webauthn
            .finish_passkey_authentication(&credential, &state)
            .map_err(|err| AuthError::new(403, format!("passkey refusee : {err}")))?;
        {
            // Met a jour le compteur de signatures (detection de clonage).
            let mut store = self.store.lock().unwrap();
            let mut changed = false;
            for stored in store.passkeys.iter_mut() {
                if stored.passkey.update_credential(&result) == Some(true) {
                    changed = true;
                }
            }
            if changed {
                self.save(&store);
            }
        }
        self.open_session()
    }
}

static GLOBAL: OnceLock<Arc<PasskeyAuth>> = OnceLock::new();

pub fn init() {
    let _ = GLOBAL.set(PasskeyAuth::load());
}

pub fn global() -> Option<&'static Arc<PasskeyAuth>> {
    GLOBAL.get()
}

/// Au moins une passkey est-elle enregistree ?
pub fn has_any() -> bool {
    global().is_some_and(|p| p.has_any())
}

/// Le jeton est-il une session ouverte par passkey ?
pub fn session_valid(token: &str) -> bool {
    global().is_some_and(|p| p.session_valid(token))
}

// ---------------------------------------------------------------- points d'acces HTTP

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::ws::AppState;

fn fail(error: AuthError) -> Response {
    let status = StatusCode::from_u16(error.0).unwrap_or(StatusCode::BAD_REQUEST);
    (status, Json(json!({ "error": error.1 }))).into_response()
}

/// Origine de la page appelante : exigee (ces points d'acces ne servent qu'a un navigateur)
/// et soumise a la meme regle que `/ws`.
fn page_origin(headers: &HeaderMap, state: &AppState) -> Result<String, Response> {
    let origin = headers.get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok());
    match origin {
        Some(origin) if crate::auth::origin_allowed(Some(origin), &state.config.allowed_origins) => Ok(origin.to_string()),
        _ => Err(fail(AuthError::new(403, "origine non autorisee"))),
    }
}

fn passkeys() -> Result<&'static Arc<PasskeyAuth>, Response> {
    global().ok_or_else(|| fail(AuthError::new(503, "authentification par passkey indisponible")))
}

fn text<'a>(body: &'a Value, key: &str) -> &'a str {
    body.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub async fn status_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let origin = match page_origin(&headers, &state) {
        Ok(origin) => origin,
        Err(response) => return response,
    };
    let passkeys = match passkeys() {
        Ok(passkeys) => passkeys,
        Err(response) => return response,
    };
    let mut status = passkeys.status(&origin);
    status["auth_required"] = json!(state.config.auth.is_some() || passkeys.has_any());
    Json(status).into_response()
}

pub async fn register_options_handler(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let origin = match page_origin(&headers, &state) {
        Ok(origin) => origin,
        Err(response) => return response,
    };
    match passkeys().map(|p| p.register_options(&origin, text(&body, "code"))) {
        Ok(Ok(options)) => Json(options).into_response(),
        Ok(Err(error)) => fail(error),
        Err(response) => response,
    }
}

pub async fn register_finish_handler(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let origin = match page_origin(&headers, &state) {
        Ok(origin) => origin,
        Err(response) => return response,
    };
    let credential = body.get("credential").cloned().unwrap_or(Value::Null);
    match passkeys().map(|p| p.register_finish(&origin, text(&body, "id"), text(&body, "code"), credential)) {
        Ok(Ok(token)) => {
            // Une passkey vient d'etre creee : l'acces a `/ws` est desormais protege.
            let _ = state.events_tx.send(crate::dashboard::context_event(&state.config, &state.current_place));
            Json(json!({ "token": token })).into_response()
        }
        Ok(Err(error)) => fail(error),
        Err(response) => response,
    }
}

pub async fn login_options_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let origin = match page_origin(&headers, &state) {
        Ok(origin) => origin,
        Err(response) => return response,
    };
    match passkeys().map(|p| p.login_options(&origin)) {
        Ok(Ok(options)) => Json(options).into_response(),
        Ok(Err(error)) => fail(error),
        Err(response) => response,
    }
}

pub async fn login_finish_handler(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let origin = match page_origin(&headers, &state) {
        Ok(origin) => origin,
        Err(response) => return response,
    };
    let credential = body.get("credential").cloned().unwrap_or(Value::Null);
    match passkeys().map(|p| p.login_finish(&origin, text(&body, "id"), credential)) {
        Ok(Ok(token)) => Json(json!({ "token": token })).into_response(),
        Ok(Err(error)) => fail(error),
        Err(response) => response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webauthn_authenticator_rs::softpasskey::SoftPasskey;
    use webauthn_authenticator_rs::WebauthnAuthenticator;

    const ORIGIN: &str = "http://localhost:5173";

    /// Magasin de passkeys isole dans un dossier temporaire, avec un code d'enrolement.
    fn fixture(name: &str) -> (Arc<PasskeyAuth>, String, PathBuf) {
        let dir = std::env::temp_dir().join(format!("nestord-passkey-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let auth = PasskeyAuth::load_from(dir.join("passkeys.json"), dir.join("enroll_code"));
        let code = "code-de-test".to_string();
        let record = json!({
            "sha256": Sha256::digest(code.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "expires_at_ms": now_ms() + 60_000,
        });
        std::fs::write(dir.join("enroll_code"), record.to_string()).unwrap();
        (auth, code, dir)
    }

    /// Fait creer une passkey par un authentificateur logiciel et l'enregistre.
    fn enroll(auth: &PasskeyAuth, code: &str, device: &mut WebauthnAuthenticator<SoftPasskey>) -> Result<String, AuthError> {
        let started = auth.register_options(ORIGIN, code)?;
        let options: CreationChallengeResponse = serde_json::from_value(started["options"].clone()).unwrap();
        let credential = device.do_registration(Url::parse(ORIGIN).unwrap(), options).unwrap();
        auth.register_finish(ORIGIN, started["id"].as_str().unwrap(), code, serde_json::to_value(credential).unwrap())
    }

    fn login(auth: &PasskeyAuth, device: &mut WebauthnAuthenticator<SoftPasskey>, origin: &str) -> Result<String, AuthError> {
        let started = auth.login_options(origin)?;
        let options: RequestChallengeResponse = serde_json::from_value(started["options"].clone()).unwrap();
        let credential = device
            .do_authentication(Url::parse(origin).unwrap(), options)
            .map_err(|err| AuthError::new(0, format!("{err:?}")))?;
        auth.login_finish(origin, started["id"].as_str().unwrap(), serde_json::to_value(credential).unwrap())
    }

    #[test]
    fn enrolement_puis_connexion() {
        let (auth, code, dir) = fixture("parcours");
        let mut device = WebauthnAuthenticator::new(SoftPasskey::new(true));
        assert!(!auth.has_any());

        let session = enroll(&auth, &code, &mut device).ok().expect("enrolement");
        assert!(auth.has_any());
        assert!(auth.session_valid(&session));
        assert!(!auth.session_valid("jeton-invente"));
        assert_eq!(auth.status(ORIGIN)["passkeys"], 1);

        // Le code ne sert qu'une fois.
        assert!(!dir.join("enroll_code").exists());
        assert!(auth.register_options(ORIGIN, &code).is_err());

        // La passkey survit a un redemarrage du daemon ; les sessions, non.
        let reloaded = PasskeyAuth::load_from(dir.join("passkeys.json"), dir.join("enroll_code"));
        assert!(reloaded.has_any());
        assert!(!reloaded.session_valid(&session));
        let again = login(&reloaded, &mut device, ORIGIN).ok().expect("connexion");
        assert!(reloaded.session_valid(&again));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mauvais_code_ou_autre_authentificateur_refuses() {
        let (auth, code, dir) = fixture("refus");
        let mut device = WebauthnAuthenticator::new(SoftPasskey::new(true));

        // Sans le bon code, pas d'enrolement, et le vrai code reste utilisable.
        assert!(auth.register_options(ORIGIN, "mauvais-code").is_err());
        assert!(!auth.has_any());
        enroll(&auth, &code, &mut device).ok().expect("enrolement");

        // Un autre authentificateur ne detient pas la cle : connexion impossible.
        let mut stranger = WebauthnAuthenticator::new(SoftPasskey::new(true));
        assert!(login(&auth, &mut stranger, ORIGIN).is_err());

        // Une ceremonie ne se rejoue pas.
        let started = auth.login_options(ORIGIN).ok().unwrap();
        let options: RequestChallengeResponse = serde_json::from_value(started["options"].clone()).unwrap();
        let credential = serde_json::to_value(device.do_authentication(Url::parse(ORIGIN).unwrap(), options).unwrap()).unwrap();
        let id = started["id"].as_str().unwrap();
        assert!(auth.login_finish(ORIGIN, id, credential.clone()).is_ok());
        assert!(auth.login_finish(ORIGIN, id, credential).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn domaine_exige_et_code_expire() {
        let (auth, code, dir) = fixture("domaine");
        // Une adresse IP n'est pas un domaine : pas de passkey possible.
        assert_eq!(auth.status("http://127.0.0.1:5173")["domain_ok"], false);
        assert!(auth.register_options("http://127.0.0.1:5173", &code).is_err());
        // Aucune passkey pour un autre domaine.
        assert!(auth.login_options("https://kanto.exemple.ts.net").is_err());

        // Code expire : refuse et efface.
        let expired = json!({
            "sha256": Sha256::digest(code.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "expires_at_ms": now_ms() - 1,
        });
        std::fs::write(dir.join("enroll_code"), expired.to_string()).unwrap();
        assert!(auth.register_options(ORIGIN, &code).is_err());
        assert!(!dir.join("enroll_code").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
