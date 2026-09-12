# Missions deleguees a des sous-agents

## Pourquoi

La session conversationnelle de Nestor doit rester reactive : c'est une
interface vocale, l'utilisateur attend une reponse en une ou deux phrases dans
la seconde. Elle ne peut donc pas executer elle-meme un travail long (lire du
code, lancer des tests, refactorer) sans devenir muette pendant une minute.

Les taches longues sont donc confiees a des **sous-agents** : un processus
dedie par mission, suivi par `nestord`, qui rend compte plus tard.

## Chaine complete

1. `nestord` expose un serveur MCP (JSON-RPC sur HTTP) sur `/mcp`, avec les
   outils `start_mission`, `stop_mission` et `list_missions`
   (`nestord/src/mcp.rs`).
2. Le CLI `claude` de la session conversationnelle s'y connecte au demarrage,
   via `--mcp-config` (fichier genere dans le repertoire temporaire). Les
   outils lui apparaissent comme `mcp__nestor__start_mission`.
3. Sur demande longue, la session appelle `start_mission(description)` et
   recoit immediatement un identifiant : elle annonce « c'est lance » et reste
   disponible pour parler.
4. `nestord` spawne le sous-agent (`nestord/src/mission.rs`) :
   `claude -p --output-format stream-json ...` ou `agy -p ...`.
5. Les appels d'outils du sous-agent sont diffuses a l'UI, marques par
   `mission_id`, pour alimenter la console d'execution.
6. A la fin, le compte rendu est **reinjecte dans la conversation** comme
   rapport interne. C'est Nestor qui l'annonce a voix haute, avec ses
   contraintes de concision, plutot qu'un texte brut pousse au TTS.

Le serveur HTTP doit ecouter **avant** le spawn de `claude`, sinon la
connexion MCP echoue pour toute la session : c'est pourquoi `main.rs` demarre
axum d'abord et renseigne la poignee de session ensuite (`OnceLock`).

## Suivi de l'avancement

Une mission longue ne doit pas etre une boite noire. A chaque appel d'outil du
sous-agent, `nestord` republie l'evenement `mission` avec un champ `progress`
decrivant l'activite en une ligne (« Read : nestord/src/audio/vad.rs »,
« Bash : cargo test »). Le detail retenu est le premier champ parlant de
l'entree de l'outil (`file_path`, `path`, `command`, `CommandLine`, `pattern`,
`query`).

`list_missions` renvoie egalement cette derniere activite, ce qui permet de
repondre vocalement a « ou en est la mission ? » sans attendre le compte rendu.

## Annulation

Deux chemins, pour le meme mecanisme :

- Vocal : l'utilisateur demande d'arreter, la session appelle
  `stop_mission(id, reason)`.
- UI : le bouton d'annulation du panneau des missions emet
  `{"type": "stop_mission", "id": 1, "reason": "..."}` sur le WebSocket
  (`reason` facultatif).

Dans les deux cas, `nestord` envoie le signal sur le canal d'annulation de la
mission ; la boucle de lecture du flux sort par `select!`, tue le
sous-processus sans attendre la fin de son tour, et la mission passe au statut
`cancelled`. Le compte rendu d'annulation est lui aussi reinjecte dans la
conversation, pour que Nestor le dise plutot que de rester muet.

Ce compte rendu contient le **motif** et le **travail deja produit** : le texte
que le sous-agent avait commence a rediger n'est pas jete (tronque a 600
caracteres, puisqu'il finit lu a voix haute). Une annulation sans explication
ni resultat partiel etait la principale frustration remontee a l'usage.

Les appels d'outils encore ouverts au moment de l'arret sont refermes
explicitement : sans cela, l'UI les affiche « en cours » indefiniment
(cf. `.agent/reponse-annulation-mission-ui.md`).

## Routage des backends

- Backend par defaut : `claude`.
- `agy` (Antigravity) peut etre demande explicitement par la session.
- Bascule automatique vers `agy` quand le quota Claude depasse 85 % de
  remplissage : une mission longue coupee en plein milieu par la limite de
  taux est pire qu'une mission confiee a l'autre agent. Le seuil est
  `USAGE_ROUTING_THRESHOLD` dans `mission.rs`.

Le quota est lu dans le flux de la session conversationnelle : le CLI emet des
evenements `rate_limit_event` contenant le remplissage des fenetres 5 h et 7 j
(`nestord/src/usage.rs`).

## Les deux CLI ne se pilotent pas de la meme facon

`agy` ressemble beaucoup a `claude` en surface (`-p`, `--output-format
stream-json`, `--dangerously-skip-permissions`), mais deux differences cassent
tout si on les ignore - les deux ont ete rencontrees en integration :

**1. Passage du prompt.** `claude` traite `-p` comme un booleen et prend le
prompt en argument positionnel. `agy` parse ses flags a la maniere de Go : le
prompt doit etre **attache** au flag, sinon `-p` avale l'option suivante.

```sh
claude -p "ma mission" --output-format stream-json --verbose   # ok
agy -p="ma mission" --output-format stream-json                # ok
agy -p "ma mission" --output-format stream-json                # -p prend "--output-format" comme prompt
```

`--verbose` est par ailleurs obligatoire pour `claude` avec
`--output-format stream-json`, et inexistant pour `agy`.

**2. Schema du flux.** Les deux sortent du NDJSON, mais rien de commun :

| | `claude` | `agy` |
|---|---|---|
| Enveloppe | `type` | `event` |
| Texte | `assistant` -> `message.content[].text` | `step_update` (`step_type: agent_response`) -> `text_delta` |
| Outil | `assistant` -> bloc `tool_use` (`name`, `input`) | `step_update` (`step_type: tool`) -> `tool_name`, `tool_info.parameters` |
| Etat d'outil | pas d'etat de fin dans ce flux | `state` : `ACTIVE` puis `DONE` |
| Fin | `result` -> `result` | `result` -> `result.response` + `result.status` |

D'ou deux parseurs distincts dans `mission.rs` (`parse_claude_line` et
`parse_agy_line`), qui alimentent la meme structure `StreamOutcome`. Avantage
au passage pour `agy` : comme il signale l'etat `DONE` de ses outils, ses
`tool_call` remontent a l'UI avec le statut `completed`.

## Nouveaux evenements pour l'UI

En plus des evenements deja decrits dans `audio-protocol.md` :

```json
{"type": "mission", "id": 1, "backend": "claude", "status": "started",
 "description": "...", "summary": "note de routage eventuelle",
 "progress": "Read : nestord/src/audio/vad.rs"}
```

`status` vaut `started`, `completed`, `failed` ou `cancelled`. `summary` porte
le compte rendu final (ou, pour une annulation, le motif et le travail
partiel), et eventuellement une note de routage au demarrage. `progress` n'est
present que sur les evenements d'avancement d'une mission en cours.

Cote client, en plus de `barge_in` et `send_text` :

```json
{"type": "stop_mission", "id": 1, "reason": "l'utilisateur a change d'avis"}
```

```json
{"type": "usage", "five_hour": 0.24, "seven_day": 0.43, "resets_at": 1789230600}
```

Taux de remplissage du quota de la session locale (0 a 1), utile pour un
indicateur dans l'UI. `resets_at` est un horodatage epoch en secondes.

Enfin, `tool_call` porte desormais un champ optionnel `mission_id` : absent
pour les outils de la session conversationnelle, present pour ceux d'un
sous-agent. La console peut ainsi regrouper les executions par mission.

## Instantane a la connexion

Les evenements sont diffuses en direct et ne sont pas rejoues. Pour qu'un
client arrivant en cours de route (ou se reconnectant) ne voie pas un panneau
vide, `nestord` envoie a chaque nouvelle connexion WebSocket un instantane :
le quota courant, puis une ligne `mission` par mission connue. Le front n'a
donc rien a persister de son cote.
