# Présence et veille de l'ordinateur

La machine ne doit jamais s'endormir en plein travail, et ne doit pas rester
allumée pour rien. Deux modules : `power.rs` (présence, verrou de veille) et
`sleep.rs` (conditions de mise en veille, réveil programmé).

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

Une session `claude` morte est **relancée** (`claude_process::ensure_alive`) :
la session est remplaçable (`ClaudeSlot`), le superviseur garde les paramètres
de lancement et l'identifiant de session, et reprend la conversation par
`--resume` à la première tentative. Entre-temps le cerveau est en mode réduit ;
la bascule est silencieuse (pas d'annonce de quota pour un plantage) ; au
retour, `set_backend("auto")` revient à Claude sauf quota épuisé ou mode réduit
choisi à la main. Une mort
précoce (moins de 30 s après le lancement) espace les relances : 5 s, 10 s,
20 s… jusqu'à 5 min. La même relance joue à tout moment, pas seulement après
une veille.

Vérifié sur la machine : processus `claude` tué à la main, relancé 5 s plus
tard, cerveau revenu sur Claude. L'identifiant de session n'est connu qu'une
fois une conversation entamée (le CLI n'émet rien avant la première entrée) :
une session tuée avant tout échange repart à neuf, ce qui ne perd rien. La
reprise par `--resume` d'une conversation entamée n'a pas été exercée en
conditions réelles (elle consomme un tour de quota).

## Veille nocturne et réveil (`sleep.rs`)

Tant que nestord gère la veille (`[sleep] enabled`, par défaut), il tient un
verrou supplémentaire « veille différée (…) » jusqu'à ce que **toutes** les
conditions soient réunies :

1. Monsieur est **chez lui** : dernière position reçue dans le lieu
   `home_place`, plus récente que `location_max_age_minutes`. Sans partage de
   position depuis le téléphone, la machine ne dormirait donc jamais :
   `require_home = false` retire cette condition.
2. Il **n'utilise plus l'ordinateur** (présence mesurée et absente).
3. **Aucune raison d'être actif** (missions, parole, appel).
4. **Plage de repos** : heures calmes, ou aucun rendez-vous avant
   `rest_free_hours`.

Déroulé, chaque minute :

1. Heure du réveil = premier rendez-vous à heure fixe moins la préparation
   (`[wake] preparation_minutes`) et le trajet estimé s'il a un lieu
   (`[proactive] default_travel_minutes`), bornée par `[wake] default_time`
   (la plus tôt des deux ; l'heure par défaut seule si l'agenda est vide).
2. Dès que les conditions sont réunies, un timer systemd utilisateur
   transitoire `nestor-wake` est armé avec `WakeSystem=true` (un seul à la
   fois, réarmé si l'heure change de plus d'une minute). Son service appelle
   `POST /wake` avec un secret tiré au démarrage, lu dans
   `~/.config/nestord/wake_header` (droits 600) : la ligne de commande de
   l'unité, visible par tout utilisateur local, ne porte que le chemin.
3. nestord **vérifie** le timer (`systemctl --user list-timers --output=json`)
   avant de relâcher le verrou. Sans réveil armé, la machine reste éveillée et
   le contexte dit « réveil non programmé ».
4. Le verrou relâché, c'est la politique d'économie d'énergie de GNOME qui
   endort la machine (sur cette machine : suspension après 60 min
   d'inactivité). `force_suspend = true` demande `systemctl suspend` sans
   attendre.
5. Au réveil : `PrepareForSleep(false)` recalcule le contexte et relance la
   session si besoin ; le timer déclenche `/wake`, qui réinjecte un rapport
   interne (heure, premier rendez-vous, départ conseillé, tâches en retard)
   que Nestor formule. Sans client connecté, ou si la session est en cours de
   relance, l'annonce attend (prochaine connexion ou session prête). Un
   événement `alert` de type `wake` trace le réveil. Pendant
   `wake_grace_minutes` après une sortie de veille ou un réveil programmé, la
   machine reste éveillée : sans ce sursis elle se rendormirait avant
   l'annonce.

Diagnostic : `systemctl --user list-timers nestor-wake.timer`.

```toml
[sleep]
enabled = true
home_place = "domicile"
require_home = true
location_max_age_minutes = 180
rest_free_hours = 4
force_suspend = false
wake_grace_minutes = 30
```

**À valider sur la machine**, et non vérifiable par le code : que le timer
`WakeSystem` en instance utilisateur sorte réellement la machine de veille.
Procédure : `systemd-run --user --on-active=300 --timer-property=WakeSystem=true --unit=test-wake true`,
puis `systemctl suspend` ; la machine doit se réveiller dans les cinq
minutes. La documentation systemd indique que `WakeSystem=` demande des
privilèges « généralement » réservés à l'instance système ; l'instance
utilisateur l'accepte ici (`WakeSystem=yes`), reste à observer l'effet. En cas
d'échec, il faudra passer par une unité système et une règle polkit. Mode de
veille : `s2idle [deep]`.

**Pas encore fait** : sonner le téléphone au réveil. Il faut d'abord le canal
nestord → mobile hors appel (voir `docs/agenda.md`).

## Dans l'interface

L'événement `context` porte `present`, `idle_secs`, `inhibit` (motif du verrou),
`resumed_at_ms`, puis `sleep_managed`, `sleep_allowed`, `sleep_blockers`,
`wake_at_ms` et `wake_armed`. Le panneau « Situation » affiche la présence,
l'état du verrou et la veille nocturne (« permise · réveil 07:30 » ou
« différée » avec ses motifs).

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
