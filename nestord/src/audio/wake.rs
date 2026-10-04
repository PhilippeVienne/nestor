//! Detection locale du mot-cle d'activation ("Hey Nestor").
//!
//! Pour preserver la batterie et la confidentialite (eviter d'interroger Claude/AGY
//! ou de declencher la synthese vocale sur chaque bruit de fond), Nestor n'engage la
//! conversation que lorsqu'il est interpelle par son mot-cle :
//! "Hey Nestor", "Nestor", "Dis Nestor", "Eh Nestor".
//!
//! Une fois reveille, Nestor reste en ecoute active pendant une fenetre configurable
//! (15 secondes par defaut) apres sa derniere prise de parole, permettant un dialogue
//! naturel sans repeter "Hey Nestor" a chaque phrase.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Actions resultantes de l'evaluation d'un enonce transcrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeAction {
    /// Le mot-cle a ete detecte seul (ex: "Hey Nestor", "Nestor !").
    /// Nestor doit accuser reception ("Oui, Monsieur ?") et ouvrir la fenetre d'ecoute.
    WakeOnly { ack_phrase: String },
    /// Le mot-cle a ete detecte avec une commande (ex: "Hey Nestor, quelle heure est-il ?").
    /// Le mot-cle est retire et la commande propre est transmise au cerveau.
    Command { query: String },
    /// Nestor est deja en session active (conversation en cours dans la fenetre de follow-up).
    /// Le message est traite comme la suite normale du dialogue.
    FollowUp { text: String },
    /// Nestor est en veille et aucun mot-cle n'est present : l'enonce est ignore.
    Ignored,
}

/// Detecteur de mot-cle d'activation.
pub struct WakeDetector {
    enabled: bool,
    timeout_ms: u64,
    wake_active_until_ms: Arc<AtomicU64>,
    address_form: String,
    custom_ack: Option<String>,
}

impl WakeDetector {
    pub fn new(
        enabled: bool,
        timeout_secs: u64,
        wake_active_until_ms: Arc<AtomicU64>,
        address_form: String,
        custom_ack: Option<String>,
    ) -> Self {
        Self {
            enabled,
            timeout_ms: timeout_secs.max(3) * 1000,
            wake_active_until_ms,
            address_form,
            custom_ack,
        }
    }

    /// Indique si Nestor est actuellement dans une fenetre de dialogue actif.
    pub fn is_active(&self, now_ms: u64) -> bool {
        now_ms < self.wake_active_until_ms.load(Ordering::SeqCst)
    }

    /// Prolonge la session active d'une duree de `timeout_ms`.
    pub fn refresh_session(&self, now_ms: u64) {
        let deadline = now_ms + self.timeout_ms;
        self.wake_active_until_ms.store(deadline, Ordering::SeqCst);
    }

    /// Formule la phrase d'acquittement personnalisee ("Oui, Monsieur ?").
    pub fn ack_phrase(&self) -> String {
        if let Some(ref custom) = self.custom_ack {
            custom.replace("{address}", &self.address_form)
        } else {
            format!("Oui, {} ?", self.address_form)
        }
    }

    /// Evalue un texte transcrit par Whisper.
    pub fn evaluate(&self, text: &str, now_ms: u64) -> WakeAction {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return WakeAction::Ignored;
        }

        // Si la detection est desactivee, tout enonce est traite en direct.
        if !self.enabled {
            return WakeAction::Command { query: trimmed.to_string() };
        }

        let is_active = self.is_active(now_ms);

        if let Some((_prefix, query)) = extract_wake_invocation(trimmed) {
            // Mot-cle detecte ! Nestor s'eveille.
            self.refresh_session(now_ms);
            if query.is_empty() {
                WakeAction::WakeOnly { ack_phrase: self.ack_phrase() }
            } else {
                WakeAction::Command { query }
            }
        } else if is_active {
            // Dialogue en cours dans la fenetre active (mode follow-up).
            self.refresh_session(now_ms);
            WakeAction::FollowUp { text: trimmed.to_string() }
        } else {
            // En veille, enonce ignorant le mot-cle : on ignore sans reveiller l'IA.
            WakeAction::Ignored
        }
    }
}

/// Tente d'extraire une interpellation de Nestor dans l'enonce.
///
/// Retourne `Some((prefixe_trouve, commande_restante))` si l'enonce s'adresse a Nestor.
/// Retourne `None` s'il s'agit d'une mention a la 3e personne ("Nestor a dit hier")
/// ou d'une phrase non adressee a l'assistant ("Passe-moi le sel").
pub fn extract_wake_invocation(raw: &str) -> Option<(String, String)> {
    let nestor_pos = find_case_insensitive(raw, "nestor")?;
    let prefix = &raw[..nestor_pos];
    let norm_prefix = normalize_for_match(prefix);

    // Prefixes autorises devant "nestor" pour considerer que c'est une interpellation directe.
    const ALLOWED_PREFIXES: &[&str] = &[
        "",             // "Nestor, ..."
        "hey",          // "Hey Nestor, ..."
        "he",           // "Hé Nestor, ..."
        "eh",           // "Eh Nestor, ..."
        "dis",          // "Dis Nestor, ..."
        "dis moi",      // "Dis-moi Nestor, ..."
        "ok",           // "Ok Nestor, ..."
        "okay",         // "Okay Nestor, ..."
        "salut",        // "Salut Nestor, ..."
        "bonjour",      // "Bonjour Nestor, ..."
        "coucou",       // "Coucou Nestor, ..."
        "allo",         // "Allô Nestor, ..."
        // Variantes avec mots de remplissage oraux ("euh", "ah")
        "euh",
        "ah",
        "euh hey",
        "euh he",
        "euh dis",
        "ah hey",
        "ah dis",
    ];

    if !ALLOWED_PREFIXES.contains(&norm_prefix.as_str()) {
        // Le mot "Nestor" apparait, mais avec un contexte non interpellatif
        // (ex: "est-ce que nestor", "selon nestor", "hier avec nestor").
        return None;
    }

    let nestor_end = nestor_pos + "nestor".len();
    let remainder = &raw[nestor_end..];

    // Nettoie la ponctuation separatrice entre l'interpellation et la requete.
    let query = remainder
        .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ':' | '.' | '!' | '?' | '-' | '…'))
        .trim();

    Some((prefix.trim().to_string(), query.to_string()))
}

/// Recherche insensible a la casse de `needle` dans `haystack`.
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let h_lower = haystack.to_lowercase();
    let n_lower = needle.to_lowercase();
    h_lower.find(&n_lower)
}

/// Normalise une chaine pour la comparaison de mots-cles :
/// minuscule, suppression des accents francais, suppression de la ponctuation,
/// compactage des espaces.
fn normalize_for_match(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c.to_ascii_lowercase() {
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'à' | 'â' | 'ä' => out.push('a'),
            'ô' | 'ö' => out.push('o'),
            'î' | 'ï' => out.push('i'),
            'û' | 'ù' | 'ü' => out.push('u'),
            'ç' => out.push('c'),
            '-' | '_' => out.push(' '),
            c if c.is_ascii_alphanumeric() || c == ' ' => out.push(c),
            _ => {} // Ignore virgules, apostrophes, etc.
        }
    }

    // Compacte les espaces multiples
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wake_invocation() {
        // Wake-only
        assert_eq!(extract_wake_invocation("Hey Nestor"), Some(("Hey".to_string(), "".to_string())));
        assert_eq!(extract_wake_invocation("Hey Nestor !"), Some(("Hey".to_string(), "".to_string())));
        assert_eq!(extract_wake_invocation("Hé Nestor"), Some(("Hé".to_string(), "".to_string())));
        assert_eq!(extract_wake_invocation("Dis Nestor ?"), Some(("Dis".to_string(), "".to_string())));
        assert_eq!(extract_wake_invocation("Nestor"), Some(("".to_string(), "".to_string())));
        assert_eq!(extract_wake_invocation("Nestor !"), Some(("".to_string(), "".to_string())));

        // One-shot commands
        assert_eq!(
            extract_wake_invocation("Hey Nestor, quelle heure est-il ?"),
            Some(("Hey".to_string(), "quelle heure est-il ?".to_string()))
        );
        assert_eq!(
            extract_wake_invocation("Hé Nestor ! Peux-tu lancer les tests ?"),
            Some(("Hé".to_string(), "Peux-tu lancer les tests ?".to_string()))
        );
        assert_eq!(
            extract_wake_invocation("Dis-moi Nestor, où est le fichier config ?"),
            Some(("Dis-moi".to_string(), "où est le fichier config ?".to_string()))
        );
        assert_eq!(
            extract_wake_invocation("Bonjour Nestor, comment vas-tu ?"),
            Some(("Bonjour".to_string(), "comment vas-tu ?".to_string()))
        );
        assert_eq!(
            extract_wake_invocation("Euh, hey Nestor, regarde les logs"),
            Some(("Euh, hey".to_string(), "regarde les logs".to_string()))
        );

        // 3rd-person mentions (must NOT trigger)
        assert_eq!(extract_wake_invocation("Est-ce que Nestor a fini ?"), None);
        assert_eq!(extract_wake_invocation("J'ai vu Nestor ce matin."), None);
        assert_eq!(extract_wake_invocation("Selon Nestor, la meteo est bonne."), None);

        // Completely unrelated speech (must NOT trigger)
        assert_eq!(extract_wake_invocation("Passe-moi le sel s'il te plait."), None);
        assert_eq!(extract_wake_invocation("Il fait beau aujourd'hui."), None);
    }

    #[test]
    fn test_wake_detector_flow() {
        let active_until = Arc::new(AtomicU64::new(0));
        let detector = WakeDetector::new(true, 15, active_until.clone(), "Monsieur".to_string(), None);

        let now = 1_000_000;

        // 1. Initial standby: unrelated utterance is ignored
        assert_eq!(detector.evaluate("Il fait beau aujourd'hui", now), WakeAction::Ignored);
        assert!(!detector.is_active(now));

        // 2. User says "Hey Nestor" alone -> WakeOnly
        let action = detector.evaluate("Hey Nestor !", now);
        assert_eq!(action, WakeAction::WakeOnly { ack_phrase: "Oui, Monsieur ?".to_string() });
        assert!(detector.is_active(now));
        assert_eq!(active_until.load(Ordering::SeqCst), now + 15_000);

        // 3. During active window (e.g. 5s later), user speaks without wake word -> FollowUp
        let follow_up = detector.evaluate("Quelle heure est-il ?", now + 5_000);
        assert_eq!(follow_up, WakeAction::FollowUp { text: "Quelle heure est-il ?".to_string() });
        assert_eq!(active_until.load(Ordering::SeqCst), now + 20_000);

        // 4. After 25s (timeout expired), user speaks without wake word -> Ignored
        let expired = detector.evaluate("Et demain ?", now + 30_000);
        assert_eq!(expired, WakeAction::Ignored);

        // 5. One-shot command works from standby
        let oneshot = detector.evaluate("Hey Nestor, prépare le build", now + 35_000);
        assert_eq!(oneshot, WakeAction::Command { query: "prépare le build".to_string() });
    }
}
