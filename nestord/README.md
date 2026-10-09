# nestord

Daemon backend de Nestor (assistant vocal local pour Claude Code) : pilote un
sous-processus `claude` headless, delegue les taches longues a des sous-agents
et expose une API WebSocket a l'UI web et a l'appli mobile sur `127.0.0.1:8340/ws`.

- `../docs/audio-protocol.md` : contrat audio avec le front.
- `../docs/missions.md` : delegation aux sous-agents, serveur MCP, routage.
- `../docs/proactive.md` : boucle proactive, alertes a l'initiative de Nestor.
- `../docs/position.md` : position envoyee par l'appli mobile, `POST /location`.
- `../docs/veille.md` : presence devant l'ordinateur et inhibition de la veille.
- `../docs/agenda.md` : Google Calendar en lecture directe, `nestord onboard --google`.
- `../docs/canal-mobile.md` : telephone joignable hors appel (`?client=standby`), sonnerie au reveil.
- `../docs/memory.md` : memoire longue (SQLite + FTS5), outils `memory_write`, `memory_search`, `memory_forget`.

Endpoints : `/ws` (WebSocket UI) et `/mcp` (serveur MCP consomme par la
session conversationnelle elle-meme).

## Modes de compilation

- **Core (par defaut)** : `cargo build` / `cargo run`. Pilotage texte de
  Claude + WebSocket. Aucune dependance systeme au-dela d'un compilateur Rust.
- **`full-audio`** : `cargo build --features full-audio`. Ajoute le pipeline
  VAD (Silero v5) + STT (Whisper large-v3-turbo) + TTS (Piper), tout en
  CUDA via ONNX Runtime et whisper.cpp.

## Prerequis systeme pour `full-audio`

Paquets a installer (Ubuntu/Debian) :

```sh
sudo apt-get install -y libssl-dev nvidia-cuda-toolkit nvidia-cudnn espeak-ng
```

- `libssl-dev` : requis par une dependance transitive (openssl-sys).
- `nvidia-cuda-toolkit` : fournit `nvcc`, necessaire pour compiler le backend
  CUDA de whisper.cpp (via `whisper-rs-sys`, build cmake). Sur Ubuntu, la
  version depot (`12.4.x`) suffit meme si le driver GPU est plus recent.
- `nvidia-cudnn` : requis par ONNX Runtime (execution provider CUDA) pour
  Silero VAD et Piper. C'est un paquet "installateur" qui telecharge cuDNN
  depuis NVIDIA lors de son installation (`update-nvidia-cudnn`) - relancer
  l'installation si `libcudnn.so.9` n'apparait pas sous
  `/usr/lib/x86_64-linux-gnu/` du premier coup.
- `espeak-ng` : phonemisation (texte -> IPA) en amont de Piper TTS. Sur
  certaines configs, `apt install espeak-ng` n'installe que les
  dependances (`libespeak-ng1`, `espeak-ng-data`) sans le binaire CLI :
  verifier avec `which espeak-ng` et reinstaller le paquet explicitement
  si besoin.

### CUDA 13 vs CUDA 12 (contournement connu)

Le binaire ONNX Runtime distribue par la crate `ort` est lie a CUDA 13
(`libcublasLt.so.13`, absent du paquet `nvidia-cuda-toolkit` d'Ubuntu qui ne
fournit que CUDA 12.4). Contournement utilise sur cette machine : reutiliser
les bibliotheques CUDA 13 embarquees par Ollama (`/usr/local/lib/ollama/cuda_v13`)
via `LD_LIBRARY_PATH` :

```sh
LD_LIBRARY_PATH=/usr/local/lib/ollama/cuda_v13:$LD_LIBRARY_PATH \
  cargo run --features full-audio
```

Si Ollama n'est pas installe, une alternative propre serait d'installer le
toolkit CUDA 13 complet depuis le depot NVIDIA (`developer.nvidia.com/cuda-downloads`).

## Modeles requis (`full-audio`)

A placer sous `$NESTORD_MODELS_DIR` (par defaut `~/.local/share/nestord/models/`) :

```
models/
├── silero_vad.onnx                  # https://github.com/snakers4/silero-vad
├── ggml-large-v3-turbo-q5_0.bin     # https://huggingface.co/ggerganov/whisper.cpp
├── vocabulaire.txt                  # optionnel : vocabulaire STT du projet
└── piper/
    ├── fr_FR-upmc-medium.onnx        # voix par defaut (locuteur 1 = Pierre, masculin)
    └── fr_FR-upmc-medium.onnx.json   # config obligatoire (phonemes, sample rate, scales)
```

Source Piper : `rhasspy/piper-voices` sur Hugging Face, repertoire `fr/fr_FR/`.
Voix masculines francaises disponibles : `upmc/medium` (locuteur 1, 22,05 kHz),
`tom/medium` (44,1 kHz), `gilles/low` (16 kHz, mais son vocabulaire de phonemes
ignore la nasalisation - francais degrade).

Piper a ete prefere a Kokoro-82M parce que Kokoro v1.0 n'embarque qu'une seule
voix francaise (`ff_siwis`), feminine.

## Variables d'environnement

| Variable              | Defaut                              | Usage                                  |
|-----------------------|--------------------------------------|-----------------------------------------|
| `NESTORD_MODELS_DIR`  | `~/.local/share/nestord/models`      | Racine des modeles VAD/STT/TTS          |
| `NESTORD_TTS_VOICE`   | `fr_FR-upmc-medium`                  | Voix Piper (nom de fichier sans `.onnx`)|
| `NESTORD_TTS_SPEAKER` | `1`                                  | Locuteur (modeles multi-locuteurs)      |
| `NESTORD_STT_LANG`    | `fr`                                 | Langue Whisper (`auto` = detection)     |
| `RUST_LOG`            | `nestord=info`                       | Niveau de log (`tracing-subscriber`)    |

La langue de phonemisation n'est pas configurable : elle vient du champ
`espeak.voice` de la config `.onnx.json` de la voix.

Le fichier `vocabulaire.txt` (optionnel, a cote des modeles) ajoute des termes
a l'amorce de transcription Whisper, une entree par ligne. C'est le levier
pour les mots mal transcrits : sans lui, « Bash » devient « bâches » et
« tool use » devient « Toulouse ».

`NESTORD_STT_LANG=auto` est deconseille : sur des enonces courts, Whisper
identifie regulierement du francais comme de l'anglais et produit alors une
phrase anglaise inventee.
