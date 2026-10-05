//! Annulation d'echo (AEC3 de WebRTC, via `sonora`, en Rust pur) avec le signal
//! de reference : ce que Nestor envoie aux haut-parleurs.
//!
//! Pendant que Nestor parle, la parole detectee au micro sert a l'interrompre
//! (barge-in vocal). Sans annulation d'echo, sa propre voix, renvoyee par les
//! haut-parleurs, declencherait cette interruption. Les clients annulent deja
//! l'echo (navigateur, Android) ; cette seconde passe, cote serveur, retire ce
//! qu'il reste, en connaissant exactement le signal joue.
//!
//! Synchronisation : le reference avance au rythme des trames micro (10 ms de
//! reference consommees par 10 ms de micro), comme un flux rendu/capture
//! classique ; l'AEC estime lui-meme le retard de lecture. Cela suppose que les
//! clients envoient le micro en continu pendant la lecture.

use std::collections::VecDeque;
use std::sync::Mutex;

use sonora::config::EchoCanceller as EchoConfig;
use sonora::{AudioProcessing, Config, StreamConfig};

/// Cadence de traitement : 10 ms a 16 kHz.
pub const FRAME_SAMPLES: usize = 160;
const SAMPLE_RATE: u32 = 16_000;

/// File des echantillons (16 kHz) que les clients sont en train de lire.
#[derive(Default)]
pub struct Reference {
    queue: Mutex<VecDeque<f32>>,
}

impl Reference {
    /// Empile une phrase synthetisee (rate d'origine quelconque, rechantillonnee a 16 kHz).
    pub fn push(&self, waveform: &[f32], sample_rate: u32) {
        let ratio = sample_rate as f64 / SAMPLE_RATE as f64;
        let n = (waveform.len() as f64 / ratio) as usize;
        let mut guard = self.queue.lock().unwrap();
        for i in 0..n {
            // Interpolation lineaire.
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = waveform[idx];
            let b = *waveform.get(idx + 1).unwrap_or(&a);
            guard.push_back(a + (b - a) * frac);
        }
    }

    /// Retire une trame de reference (silence si plus rien n'est en cours de lecture).
    pub fn pop_frame(&self) -> [f32; FRAME_SAMPLES] {
        let mut frame = [0f32; FRAME_SAMPLES];
        let mut guard = self.queue.lock().unwrap();
        for s in frame.iter_mut() {
            *s = guard.pop_front().unwrap_or(0.0);
        }
        frame
    }

    /// Abandonne ce qui reste a lire (interruption, fin de lecture).
    pub fn clear(&self) {
        self.queue.lock().unwrap().clear();
    }
}

pub struct EchoCanceller {
    apm: AudioProcessing,
}

impl EchoCanceller {
    pub fn new() -> Self {
        let config = Config { echo_canceller: Some(EchoConfig::default()), ..Default::default() };
        let apm = AudioProcessing::builder()
            .config(config)
            .capture_config(StreamConfig::new(SAMPLE_RATE, 1))
            .render_config(StreamConfig::new(SAMPLE_RATE, 1))
            .build();
        Self { apm }
    }

    /// Retire de `mic` l'echo de `reference` (une trame de 10 ms chacun). En cas
    /// d'erreur interne, rend le micro tel quel.
    pub fn process(&mut self, mic: &[f32], reference: &[f32; FRAME_SAMPLES]) -> [f32; FRAME_SAMPLES] {
        let mut out = [0f32; FRAME_SAMPLES];
        let mut render_out = [0f32; FRAME_SAMPLES];
        if self.apm.process_render_f32(&[&reference[..]], &mut [&mut render_out[..]]).is_err()
            || self.apm.process_capture_f32(&[mic], &mut [&mut out[..]]).is_err()
        {
            out.copy_from_slice(mic);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// Voix synthetique : harmoniques modulees, assez riche pour que l'AEC converge.
    fn voice(n: usize, seed: f32) -> Vec<f32> {
        let mut x = 0x1234_5678u32 ^ (seed as u32);
        (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = ((x >> 16) as f32 / 32768.0 - 1.0) * 0.05;
                let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin().abs();
                env * (0.25 * (2.0 * std::f32::consts::PI * 180.0 * t).sin()
                    + 0.15 * (2.0 * std::f32::consts::PI * 540.0 * t).sin()
                    + 0.1 * (2.0 * std::f32::consts::PI * 1700.0 * t).sin())
                    + noise
            })
            .collect()
    }

    #[test]
    fn reference_resample_et_vidage() {
        let r = Reference::default();
        r.push(&vec![0.5; 22_050], 22_050);
        assert!((r.pop_frame()[0] - 0.5).abs() < 1e-6);
        r.clear();
        assert_eq!(r.pop_frame(), [0f32; FRAME_SAMPLES]);
    }

    #[test]
    fn attenue_un_echo_retarde() {
        // Echo = reference retardee de 120 ms et attenuee : apres convergence, le
        // residu doit etre nettement plus faible que le micro brut.
        let seconds = 8;
        let n = SAMPLE_RATE as usize * seconds;
        let render = voice(n, 1.0);
        let delay = (0.12 * SAMPLE_RATE as f32) as usize;
        let mic: Vec<f32> = (0..n).map(|i| if i >= delay { 0.4 * render[i - delay] } else { 0.0 }).collect();

        let mut aec = EchoCanceller::new();
        let (mut raw_tail, mut clean_tail) = (Vec::new(), Vec::new());
        for f in 0..n / FRAME_SAMPLES {
            let range = f * FRAME_SAMPLES..(f + 1) * FRAME_SAMPLES;
            let mut rf = [0f32; FRAME_SAMPLES];
            rf.copy_from_slice(&render[range.clone()]);
            let out = aec.process(&mic[range.clone()], &rf);
            if f >= (n / FRAME_SAMPLES) * 3 / 4 {
                raw_tail.extend_from_slice(&mic[range]);
                clean_tail.extend_from_slice(&out);
            }
        }
        let erle_db = 20.0 * (rms(&raw_tail) / rms(&clean_tail).max(1e-9)).log10();
        println!("ERLE mesure : {erle_db:.1} dB");
        assert!(erle_db > 10.0, "echo insuffisamment annule : {erle_db:.1} dB");
    }
}
