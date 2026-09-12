//! Pipeline VAD/STT/TTS local de Nestor.
//!
//! La capture micro et la lecture audio sont geree par le front (Antigravity).
//! Entree : frames binaires WebSocket PCM16LE 16 kHz (voir `ws.rs`). Sortie :
//! evenement JSON `audio_chunk` (PCM16 base64) plutot qu'une frame binaire
//! brute, car le front interprete toujours les frames binaires recues comme
//! du WAV (cf. `docs/audio-protocol.md` pour le detail de cette decision).
//!
//! Le calcul (ONNX Runtime / whisper.cpp, tous deux bloquants) tourne sur des
//! threads OS dedies plutot que sur l'executeur tokio.

mod stt;
mod tts;
mod vad;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use tokio::sync::{broadcast, mpsc};

use crate::claude_process::ClaudeHandle;
use crate::protocol::{DaemonStatus, Role, ServerEvent};

/// Frequence d'echantillonnage attendue pour les frames binaires micro
/// envoyees par le front (PCM16LE mono).
pub const MIC_SAMPLE_RATE: u32 = 16_000;

/// Nombre d'echantillons par fenetre d'analyse Silero VAD (32 ms a 16 kHz).
const VAD_CHUNK_SAMPLES: usize = 512;
/// Seuil de probabilite au-dessus duquel une fenetre est consideree comme de la parole.
const VAD_THRESHOLD: f32 = 0.5;
/// Duree de silence apres laquelle on considere l'enonce utilisateur termine.
const SILENCE_HANGOVER_MS: u64 = 700;
/// Duree minimale de parole avant de declencher une transcription (evite le bruit court).
const MIN_SPEECH_MS: u64 = 250;
/// Marge ajoutee a la fenetre de suppression d'echo. Le serveur ne connait pas
/// l'instant reel de lecture cote front (reseau + decodage + planification
/// Web Audio), donc la fenetre demarre a l'envoi et cette marge absorbe le
/// decalage. La protection principale reste la coupure du micro cote front
/// pendant la lecture.
const ECHO_TAIL_MS: u64 = 1_200;
/// Niveau RMS minimal d'un enonce pour etre transcrit. L'echo residuel et le
/// bruit de fond sont nettement plus faibles que la voix directe, et Whisper
/// hallucine volontiers des phrases entieres sur ce genre de signal.
const MIN_UTTERANCE_RMS: f32 = 0.01;
/// Audio conserve avant le declenchement de la VAD. Le premier phoneme est
/// souvent plus faible que le seuil : sans ce pre-roll, l'enonce transcrit
/// commence apres l'attaque du mot.
const PREROLL_SAMPLES: usize = 4_800; // 300 ms a 16 kHz

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn models_dir() -> PathBuf {
    std::env::var("NESTORD_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".local/share/nestord/models")
        })
}

/// Demarre le pipeline VAD/STT (thread dedie, consomme `mic_rx`) et le
/// pipeline TTS (thread dedie, consomme `tts_rx` et publie des `AudioChunk`
/// JSON sur `events_tx`).
pub fn spawn(
    events_tx: broadcast::Sender<ServerEvent>,
    claude: ClaudeHandle,
    mic_rx: mpsc::Receiver<Vec<u8>>,
    tts_rx: mpsc::UnboundedReceiver<String>,
    barge_in_gen: Arc<AtomicU64>,
    speaking_until_ms: Arc<AtomicU64>,
) -> Result<()> {
    let models = models_dir();

    let vad_model = models.join("silero_vad.onnx");
    let whisper_model = models.join("ggml-large-v3-turbo-q5_0.bin");
    anyhow::ensure!(vad_model.exists(), "modele VAD introuvable: {}", vad_model.display());
    anyhow::ensure!(whisper_model.exists(), "modele Whisper introuvable: {}", whisper_model.display());

    {
        let events_tx = events_tx.clone();
        let speaking_until_ms = speaking_until_ms.clone();
        std::thread::spawn(move || {
            if let Err(err) =
                listen_loop(events_tx, claude, mic_rx, &vad_model, &whisper_model, speaking_until_ms)
            {
                tracing::error!(?err, "pipeline d'ecoute (VAD/STT) interrompu");
            }
        });
    }

    {
        let piper_dir = models.join("piper");
        std::thread::spawn(move || {
            if let Err(err) = speak_loop(events_tx, tts_rx, &piper_dir, barge_in_gen, speaking_until_ms) {
                tracing::error!(?err, "pipeline de synthese vocale (TTS) interrompu");
            }
        });
    }

    Ok(())
}

/// Boucle reception PCM16 (front) -> VAD -> accumulation -> STT -> envoi a Claude.
fn listen_loop(
    events_tx: broadcast::Sender<ServerEvent>,
    claude: ClaudeHandle,
    mut mic_rx: mpsc::Receiver<Vec<u8>>,
    vad_model: &std::path::Path,
    whisper_model: &std::path::Path,
    speaking_until_ms: Arc<AtomicU64>,
) -> Result<()> {
    let mut vad = vad::SileroVad::load(vad_model).context("chargement Silero VAD")?;
    let whisper = stt::WhisperStt::load(whisper_model).context("chargement Whisper")?;
    // Langue forcee par defaut : la detection automatique se trompe souvent
    // sur des enonces courts et produit alors de l'anglais invente.
    let stt_lang = std::env::var("NESTORD_STT_LANG").unwrap_or_else(|_| "fr".to_string());

    let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Listening });

    let mut pending: Vec<f32> = Vec::new();
    let mut speech_buf: Vec<f32> = Vec::new();
    let mut preroll: Vec<f32> = Vec::new();
    let mut in_speech = false;
    let mut silence_run_ms: u64 = 0;
    let mut speech_run_ms: u64 = 0;

    let mut frames_received: u64 = 0;
    let mut windows_analyzed: u64 = 0;
    let mut was_suppressing = false;

    while let Some(bytes) = mic_rx.blocking_recv() {
        frames_received += 1;

        // Suppression d'echo : tant que la synthese est censee etre en cours de
        // lecture, on jette l'entree micro et on repart d'un buffer propre.
        if now_ms() < speaking_until_ms.load(Ordering::SeqCst) {
            pending.clear();
            speech_buf.clear();
            preroll.clear();
            in_speech = false;
            silence_run_ms = 0;
            speech_run_ms = 0;
            was_suppressing = true;
            continue;
        }

        // La transcription Whisper est synchrone : pendant qu'elle tourne (et
        // pendant la lecture), les frames s'accumulent dans le channel. A la
        // sortie de la fenetre de suppression, ce backlog contient encore de
        // l'echo qu'il ne faut pas analyser.
        if was_suppressing {
            was_suppressing = false;
            let mut dropped = 0u32;
            while mic_rx.try_recv().is_ok() {
                dropped += 1;
            }
            tracing::debug!(dropped, "backlog micro purge apres la fenetre de suppression d'echo");
            continue;
        }

        let decoded = pcm16_bytes_to_f32(&bytes);
        if frames_received % 50 == 1 {
            let peak = decoded.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
            let rms = if decoded.is_empty() {
                0.0
            } else {
                (decoded.iter().map(|s| s * s).sum::<f32>() / decoded.len() as f32).sqrt()
            };
            tracing::debug!(frames_received, len = bytes.len(), peak, rms, "frames audio micro en cours de reception");
        }
        pending.extend(decoded);

        while pending.len() >= VAD_CHUNK_SAMPLES {
            let window: Vec<f32> = pending.drain(..VAD_CHUNK_SAMPLES).collect();
            let window_ms = (VAD_CHUNK_SAMPLES as f64 / MIC_SAMPLE_RATE as f64 * 1000.0) as u64;

            let prob = match vad.process(&window) {
                Ok(p) => p,
                Err(err) => {
                    tracing::error!(?err, "erreur VAD, fenetre ignoree");
                    continue;
                }
            };

            windows_analyzed += 1;
            if windows_analyzed % 20 == 1 {
                tracing::debug!(prob, windows_analyzed, "fenetre VAD analysee");
            }

            if prob >= VAD_THRESHOLD {
                if !in_speech {
                    tracing::info!(prob, "debut de parole detecte");
                    // Reprend le pre-roll pour ne pas amputer l'attaque du mot.
                    speech_buf.extend_from_slice(&preroll);
                }
                speech_run_ms += window_ms;
                silence_run_ms = 0;
                in_speech = true;
                speech_buf.extend_from_slice(&window);
            } else if !in_speech {
                // Silence : on garde juste les dernieres fenetres en reserve.
                preroll.extend_from_slice(&window);
                if preroll.len() > PREROLL_SAMPLES {
                    preroll.drain(..preroll.len() - PREROLL_SAMPLES);
                }
            } else {
                silence_run_ms += window_ms;
                speech_buf.extend_from_slice(&window);

                if silence_run_ms >= SILENCE_HANGOVER_MS {
                    in_speech = false;
                    let utterance = std::mem::take(&mut speech_buf);
                    let had_speech_ms = speech_run_ms;
                    speech_run_ms = 0;
                    silence_run_ms = 0;

                    tracing::info!(had_speech_ms, utterance_samples = utterance.len(), "fin de parole detectee (silence)");

                    if had_speech_ms < MIN_SPEECH_MS {
                        tracing::debug!(had_speech_ms, "enonce trop court, ignore");
                        continue;
                    }

                    let rms = (utterance.iter().map(|s| s * s).sum::<f32>()
                        / utterance.len().max(1) as f32)
                        .sqrt();
                    if rms < MIN_UTTERANCE_RMS {
                        tracing::debug!(rms, "enonce trop faible (echo/bruit probable), ignore");
                        continue;
                    }

                    let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Thinking });
                    let transcription = whisper.transcribe(&utterance, &stt_lang);
                    tracing::debug!(?transcription, "resultat de la transcription whisper");
                    match transcription {
                        Ok(text) if !text.trim().is_empty() => {
                            let text = text.trim().to_string();
                            let _ = events_tx.send(ServerEvent::Transcript {
                                role: Role::User,
                                delta: None,
                                text: text.clone(),
                                is_final: Some(true),
                            });
                            if let Err(err) = claude.send_user_message_blocking(&text) {
                                tracing::error!(?err, "echec d'envoi de la transcription a claude");
                            }
                        }
                        Ok(_) => {
                            let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Listening });
                        }
                        Err(err) => {
                            tracing::error!(?err, "echec de transcription whisper");
                            let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Listening });
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Boucle reception des segments de texte -> synthese Piper -> envoi d'un
/// `AudioChunk` JSON (PCM16 base64) au front, avec prise en charge du
/// barge-in (abandon des segments d'un tour interrompu).
fn speak_loop(
    events_tx: broadcast::Sender<ServerEvent>,
    mut tts_rx: mpsc::UnboundedReceiver<String>,
    piper_dir: &std::path::Path,
    barge_in_gen: Arc<AtomicU64>,
    speaking_until_ms: Arc<AtomicU64>,
) -> Result<()> {
    let voice = std::env::var("NESTORD_TTS_VOICE").unwrap_or_else(|_| "fr_FR-upmc-medium".to_string());
    let speaker_id = std::env::var("NESTORD_TTS_SPEAKER")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1); // locuteur 1 du modele upmc = Pierre (masculin)

    let synth = tts::PiperTts::load(piper_dir, &voice, speaker_id).context("chargement Piper TTS")?;
    let sample_rate = synth.sample_rate();

    while let Some(sentence) = tts_rx.blocking_recv() {
        let start_gen = barge_in_gen.load(Ordering::SeqCst);

        let waveform = match synth.synthesize(&sentence) {
            Ok(w) => w,
            Err(err) => {
                tracing::error!(?err, %sentence, "echec de synthese TTS, segment ignore");
                continue;
            }
        };

        if barge_in_gen.load(Ordering::SeqCst) != start_gen {
            speaking_until_ms.store(0, Ordering::SeqCst);
            continue; // le tour a ete interrompu pendant la synthese elle-meme
        }

        // Le front joue les segments les uns apres les autres : on empile les
        // durees pour couvrir toute la file de lecture, pas seulement ce segment.
        let duration_ms = (waveform.len() as u64 * 1000) / sample_rate.max(1) as u64;
        let playback_start = speaking_until_ms.load(Ordering::SeqCst).max(now_ms());
        speaking_until_ms.store(playback_start + duration_ms + ECHO_TAIL_MS, Ordering::SeqCst);

        let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Speaking });
        let pcm_base64 = base64::engine::general_purpose::STANDARD.encode(f32_to_pcm16_bytes(&waveform));
        let _ = events_tx.send(ServerEvent::AudioChunk {
            data: pcm_base64,
            format: "pcm16".to_string(),
            sample_rate,
        });

        // Ne repasse en ecoute que si aucun nouveau segment n'est deja en attente
        // (evite un flicker d'etat entre deux phrases d'une meme reponse).
        if tts_rx.is_empty() {
            let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Listening });
        }
    }

    Ok(())
}

fn pcm16_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect()
}

fn f32_to_pcm16_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}
