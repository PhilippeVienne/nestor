//! Filtre d'auto-ecoute : Nestor ne doit pas traiter sa propre voix, renvoyee
//! par les haut-parleurs vers le micro, comme une parole de l'utilisateur.
//!
//! La suppression d'echo par le temps (`speaking_until_ms`) ne suffit pas : la
//! latence de lecture des clients et la reverberation font arriver l'echo apres
//! la fin de la fenetre estimee. Whisper le transcrit alors comme du texte, et
//! la fenetre conversationnelle active le prend pour une suite de dialogue.
//!
//! On garde donc les phrases recemment synthetisees : un enonce transcrit qui
//! les recopie (mots communs, enchainements identiques) est un echo.

use std::collections::VecDeque;
use std::sync::Mutex;

/// Duree pendant laquelle une phrase prononcee reste susceptible de revenir en echo
/// (lecture d'une longue reponse comprise).
const WINDOW_MS: u64 = 90_000;
const MAX_SENTENCES: usize = 16;
/// En dessous, l'enonce est trop court pour conclure ("oui", "d'accord"...).
const MIN_TOKENS: usize = 4;
const UNIGRAM_RATIO: f32 = 0.8;
const BIGRAM_RATIO: f32 = 0.5;
const MIN_BIGRAMS: usize = 3;
/// Un mot de cette longueur ou plus est « porteur de sens » (hors mots-outils).
const CONTENT_LEN: usize = 5;

#[derive(Default)]
pub struct RecentSpeech {
    sentences: Mutex<VecDeque<(u64, Vec<String>)>>,
}

fn tokenize(text: &str) -> Vec<String> {
    // L'apostrophe fait partie du mot ("c'est" -> "cest") : les deux transcriptions
    // d'une meme phrase ne doivent pas diverger sur la graphie des elisions.
    text.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '\u{2019}'))
        .filter(|w| !w.is_empty())
        .map(|w| {
            w.chars()
                .filter(|c| *c != '\'' && *c != '\u{2019}')
                .flat_map(char::to_lowercase)
                .map(|c| match c {
                    'é' | 'è' | 'ê' | 'ë' => 'e',
                    'à' | 'â' | 'ä' => 'a',
                    'ô' | 'ö' => 'o',
                    'î' | 'ï' => 'i',
                    'û' | 'ù' | 'ü' => 'u',
                    'ç' => 'c',
                    c => c,
                })
                .collect()
        })
        .collect()
}

impl RecentSpeech {
    /// Memorise une phrase que Nestor s'apprete a prononcer.
    pub fn record(&self, now_ms: u64, sentence: &str) {
        let tokens = tokenize(sentence);
        if tokens.is_empty() {
            return;
        }
        let mut guard = self.sentences.lock().unwrap();
        guard.push_back((now_ms, tokens));
        while guard.len() > MAX_SENTENCES {
            guard.pop_front();
        }
    }

    /// `heard` (texte transcrit) recopie-t-il ce que Nestor vient de dire ?
    pub fn is_echo(&self, now_ms: u64, heard: &str) -> bool {
        let heard = tokenize(heard);
        if heard.len() < MIN_TOKENS {
            return false;
        }

        let guard = self.sentences.lock().unwrap();
        let spoken: Vec<&Vec<String>> = guard
            .iter()
            .filter(|(ts, _)| now_ms.saturating_sub(*ts) <= WINDOW_MS)
            .map(|(_, t)| t)
            .collect();
        if spoken.is_empty() {
            return false;
        }

        let words: std::collections::HashSet<&str> = spoken.iter().flat_map(|t| t.iter().map(String::as_str)).collect();
        let bigrams: std::collections::HashSet<(&str, &str)> = spoken
            .iter()
            .flat_map(|t| t.windows(2).map(|w| (w[0].as_str(), w[1].as_str())))
            .collect();

        let common: Vec<&String> = heard.iter().filter(|w| words.contains(w.as_str())).collect();
        let unigram = common.len() as f32 / heard.len() as f32;
        let has_content = common.iter().any(|w| w.chars().count() >= CONTENT_LEN);
        if unigram >= UNIGRAM_RATIO && has_content {
            return true;
        }

        let heard_bigrams: Vec<(&str, &str)> = heard.windows(2).map(|w| (w[0].as_str(), w[1].as_str())).collect();
        heard_bigrams.len() >= MIN_BIGRAMS
            && heard_bigrams.iter().filter(|b| bigrams.contains(*b)).count() as f32 / heard_bigrams.len() as f32 >= BIGRAM_RATIO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPOKEN: &str = "Il me semble que nous avons échangé nos rôles : c'est moi, Nestor, qui suis votre majordome.";

    fn speech() -> RecentSpeech {
        let r = RecentSpeech::default();
        r.record(1_000, SPOKEN);
        r
    }

    #[test]
    fn echo_deforme_par_whisper() {
        assert!(speech().is_echo(5_000, "C'est moi qui devenais Nestor."));
        assert!(speech().is_echo(5_000, "il me semble que nous avons échangé nos rôles"));
    }

    #[test]
    fn vraie_parole_de_l_utilisateur() {
        let r = speech();
        assert!(!r.is_echo(5_000, "Quelle heure est-il maintenant ?"));
        assert!(!r.is_echo(5_000, "L'interruption elle ne marche pas très bien."));
        // Trop court pour conclure, meme avec des mots communs.
        assert!(!r.is_echo(5_000, "oui c'est moi"));
    }

    #[test]
    fn mots_outils_seuls_ne_suffisent_pas() {
        let r = RecentSpeech::default();
        r.record(0, "je vais ouvrir le fichier de configuration pour vous");
        assert!(!r.is_echo(1_000, "je veux que tu me le dises pour moi"));
    }

    #[test]
    fn echo_hors_fenetre_ou_sans_historique() {
        assert!(!speech().is_echo(1_000 + WINDOW_MS + 1, "C'est moi qui devenais Nestor."));
        assert!(!RecentSpeech::default().is_echo(0, "C'est moi qui devenais Nestor."));
    }
}
