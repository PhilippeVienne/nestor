//! Controle d'acces au daemon.
//!
//! Trois protections, pensees pour que l'assistant lui-meme (qui execute des
//! commandes avec les droits de l'utilisateur) ne puisse pas s'accorder ce que
//! l'utilisateur doit valider :
//!
//! - **Jeton d'acces a `/ws`** : nestord n'en garde que l'empreinte SHA-256, sur
//!   disque comme en memoire. Lire `~/.config/nestord/auth_token` ne donne donc
//!   pas le jeton, et la comparaison se fait en temps constant.
//! - **Secret de `/mcp`** : tire au hasard a chaque demarrage et remis au seul
//!   assistant. Il ouvre les outils, pas `/ws` : il ne permet ni d'approuver une
//!   ecriture ni de changer un reglage.
//! - **Origine** : une page web d'un autre site ne peut pas se connecter, meme
//!   sans jeton (le navigateur envoie son en-tete `Origin`, que l'on verifie).
//!
//! Limite assumee : l'assistant tourne sous le meme compte que l'utilisateur. Il
//! peut lire ses autres fichiers (jetons OAuth des serveurs MCP locaux, profil du
//! navigateur). Une isolation complete demanderait un compte ou un bac a sable
//! distinct pour l'assistant.

use std::io::Read;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

/// Empreinte SHA-256 d'un jeton d'acces.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenHash([u8; 32]);

impl std::fmt::Debug for TokenHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenHash(…)")
    }
}

impl TokenHash {
    pub fn of(token: &str) -> Self {
        Self(Sha256::digest(token.as_bytes()).into())
    }

    /// Lit une empreinte en hexadecimal (64 caracteres).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        if hex.len() != 64 || !hex.is_ascii() {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Le jeton presente correspond-il ? Comparaison en temps constant.
    pub fn matches(&self, presented: &str) -> bool {
        constant_time_eq(&Self::of(presented).0, &self.0)
    }
}

/// Egalite sans sortie anticipee : la duree ne depend pas de la position du premier ecart.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Chaine aleatoire de `bytes` octets en hexadecimal, tiree du noyau.
pub fn random_hex(bytes: usize) -> anyhow::Result<String> {
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// L'origine de la requete est-elle acceptable ?
///
/// Un client natif (assistant, application mobile, script) n'envoie pas d'en-tete
/// `Origin`. Un navigateur l'envoie toujours et ne peut pas le falsifier : on
/// n'accepte que la machine locale et les origines declarees dans `allowed_origins`.
pub fn origin_allowed(origin: Option<&str>, allowed: &[String]) -> bool {
    let Some(origin) = origin else { return true };
    let origin = origin.trim().trim_end_matches('/');
    if allowed.iter().any(|a| a.trim().trim_end_matches('/').eq_ignore_ascii_case(origin)) {
        return true;
    }
    let Some(rest) = origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://")) else {
        return false;
    };
    // Hote sans le port (une adresse IPv6 est entre crochets).
    let host = if let Some(end) = rest.find(']') {
        &rest[..=end]
    } else {
        rest.split(':').next().unwrap_or_default()
    };
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

static SECRET_ENV_VARS: OnceLock<Vec<String>> = OnceLock::new();

/// Declare les variables d'environnement qui portent un secret (jeton de nestord,
/// identifiants des connecteurs) : elles sont retirees de l'environnement de l'assistant.
pub fn set_secret_env_vars(mut vars: Vec<String>) {
    vars.push("NESTORD_AUTH_TOKEN".to_string());
    vars.sort();
    vars.dedup();
    let _ = SECRET_ENV_VARS.set(vars);
}

/// Variables a retirer de l'environnement de tout sous-processus de l'assistant.
pub fn secret_env_vars() -> &'static [String] {
    SECRET_ENV_VARS.get().map(Vec::as_slice).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empreinte_du_jeton() {
        let hash = TokenHash::of("jeton-secret");
        assert!(hash.matches("jeton-secret"));
        assert!(!hash.matches("jeton-secreT"));
        assert!(!hash.matches(""));
        // L'empreinte se relit, et ne revele pas le jeton dans les journaux.
        assert_eq!(TokenHash::from_hex(&hash.to_hex()), Some(hash.clone()));
        assert_eq!(format!("{hash:?}"), "TokenHash(…)");
        assert!(TokenHash::from_hex("pas une empreinte").is_none());
        assert!(TokenHash::from_hex(&"z".repeat(64)).is_none());
    }

    #[test]
    fn origines_acceptees() {
        let allowed = vec!["https://kanto.exemple.ts.net:8443".to_string()];
        // Clients natifs : pas d'en-tete Origin.
        assert!(origin_allowed(None, &[]));
        // Interface servie sur la machine locale.
        assert!(origin_allowed(Some("http://127.0.0.1:5173"), &[]));
        assert!(origin_allowed(Some("http://localhost:5173"), &[]));
        assert!(origin_allowed(Some("http://[::1]:5173"), &[]));
        // Origine declaree.
        assert!(origin_allowed(Some("https://kanto.exemple.ts.net:8443/"), &allowed));
        // Page d'un autre site, y compris les tentatives de ressemblance.
        assert!(!origin_allowed(Some("https://site-malveillant.example"), &allowed));
        assert!(!origin_allowed(Some("http://127.0.0.1.site-malveillant.example"), &[]));
        assert!(!origin_allowed(Some("http://localhost.site-malveillant.example:5173"), &[]));
        assert!(!origin_allowed(Some("null"), &[]));
    }

    #[test]
    fn comparaison_en_temps_constant() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
