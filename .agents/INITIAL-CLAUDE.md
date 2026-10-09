# PROMPT INITIAL POUR CLAUDE CODE

> Document historique (septembre 2026) : prompt de depart, conserve pour la tracabilite. Le contrat en vigueur est celui de `nestord/src/protocol.rs` et de `docs/audio-protocol.md` ; la voix est Piper (22 050 Hz), pas Kokoro, et l'UI web vit dans `web/`.

Tu collabores avec Antigravity (en charge de l'UI Web/Dashboard) sur un projet commun : "Jarvis local pour Claude Code".
Consulte et synchronise-toi impérativement avec les spécifications techniques ci-dessous pour respecter les contrats d'interface (WebSocket, JSON, ports).

## 1. Contexte & Architecture
- Machine cible : Ubuntu Linux, i5-13600KF, 64 Go RAM, NVIDIA RTX 3060 (12 Go VRAM).
- Rôle : Développer le daemon backend en Rust (`jarvisd`) qui orchestre l'audio local, pilote un sous-processus `claude -p` headless et expose une API WebSocket pour l'UI d'Antigravity.
- Stack Rust :
  * Audio I/O : `cpal` (capture 16 kHz mono), `rodio` (playback).
  * VAD : Silero VAD v5 via ONNX Runtime (`ort` avec feature CUDA).
  * STT : Whisper Large-v3-Turbo via `whisper-rs` (compilé avec support CUDA).
  * TTS : Kokoro-82M ONNX (français/anglais) via `ort` (CUDA).
  * Async & Réseau : `tokio`, `axum` (WebSocket + HTTP), `serde_json`.

## 2. Pilotage Headless de Claude Code
- Spawner un sous-processus `claude` :
  ```bash
  claude -p \
    --input-format stream-json \
    --output-format stream-json \
    --include-partial-messages \
    --dangerously-skip-permissions

```

* Scrub strict de l'environnement : purger impérativement `ANTHROPIC_API_KEY`, `CLAUDE_CODE_API_KEY` et `CLAUDECODE` avant le spawn pour consommer le quota de session locale (`claude auth login`).
* Stdin : injecter les messages sous forme NDJSON : `{"type": "user_message", "content": "..."}\n`.
* Stdout : parser au fil de l'eau les `stream_event` (`text_delta`). Découper par ponctuation (`.`, `,`, `!`, `?`, `\n`) et envoyer immédiatement les segments au pipeline TTS.

## 3. Contrat d'Interface WebSocket avec Antigravity

Le serveur Axum écoute sur `127.0.0.1:8340/ws`. Les payloads JSON échangés doivent respecter strictement ce format :

* Événements émis par le Daemon Rust vers l'UI :
* Événement état : `{"type": "state", "status": "listening" | "thinking" | "speaking" | "idle"}`
* Amplitude audio (pour l'orbe 3D) : `{"type": "audio_levels", "rms": 0.42, "peak": 0.85}`
* Transcription utilisateur en direct : `{"type": "transcript", "role": "user", "text": "...", "is_final": true}`
* Streaming texte de Claude : `{"type": "transcript", "role": "assistant", "delta": "...", "text": "..."}`
* Exécution d'outil Claude : `{"type": "tool_call", "name": "Bash", "input": {...}, "status": "running" | "completed"}`


* Événements reçus par le Daemon depuis l'UI :
* Interruption forcée : `{"type": "barge_in"}` (stoppe immédiatement rodio et flush les buffers).
* Injection texte manuelle : `{"type": "send_text", "content": "..."}`.



## 4. Tâche immédiate

1. Initialise le projet Cargo avec les dépendances ciblées.
2. Écris le module de sous-processus `claude` avec le scrub d'environnement et le parsing NDJSON.
3. Mets en place le serveur Axum exposant le WebSocket pour permettre à Antigravity de se connecter dès son premier build.
