# Présence et veille de l'ordinateur

La machine ne doit jamais s'endormir en plein travail, et ne doit pas rester
allumée pour rien. Cette page couvre la première moitié (étape E de
`.agent/VISION.md`) : savoir si Monsieur est devant l'ordinateur, et empêcher
la veille tant que Nestor a une raison d'être actif. La mise en veille
volontaire et le réveil programmé (étape G) viendront ensuite.

Module : `nestord/src/power.rs`.

## Présence

nestord interroge `org.gnome.Mutter.IdleMonitor` sur le bus de session
(`GetIdletime`, toutes les 5 secondes). Au-delà de `idle_minutes`
d'inactivité, Monsieur est considéré absent. Sans GNOME ni bus de session,
la présence n'est pas mesurée et Monsieur est **présumé présent** : on préfère
ne jamais endormir la machine plutôt que de l'endormir à tort.

## Inhibition de la veille

Un **seul verrou** logind (`org.freedesktop.login1.Manager.Inhibit`, `sleep:idle`,
mode `block`), pris dès qu'il existe une raison d'être actif et relâché quand
il n'en reste aucune. Raisons, dans l'ordre du motif :

- une mission en statut `started` (« mission #3 en cours ») ;
- Nestor qui réfléchit ou parle (« Nestor réfléchit », « Nestor parle ») ;
- un appel mobile en cours (client `mobile` connecté à `/ws`).

Écouter sans parler ne retient pas la machine. Quand le motif change, le
nouveau verrou est pris **avant** de relâcher l'ancien : aucune fenêtre sans
protection. Diagnostic :

```sh
systemd-inhibit --list      # WHO=nestord, WHY=mission #3 en cours ; Nestor parle
```

## Reprise après veille

Le signal `PrepareForSleep(false)` de logind marque la sortie de veille :
le contexte est rediffusé (`resumed_at_ms`) et les raisons réévaluées.

**Pas encore fait** : relancer une session `claude` morte pendant la veille.
La session est un `OnceLock` et sa mort bascule en mode réduit (`brain.rs`) ;
la boucle proactive le signale (`session_fallback`). Une relance propre demande
de rendre la session remplaçable, chantier à part.

## Dans l'interface

L'événement `context` porte `present`, `idle_secs`, `inhibit` (motif du verrou)
et `resumed_at_ms`. Le panneau « Situation » affiche la présence et l'état de
la veille (« empêchée · mission #3 en cours » ou « autorisée »).

## Configuration

```toml
[power]
inhibit = true       # false : ne jamais prendre de verrou
idle_minutes = 15
```

## Vérifié sur la machine cible

- Ubuntu GNOME sur Wayland, systemd 259 : IdleMonitor et `Inhibit` disponibles.
- Test réel du verrou : `cargo test power::tests::verrou_logind -- --ignored`.
- Mode de veille : `/sys/power/mem_sleep` = `s2idle [deep]` (à retenir pour
  l'étape G, fiabilité du réveil par timer).
