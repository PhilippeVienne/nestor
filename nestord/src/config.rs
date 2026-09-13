//! Configuration de nestord, lue au demarrage depuis un fichier TOML.
//!
//! Elle porte ce qui depend de l'utilisateur et de son domicile : forme
//! d'adresse, lieux nommes avec leur rayon, heures calmes, heure de reveil par
//! defaut. Ces valeurs conditionnent le contexte (`context.rs`) et, plus tard,
//! la boucle proactive et la mise en veille.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Emplacement par defaut : `~/.config/nestord/config.toml`.
fn default_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config/nestord/config.toml")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Forme d'adresse employee par Nestor. `NESTORD_ADDRESS_FORM` a priorite.
    pub address_form: String,
    /// Lieux reconnus, pour pouvoir dire « Monsieur est chez lui ».
    pub places: Vec<Place>,
    pub quiet_hours: QuietHours,
    pub wake: WakeConfig,
    pub judge: JudgeConfig,
    /// Jeton partage exige pour se connecter a `/ws` (query `?token=...`).
    /// `NESTORD_AUTH_TOKEN` a priorite. Absent (defaut) : pas de verification,
    /// tolerable tant que nestord n'ecoute que sur `127.0.0.1` - a definir
    /// avant toute exposition au-dela du loopback (VPN, Tailscale...).
    /// `/mcp` n'est jamais concerne : reserve a la boucle locale.
    pub auth_token: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            address_form: "Monsieur".to_string(),
            places: Vec::new(),
            quiet_hours: QuietHours::default(),
            wake: WakeConfig::default(),
            judge: JudgeConfig::default(),
            auth_token: None,
        }
    }
}

/// Juge de conscience local (LLM Ollama), porte de `agy-plugin-guardrail` :
/// evalue le risque d'un message utilisateur ou d'une mission avant de la
/// lancer. Cf. `judge.rs`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct JudgeConfig {
    pub enabled: bool,
    pub ollama_host: String,
    pub model: String,
    /// Score (0-100) a partir duquel une confirmation humaine est exigee.
    pub confirm_threshold: u8,
    /// Score (0-100) a partir duquel l'action est refusee sans appel possible.
    pub reject_threshold: u8,
    /// Delai maximal d'attente d'Ollama avant d'exiger une confirmation par prudence (fail-safe).
    pub timeout_ms: u64,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ollama_host: "http://127.0.0.1:11434".to_string(),
            model: "llama3.2:1b".to_string(),
            confirm_threshold: 60,
            reject_threshold: 90,
            timeout_ms: 4000,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Place {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// Rayon en metres a l'interieur duquel on considere etre sur le lieu.
    #[serde(default = "default_radius_m")]
    pub radius_m: f64,
}

fn default_radius_m() -> f64 {
    150.0
}

/// Plage pendant laquelle Nestor ne prend pas l'initiative de parler.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct QuietHours {
    /// Heure de debut au format `HH:MM`.
    pub start: String,
    /// Heure de fin au format `HH:MM`.
    pub end: String,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self { start: "22:00".to_string(), end: "07:30".to_string() }
    }
}

impl QuietHours {
    /// Indique si `time` tombe dans la plage calme. Gere le cas ou la plage
    /// traverse minuit (ex: 22:00 -> 07:30).
    pub fn contains(&self, time: chrono::NaiveTime) -> bool {
        let (Some(start), Some(end)) = (parse_hhmm(&self.start), parse_hhmm(&self.end)) else {
            return false;
        };

        if start <= end {
            time >= start && time < end
        } else {
            time >= start || time < end
        }
    }
}

fn parse_hhmm(value: &str) -> Option<chrono::NaiveTime> {
    chrono::NaiveTime::parse_from_str(value.trim(), "%H:%M").ok()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WakeConfig {
    /// Heure de reveil si l'agenda du lendemain est vide.
    pub default_time: String,
    /// Temps de preparation a reserver avant un premier rendez-vous.
    pub preparation_minutes: u32,
}

impl Default for WakeConfig {
    fn default() -> Self {
        Self { default_time: "07:30".to_string(), preparation_minutes: 45 }
    }
}

impl Config {
    /// Charge la configuration, ou retourne les valeurs par defaut si le
    /// fichier est absent. Un fichier illisible ou invalide est signale mais
    /// ne bloque pas le demarrage : Nestor doit pouvoir tourner sans config.
    pub fn load() -> Self {
        let path = std::env::var("NESTORD_CONFIG").map(PathBuf::from).unwrap_or_else(|_| default_config_path());
        Self::load_from(&path)
    }

    fn load_from(path: &Path) -> Self {
        let mut config = match std::fs::read_to_string(path) {
            Ok(raw) => match toml::from_str::<Config>(&raw) {
                Ok(config) => {
                    tracing::info!(
                        path = %path.display(),
                        places = config.places.len(),
                        "configuration chargee"
                    );
                    config
                }
                Err(err) => {
                    tracing::error!(?err, path = %path.display(), "configuration invalide, valeurs par defaut utilisees");
                    Config::default()
                }
            },
            Err(_) => {
                tracing::info!(path = %path.display(), "aucune configuration, valeurs par defaut utilisees");
                Config::default()
            }
        };

        if let Ok(form) = std::env::var("NESTORD_ADDRESS_FORM") {
            let form = form.trim().to_string();
            if !form.is_empty() {
                config.address_form = form;
            }
        }

        if let Ok(token) = std::env::var("NESTORD_AUTH_TOKEN") {
            let token = token.trim().to_string();
            if !token.is_empty() {
                config.auth_token = Some(token);
            }
        }

        if config.auth_token.is_none() {
            tracing::warn!(
                "aucun auth_token configure : /ws accepte toute connexion. Tolerable tant que \
nestord n'ecoute que sur 127.0.0.1, a definir avant toute exposition reseau plus large."
            );
        }

        config
    }

    /// Nom du lieu connu contenant ce point, s'il y en a un.
    pub fn place_at(&self, lat: f64, lon: f64) -> Option<&str> {
        self.places
            .iter()
            .find(|place| distance_m(lat, lon, place.lat, place.lon) <= place.radius_m)
            .map(|place| place.name.as_str())
    }
}

/// Distance en metres entre deux points (formule de haversine). Suffisant a
/// l'echelle d'un rayon de quelques centaines de metres.
fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    let (phi1, phi2) = (lat1.to_radians(), lat2.to_radians());
    let delta_phi = phi2 - phi1;
    let delta_lambda = (lon2 - lon1).to_radians();

    let a = (delta_phi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (delta_lambda / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_entre_deux_points_proches() {
        // Un degre de latitude vaut environ 111 km ; 0,001 degre ~ 111 m.
        let d = distance_m(48.8566, 2.3522, 48.8576, 2.3522);
        assert!((d - 111.0).abs() < 5.0, "distance inattendue : {d}");
    }

    #[test]
    fn lieu_reconnu_dans_son_rayon() {
        let config = Config {
            places: vec![Place { name: "domicile".into(), lat: 48.8566, lon: 2.3522, radius_m: 150.0 }],
            ..Config::default()
        };

        assert_eq!(config.place_at(48.8566, 2.3522), Some("domicile"));
        assert_eq!(config.place_at(48.8576, 2.3522), Some("domicile"));
        assert_eq!(config.place_at(48.8700, 2.3522), None);
    }

    #[test]
    fn heures_calmes_traversant_minuit() {
        let quiet = QuietHours::default(); // 22:00 -> 07:30

        assert!(quiet.contains(chrono::NaiveTime::from_hms_opt(23, 0, 0).unwrap()));
        assert!(quiet.contains(chrono::NaiveTime::from_hms_opt(3, 0, 0).unwrap()));
        assert!(!quiet.contains(chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap()));
        assert!(!quiet.contains(chrono::NaiveTime::from_hms_opt(7, 30, 0).unwrap()));
    }
}
