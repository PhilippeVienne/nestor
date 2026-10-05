//! Detection de fin de tour via Smart Turn v3 (pipecat-ai/smart-turn, ONNX, BSD-2).
//!
//! Le VAD seul ne sait pas dire si l'utilisateur a *fini* sa phrase ou s'il
//! reflechit : un silence fixe coupe trop tot ou fait attendre inutilement.
//! Smart Turn recoit les 8 dernieres secondes d'audio et renvoie la
//! probabilite que le tour de parole soit termine.
//!
//! Entree du modele : `input_features` [1, 80, 800], un log-mel Whisper (80 mels,
//! n_fft 400, hop 160) de 8 s a 16 kHz, audio normalise (moyenne nulle, variance
//! unitaire) et complete par des zeros *au debut*. Sortie : `logits` [1, 1], deja
//! une probabilite (sigmoide integree au modele).

use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use ort::session::Session;
use ort::value::Tensor;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

const SAMPLE_RATE: usize = 16_000;
const WINDOW_SECS: usize = 8;
const WINDOW_SAMPLES: usize = SAMPLE_RATE * WINDOW_SECS;
const N_FFT: usize = 400;
const HOP: usize = 160;
const N_MELS: usize = 80;
const N_FREQS: usize = N_FFT / 2 + 1;
/// Trames du spectrogramme : 800 pour 8 s (la derniere, 801e, est ignoree).
const N_FRAMES: usize = WINDOW_SAMPLES / HOP;

/// Echelle mel de Slaney (celle de Whisper) : lineaire jusqu'a 1 kHz, puis logarithmique.
fn hz_to_mel(f: f64) -> f64 {
    if f >= 1000.0 { 15.0 + (f / 1000.0).ln() / (6.4f64.ln() / 27.0) } else { f / (200.0 / 3.0) }
}

fn mel_to_hz(m: f64) -> f64 {
    if m >= 15.0 { 1000.0 * ((6.4f64.ln() / 27.0) * (m - 15.0)).exp() } else { m * (200.0 / 3.0) }
}

/// Banc de filtres mel triangulaires, normalises (Slaney) : [N_MELS][N_FREQS] aplati.
fn mel_filters() -> Vec<f32> {
    let freqs: Vec<f64> = (0..N_FREQS).map(|i| i as f64 * (SAMPLE_RATE as f64 / 2.0) / (N_FREQS - 1) as f64).collect();
    let (lo_mel, hi_mel) = (hz_to_mel(0.0), hz_to_mel(SAMPLE_RATE as f64 / 2.0));
    let pts: Vec<f64> = (0..N_MELS + 2).map(|i| mel_to_hz(lo_mel + (hi_mel - lo_mel) * i as f64 / (N_MELS + 1) as f64)).collect();
    let mut fb = vec![0f32; N_MELS * N_FREQS];
    for m in 0..N_MELS {
        let (lo, ce, hi) = (pts[m], pts[m + 1], pts[m + 2]);
        for (k, &f) in freqs.iter().enumerate() {
            let up = (f - lo) / (ce - lo);
            let down = (hi - f) / (hi - ce);
            fb[m * N_FREQS + k] = (up.min(down).max(0.0) * (2.0 / (hi - lo))) as f32;
        }
    }
    fb
}

/// Extracteur de caracteristiques identique au `WhisperFeatureExtractor` de Hugging Face.
pub struct FeatureExtractor {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    filters: Vec<f32>,
}

impl FeatureExtractor {
    pub fn new() -> Self {
        // Fenetre de Hann periodique.
        let window = (0..N_FFT).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / N_FFT as f32).cos()).collect();
        Self { fft: FftPlanner::new().plan_fft_forward(N_FFT), window, filters: mel_filters() }
    }

    /// Retourne le log-mel [N_MELS * N_FRAMES] (ligne = bande mel) des 8 dernieres secondes de `pcm`.
    pub fn extract(&self, pcm: &[f32]) -> Vec<f32> {
        let tail = &pcm[pcm.len().saturating_sub(WINDOW_SAMPLES)..];
        let mut audio = vec![0f32; WINDOW_SAMPLES - tail.len()];
        audio.extend_from_slice(tail);

        let n = audio.len() as f32;
        let mean = audio.iter().sum::<f32>() / n;
        let var = audio.iter().map(|s| (s - mean) * (s - mean)).sum::<f32>() / n;
        let inv_std = 1.0 / (var + 1e-7).sqrt();
        for s in audio.iter_mut() {
            *s = (*s - mean) * inv_std;
        }

        // Padding "reflect" de N_FFT/2 de chaque cote (centrage des trames).
        let pad = N_FFT / 2;
        let mut padded = Vec::with_capacity(audio.len() + 2 * pad);
        padded.extend((1..=pad).rev().map(|i| audio[i]));
        padded.extend_from_slice(&audio);
        padded.extend((1..=pad).map(|i| audio[audio.len() - 1 - i]));

        let mut log_mel = vec![0f32; N_MELS * N_FRAMES];
        let mut buf = vec![Complex::new(0f32, 0f32); N_FFT];
        let mut power = vec![0f32; N_FREQS];
        for frame in 0..N_FRAMES {
            let start = frame * HOP;
            for (i, c) in buf.iter_mut().enumerate() {
                *c = Complex::new(padded[start + i] * self.window[i], 0.0);
            }
            self.fft.process(&mut buf);
            for (k, p) in power.iter_mut().enumerate() {
                *p = buf[k].norm_sqr();
            }
            for m in 0..N_MELS {
                let row = &self.filters[m * N_FREQS..(m + 1) * N_FREQS];
                let mel: f32 = row.iter().zip(&power).map(|(f, p)| f * p).sum();
                log_mel[m * N_FRAMES + frame] = mel.max(1e-10).log10();
            }
        }

        // Compression dynamique de Whisper : plancher a max - 8, puis mise a l'echelle.
        let max = log_mel.iter().cloned().fold(f32::MIN, f32::max);
        for v in log_mel.iter_mut() {
            *v = (v.max(max - 8.0) + 4.0) / 4.0;
        }
        log_mel
    }
}

pub struct SmartTurn {
    session: Session,
    features: FeatureExtractor,
}

impl SmartTurn {
    /// Charge le modele (variante CPU int8, ~8 Mo : quelques dizaines de ms par inference).
    pub fn load(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("creation du builder ONNX Runtime: {e}"))?
            .commit_from_file(model_path)
            .with_context(|| format!("chargement du modele {}", model_path.display()))?;
        Ok(Self { session, features: FeatureExtractor::new() })
    }

    /// Probabilite [0, 1] que le tour de parole soit termine, d'apres l'audio 16 kHz mono recu.
    pub fn predict(&mut self, pcm: &[f32]) -> Result<f32> {
        let input = Tensor::from_array((vec![1i64, N_MELS as i64, N_FRAMES as i64], self.features.extract(pcm)))?;
        let outputs = self.session.run(ort::inputs!["input_features" => input])?;
        let (_, prob) = outputs["logits"].try_extract_tensor::<f32>()?;
        Ok(prob.first().copied().unwrap_or(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Signal deterministe : meme formule cote numpy pour generer les valeurs attendues.
    fn test_signal() -> Vec<f32> {
        (0..24_000)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let burst = if i % 4000 < 2000 { 1.0 } else { 0.0 };
                0.3 * (2.0 * PI * 440.0 * t).sin()
                    + 0.2 * (2.0 * PI * 1234.5 * t).sin()
                    + 0.1 * (2.0 * PI * 3000.0 * t).sin() * burst
            })
            .collect()
    }

    #[test]
    fn dimensions_du_log_mel() {
        let f = FeatureExtractor::new().extract(&test_signal());
        assert_eq!(f.len(), N_MELS * N_FRAMES);
        assert!(f.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn log_mel_conforme_a_la_reference_numpy() {
        let f = FeatureExtractor::new().extract(&test_signal());
        let at = |m: usize, t: usize| f[m * N_FRAMES + t];
        for (m, t, expected) in REFERENCE {
            let got = at(*m, *t);
            assert!((got - expected).abs() < 5e-3, "mel {m} trame {t} : {got} != {expected}");
        }
    }

    // (bande mel, trame, valeur) calculees par numpy (meme algorithme que le WhisperFeatureExtractor de Hugging Face).
    const REFERENCE: &[(usize, usize, f32)] = &[
        (0, 0, -0.19826),
        (5, 799, 0.85708),
        (10, 700, 1.71225),
        (20, 720, -0.19826),
        (30, 760, 1.50377),
        (40, 790, -0.19826),
        (55, 730, 1.41544),
        (79, 799, -0.19826),
        (12, 650, 1.59839),
        (33, 799, 1.36416),
    ];
}
