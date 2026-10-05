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

mod echo;
mod stt;
mod tts;
mod vad;
pub mod turn;
pub mod wake;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use tokio::sync::{broadcast, mpsc};

use crate::brain::NestorBrain;
use crate::config::Config;
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
/// Pas entre deux consultations du modele de fin de tour pendant un silence.
const TURN_CHECK_STEP_MS: u64 = 160;
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
    brain: Arc<NestorBrain>,
    mic_rx: mpsc::Receiver<Vec<u8>>,
    tts_rx: mpsc::UnboundedReceiver<String>,
    tts_tx: mpsc::UnboundedSender<String>,
    barge_in_gen: Arc<AtomicU64>,
    turn_started_gen: Arc<AtomicU64>,
    speaking_until_ms: Arc<AtomicU64>,
    wake_active_until_ms: Arc<AtomicU64>,
    config: Arc<Config>,
) -> Result<()> {
    let models = models_dir();
    let recent_speech = Arc::new(echo::RecentSpeech::default());

    let vad_model = models.join("silero_vad.onnx");
    let whisper_model = models.join("ggml-large-v3-turbo-q5_0.bin");
    anyhow::ensure!(vad_model.exists(), "modele VAD introuvable: {}", vad_model.display());
    anyhow::ensure!(whisper_model.exists(), "modele Whisper introuvable: {}", whisper_model.display());

    // Ticker asynchrone de gestion de l'expiration du mot-cle (veille automatique)
    {
        let events_tx = events_tx.clone();
        let mut events_rx = events_tx.subscribe();
        let wake_active_until_ms = wake_active_until_ms.clone();
        let wake_enabled = config.wake_word.enabled;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            let mut was_active = false;
            // Dernier etat du daemon, pour ne retomber en veille que s'il n'est
            // ni en train de reflechir ni de parler (reponse plus longue que la fenetre).
            let mut last_status = DaemonStatus::Idle;
            loop {
                interval.tick().await;
                loop {
                    match events_rx.try_recv() {
                        Ok(ServerEvent::State { status }) => last_status = status,
                        Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                }
                if !wake_enabled {
                    continue;
                }
                let is_active = now_ms() < wake_active_until_ms.load(Ordering::SeqCst);
                if was_active && !is_active {
                    was_active = false;
                    tracing::info!("Fenetre conversationnelle expiree : Nestor se remet en veille (mot-cle 'Hey Nestor' requis)");
                    let _ = events_tx.send(ServerEvent::WakeState { active: false });
                    if last_status == DaemonStatus::Listening {
                        let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Idle });
                    }
                } else if !was_active && is_active {
                    was_active = true;
                }
            }
        });
    }

    {
        let events_tx = events_tx.clone();
        let speaking_until_ms = speaking_until_ms.clone();
        let wake_active_until_ms = wake_active_until_ms.clone();
        let barge_in_gen = barge_in_gen.clone();
        let turn_started_gen = turn_started_gen.clone();
        let recent_speech = recent_speech.clone();
        let config = config.clone();
        std::thread::spawn(move || {
            if let Err(err) = listen_loop(
                events_tx,
                brain,
                mic_rx,
                tts_tx,
                &vad_model,
                &whisper_model,
                speaking_until_ms,
                wake_active_until_ms,
                barge_in_gen,
                turn_started_gen,
                recent_speech,
                config,
            ) {
                tracing::error!(?err, "pipeline d'ecoute (VAD/STT) interrompu");
            }
        });
    }

    {
        let piper_dir = models.join("piper");
        let wake_active_until_ms = wake_active_until_ms.clone();
        let config = config.clone();
        std::thread::spawn(move || {
            if let Err(err) = speak_loop(
                events_tx,
                tts_rx,
                &piper_dir,
                barge_in_gen,
                turn_started_gen,
                recent_speech,
                speaking_until_ms,
                wake_active_until_ms,
                config,
            ) {
                tracing::error!(?err, "pipeline de synthese vocale (TTS) interrompu");
            }
        });
    }

    Ok(())
}

/// Boucle reception PCM16 (front) -> VAD -> accumulation -> STT -> mot-cle Hey Nestor -> envoi a Claude/AGY.
fn listen_loop(
    events_tx: broadcast::Sender<ServerEvent>,
    brain: Arc<NestorBrain>,
    mut mic_rx: mpsc::Receiver<Vec<u8>>,
    tts_tx: mpsc::UnboundedSender<String>,
    vad_model: &std::path::Path,
    whisper_model: &std::path::Path,
    speaking_until_ms: Arc<AtomicU64>,
    wake_active_until_ms: Arc<AtomicU64>,
    barge_in_gen: Arc<AtomicU64>,
    turn_started_gen: Arc<AtomicU64>,
    recent_speech: Arc<echo::RecentSpeech>,
    config: Arc<Config>,
) -> Result<()> {
    let mut vad = vad::SileroVad::load(vad_model).context("chargement Silero VAD")?;
    let whisper = stt::WhisperStt::load(whisper_model).context("chargement Whisper")?;
    // Langue forcee par defaut : la detection automatique se trompe souvent
    // sur des enonces courts et produit alors de l'anglais invente.
    let stt_lang = std::env::var("NESTORD_STT_LANG").unwrap_or_else(|_| "fr".to_string());

    let detector = wake::WakeDetector::new(
        config.wake_word.enabled,
        config.wake_word.timeout_secs,
        wake_active_until_ms.clone(),
        config.address_form.clone(),
        config.wake_word.ack_phrase.clone(),
        config.wake_word.words.clone(),
    );

    // Fin de tour par modele (Smart Turn) ; repli sur le silence fixe s'il est indisponible.
    let mut turn = if config.turn.enabled {
        let path = models_dir().join("smart-turn-v3.2-cpu.onnx");
        match turn::SmartTurn::load(&path) {
            Ok(t) => {
                tracing::info!(path = %path.display(), "detection de fin de tour Smart Turn activee");
                Some(t)
            }
            Err(err) => {
                tracing::warn!(?err, "Smart Turn indisponible : silence fixe de {SILENCE_HANGOVER_MS} ms");
                None
            }
        }
    } else {
        None
    };
    let mut last_turn_check_ms: u64 = 0;

    let initial_status = if config.wake_word.enabled {
        DaemonStatus::Idle
    } else {
        DaemonStatus::Listening
    };
    let _ = events_tx.send(ServerEvent::State { status: initial_status });
    let _ = events_tx.send(ServerEvent::WakeState { active: false });

    let mut pending: Vec<f32> = Vec::new();
    let mut speech_buf: Vec<f32> = Vec::new();
    let mut preroll: Vec<f32> = Vec::new();
    let mut in_speech = false;
    let mut silence_run_ms: u64 = 0;
    let mut speech_run_ms: u64 = 0;

    let mut frames_received: u64 = 0;
    let mut windows_analyzed: u64 = 0;
    let mut was_suppressing = false;
    // Parole continue detectee pendant que Nestor parle (candidate a une interruption).
    let mut barge_buf: Vec<f32> = Vec::new();
    let mut barge_run_ms: u64 = 0;

    while let Some(bytes) = mic_rx.blocking_recv() {
        frames_received += 1;

        // Suppression d'echo : tant que la synthese est censee etre en cours de
        // lecture, on n'ecoute pas pour transcrire. On surveille seulement une
        // parole franche et soutenue (barge-in vocal), qui coupe la lecture.
        if now_ms() < speaking_until_ms.load(Ordering::SeqCst) {
            speech_buf.clear();
            preroll.clear();
            in_speech = false;
            silence_run_ms = 0;
            speech_run_ms = 0;
            was_suppressing = true;

            if !config.barge_in.voice {
                pending.clear();
                continue;
            }

            pending.extend(pcm16_bytes_to_f32(&bytes));
            while pending.len() >= VAD_CHUNK_SAMPLES {
                let window: Vec<f32> = pending.drain(..VAD_CHUNK_SAMPLES).collect();
                let window_ms = (VAD_CHUNK_SAMPLES as f64 / MIC_SAMPLE_RATE as f64 * 1000.0) as u64;
                let prob = vad.process(&window).unwrap_or(0.0);
                if prob >= config.barge_in.threshold {
                    barge_buf.extend_from_slice(&window);
                    barge_run_ms += window_ms;
                } else {
                    barge_buf.clear();
                    barge_run_ms = 0;
                }
            }

            if barge_run_ms >= config.barge_in.min_speech_ms {
                tracing::info!(barge_run_ms, "barge-in vocal : Nestor est interrompu");
                barge_in_gen.fetch_add(1, Ordering::SeqCst);
                speaking_until_ms.store(0, Ordering::SeqCst);
                let timeout_ms = config.wake_word.timeout_secs.max(3) * 1000;
                wake_active_until_ms.store(now_ms() + timeout_ms, Ordering::SeqCst);
                let _ = events_tx.send(ServerEvent::Interrupt);
                let _ = events_tx.send(ServerEvent::WakeState { active: true });
                let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Listening });

                // La parole qui a declenche l'interruption devient le debut de l'enonce.
                speech_buf = std::mem::take(&mut barge_buf);
                speech_run_ms = barge_run_ms;
                barge_run_ms = 0;
                in_speech = true;
                was_suppressing = false;
            }
            continue;
        }
        barge_buf.clear();
        barge_run_ms = 0;

        // La transcription Whisper est synchrone : pendant qu'elle tourne (et
        // pendant la lecture), les frames s'accumulent dans le channel. A la
        // sortie de la fenetre de suppression, ce backlog contient encore de
        // l'echo qu'il ne faut pas analyser.
        if was_suppressing {
            was_suppressing = false;
            pending.clear();
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
                last_turn_check_ms = 0;
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

                let end_of_turn = match turn.as_mut() {
                    // Silence suffisant : on consulte le modele toutes les TURN_CHECK_STEP_MS,
                    // avec une fin forcee au bout de max_silence_ms.
                    Some(model) => {
                        if silence_run_ms >= config.turn.max_silence_ms {
                            tracing::info!(silence_run_ms, "fin de tour forcee (silence maximal)");
                            true
                        } else if silence_run_ms >= config.turn.min_silence_ms
                            && silence_run_ms >= last_turn_check_ms + TURN_CHECK_STEP_MS
                        {
                            last_turn_check_ms = silence_run_ms;
                            let started = std::time::Instant::now();
                            match model.predict(&speech_buf) {
                                Ok(p) => {
                                    tracing::debug!(p, silence_run_ms, took_ms = started.elapsed().as_millis() as u64, "Smart Turn");
                                    p >= config.turn.threshold
                                }
                                // Modele en erreur : on retombe sur le silence fixe.
                                Err(err) => {
                                    tracing::warn!(?err, "inference Smart Turn en erreur");
                                    silence_run_ms >= SILENCE_HANGOVER_MS
                                }
                            }
                        } else {
                            false
                        }
                    }
                    None => silence_run_ms >= SILENCE_HANGOVER_MS,
                };

                if end_of_turn {
                    last_turn_check_ms = 0;
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
                            let text = text.trim();
                            let now = now_ms();

                            // Echo de la propre voix de Nestor : ni dialogue ni reveil.
                            if recent_speech.is_echo(now, text) {
                                tracing::info!(%text, "enonce ecarte : echo de la voix de Nestor");
                                let status = if detector.is_active(now) { DaemonStatus::Listening } else { DaemonStatus::Idle };
                                let _ = events_tx.send(ServerEvent::State { status });
                                continue;
                            }

                            let action = detector.evaluate(text, now);
                            tracing::info!(%text, ?action, "evaluation mot-cle (wake word)");

                            match action {
                                wake::WakeAction::WakeOnly { ack_phrase } => {
                                    tracing::info!(%ack_phrase, "mot-cle seul detecte, acquittement");
                                    let _ = events_tx.send(ServerEvent::Transcript {
                                        role: Role::Assistant,
                                        delta: None,
                                        text: ack_phrase.clone(),
                                        is_final: Some(true),
                                    });
                                    let _ = events_tx.send(ServerEvent::WakeState { active: true });
                                    let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Listening });
                                    turn_started_gen.store(barge_in_gen.load(Ordering::SeqCst), Ordering::SeqCst);
                                    let _ = tts_tx.send(ack_phrase);
                                }
                                wake::WakeAction::Command { query } => {
                                    tracing::info!(%query, "mot-cle avec commande detecte");
                                    let _ = events_tx.send(ServerEvent::Transcript {
                                        role: Role::User,
                                        delta: None,
                                        text: text.to_string(),
                                        is_final: Some(true),
                                    });
                                    let _ = events_tx.send(ServerEvent::WakeState { active: true });
                                    let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Thinking });
                                    turn_started_gen.store(barge_in_gen.load(Ordering::SeqCst), Ordering::SeqCst);
                                    brain.send_user_message_from_audio(&query);
                                }
                                wake::WakeAction::FollowUp { text: follow_up } => {
                                    tracing::info!(%follow_up, "suite de dialogue en session active (follow-up)");
                                    let _ = events_tx.send(ServerEvent::Transcript {
                                        role: Role::User,
                                        delta: None,
                                        text: follow_up.clone(),
                                        is_final: Some(true),
                                    });
                                    let _ = events_tx.send(ServerEvent::WakeState { active: true });
                                    let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Thinking });
                                    turn_started_gen.store(barge_in_gen.load(Ordering::SeqCst), Ordering::SeqCst);
                                    brain.send_user_message_from_audio(&follow_up);
                                }
                                wake::WakeAction::Ignored => {
                                    tracing::info!(%text, "enonce ignore : Nestor est en veille (mot-cle 'Hey Nestor' requis)");
                                    let _ = events_tx.send(ServerEvent::State { status: DaemonStatus::Idle });
                                    let _ = events_tx.send(ServerEvent::WakeState { active: false });
                                }
                            }
                        }
                        Ok(_) => {
                            let status = if detector.is_active(now_ms()) {
                                DaemonStatus::Listening
                            } else {
                                DaemonStatus::Idle
                            };
                            let _ = events_tx.send(ServerEvent::State { status });
                        }
                        Err(err) => {
                            tracing::error!(?err, "echec de transcription whisper");
                            let status = if detector.is_active(now_ms()) {
                                DaemonStatus::Listening
                            } else {
                                DaemonStatus::Idle
                            };
                            let _ = events_tx.send(ServerEvent::State { status });
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
/// barge-in (abandon des segments d'un tour interrompu) et prolongation de
/// la fenetre conversationnelle.
fn speak_loop(
    events_tx: broadcast::Sender<ServerEvent>,
    mut tts_rx: mpsc::UnboundedReceiver<String>,
    piper_dir: &std::path::Path,
    barge_in_gen: Arc<AtomicU64>,
    turn_started_gen: Arc<AtomicU64>,
    recent_speech: Arc<echo::RecentSpeech>,
    speaking_until_ms: Arc<AtomicU64>,
    wake_active_until_ms: Arc<AtomicU64>,
    config: Arc<Config>,
) -> Result<()> {
    let voice = std::env::var("NESTORD_TTS_VOICE").unwrap_or_else(|_| "fr_FR-upmc-medium".to_string());
    let speaker_id = std::env::var("NESTORD_TTS_SPEAKER")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1); // locuteur 1 du modele upmc = Pierre (masculin)

    let timeout_ms = config.wake_word.timeout_secs.max(3) * 1000;
    let synth = tts::PiperTts::load(piper_dir, &voice, speaker_id).context("chargement Piper TTS")?;
    let sample_rate = synth.sample_rate();

    while let Some(sentence) = tts_rx.blocking_recv() {
        let start_gen = barge_in_gen.load(Ordering::SeqCst);

        // Reponse interrompue (barge-in) : on abandonne ses phrases restantes tant
        // qu'un nouveau tour utilisateur n'a pas commence.
        if start_gen != turn_started_gen.load(Ordering::SeqCst) {
            tracing::debug!(%sentence, "phrase d'une reponse interrompue, ignoree");
            continue;
        }

        let waveform = match synth.synthesize(&sentence) {
            Ok(w) => w,
            Err(err) => {
                tracing::error!(?err, %sentence, "echec de synthese TTS, segment ignore");
                continue;
            }
        };

        if barge_in_gen.load(Ordering::SeqCst) != start_gen {
            speaking_until_ms.store(0, Ordering::SeqCst);
            wake_active_until_ms.store(now_ms() + timeout_ms, Ordering::SeqCst);
            continue; // le tour a ete interrompu pendant la synthese elle-meme
        }

        // Le front joue les segments les uns apres les autres : on empile les
        // durees pour couvrir toute la file de lecture, pas seulement ce segment.
        let duration_ms = (waveform.len() as u64 * 1000) / sample_rate.max(1) as u64;
        let playback_start = speaking_until_ms.load(Ordering::SeqCst).max(now_ms());
        let playback_end = playback_start + duration_ms + ECHO_TAIL_MS;
        speaking_until_ms.store(playback_end, Ordering::SeqCst);
        wake_active_until_ms.store(playback_end + timeout_ms, Ordering::SeqCst);

        recent_speech.record(now_ms(), &sentence);
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
            let _ = events_tx.send(ServerEvent::WakeState { active: true });
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
