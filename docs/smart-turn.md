# Detection de fin de tour (Smart Turn) — prototype

Remplace le silence fixe de 700 ms par le modele
[Smart Turn v3](https://github.com/pipecat-ai/smart-turn) (BSD-2, ONNX) : apres
un court silence, il estime si l'utilisateur a *fini* sa phrase.

## Activation

1. Telecharger le modele CPU (8 Mo) dans le dossier des modeles :
   `curl -L -o ~/.local/share/nestord/models/smart-turn-v3.2-cpu.onnx https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/main/smart-turn-v3.2-cpu.onnx`
2. `NESTORD_TURN_DETECTION=1` ou, dans `config.toml` :
   ```toml
   [turn]
   enabled = true
   threshold = 0.5        # probabilite de "tour termine"
   min_silence_ms = 300   # silence avant de consulter le modele
   max_silence_ms = 1400  # fin forcee ; < maintien VAD des clients (~1600 ms)
   ```
Sans le modele, repli automatique sur le silence fixe.

## Fonctionnement

`audio/turn.rs` calcule un log-mel Whisper (80 mels, 8 s, n_fft 400, hop 160,
audio normalise et complete par des zeros au debut), puis execute le modele.
La sortie est deja une probabilite. Les clients (web, Android) gardent donc
~1,6 s de silence en fin de parole pour que le serveur puisse attendre.

## Etat de la validation

- Extraction mel Rust identique a numpy (test de parite) ; probabilites Rust =
  Python sur de vraies voix.
- Gain mesure : phrase complete close apres ~320 ms de silence au lieu de 700 ms.
- Cout : ~8 ms par inference en CPU optimise (ONNX Runtime). Les 150-240 ms
  vus en build debug viennent surtout de l'extraction mel non optimisee.
- **Non valide** : la capacite a *attendre* pendant une hesitation. Sur des
  voix de synthese coupees en plein mot, le modele repond « termine » (0,95+)
  des qu'un silence de 320 ms suit. A tester avec de vraies hesitations
  humaines avant d'activer par defaut.
