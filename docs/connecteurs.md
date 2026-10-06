# Connecteurs : serveurs MCP externes

Nestor peut utiliser des serveurs MCP externes (messagerie, agenda, notes…).
L'assistant tourne en « mode auto » du CLI `claude` (`--permission-mode auto`) :
personne ne répond à ses demandes de permission, un classifieur autorise ou
refuse chaque action à votre place. Ce classifieur laisse passer un envoi
banal et ne garantit pas d'arrêter une action dangereuse : donner à l'assistant
un accès personnel direct reviendrait à le laisser écrire sans votre accord.
`nestord` fait donc **passerelle** : l'assistant ne voit que le serveur MCP de
nestord, qui relaie les outils externes sous le nom `<serveur>__<outil>` et
applique les règles ci-dessous **dans le code** (`nestord/src/connectors.rs`).

## Règles

| Mode | Effet |
|---|---|
| `read` (lecture libre) | relayé sans demander |
| `confirm` (avec votre accord) | l'appel reste bloqué jusqu'à votre réponse : bouton de l'interface ou « oui » à la voix. Sans réponse en 2 minutes : refus |
| `off` (non exposé) | absent de la liste vue par l'assistant, et refusé même si son nom est deviné |

Mode par défaut : `read` si le serveur annonce l'outil en lecture seule
(`annotations.readOnlyHint`), `confirm` pour tout le reste.

Priorité : choix fait dans l'interface (enregistré dans
`~/.config/nestord/ui-connectors.toml`) > table `tools` de `config.toml` > défaut.

## Jeton obligatoire

Aucun connecteur externe n'est démarré sans jeton d'accès à `/ws`.

```bash
nestord onboard        # affiche le jeton UNE fois ; à saisir dans Réglages > Accès au daemon
```

## Ce que l'assistant ne peut pas faire lui-même

L'assistant exécute des commandes avec vos droits. Les protections suivantes
visent à ce qu'il ne puisse pas s'accorder ce que vous devez valider
(`nestord/src/auth.rs`) :

- **Le jeton n'est stocké nulle part en clair.** `~/.config/nestord/auth_token` ne
  contient que son empreinte SHA-256 : le lire ne donne pas le jeton. Un jeton
  perdu ne se réaffiche pas, il se remplace (`nestord onboard --rotate`).
- **`/mcp` a son propre secret**, tiré au hasard à chaque démarrage et remis au
  seul assistant. Il ouvre les outils, pas `/ws` : il ne permet ni d'approuver
  une écriture ni de changer un mode ou un réglage.
- **Les secrets quittent son environnement** : `NESTORD_AUTH_TOKEN` et toutes les
  variables `${VAR}` citées par les connecteurs sont retirées de l'environnement
  de l'assistant et des sous-agents.
- **Origine web vérifiée** : une page d'un autre site ne peut se connecter ni à
  `/ws` ni à `/mcp`, même sans jeton. Pour une interface servie ailleurs que sur
  la machine (Tailscale…), déclarer `allowed_origins = ["https://…"]`.
- **Accord à la voix** : l'annonce dit ce qui va être fait (les premiers
  arguments de l'appel). Seule une réponse brève et nette compte (« oui », « je
  confirme », « non », « annule »…) ; toute autre phrase laisse la demande en
  attente et Nestor redemande. Avec plusieurs demandes en attente, il faut
  répondre à l'écran.
- **Règles du juge indépendantes des réglages** : monter les seuils à 100 dans
  l'interface ne désarme pas les règles fixes.

**Ce qui reste hors de portée de ces protections.** L'assistant tourne sous votre
compte : il peut lire vos autres fichiers, dont les jetons que les serveurs MCP
locaux rangent dans votre dossier personnel (`~/.gmail-mcp/…`) et le profil de
votre navigateur, où l'interface mémorise le jeton. Un assistant manipulé par un
contenu piégé pourrait donc contourner la passerelle en appelant directement le
service. Une vraie étanchéité demande de faire tourner l'assistant sous un autre
compte ou dans un bac à sable ; ce n'est pas fait.

## Déclarer un serveur

Dans `~/.config/nestord/config.toml`, puis redémarrer nestord :

```toml
# Serveur HTTP
[[mcp_servers]]
name = "agenda"
url = "https://exemple.net/mcp"
headers = { Authorization = "Bearer ${AGENDA_TOKEN}" }   # ${VAR} : variable d'environnement de nestord
tools = { supprimer_evenement = "off" }

# Serveur local lancé par nestord (transport stdio)
[[mcp_servers]]
name = "notes"
command = "npx"
args = ["-y", "un-serveur-mcp"]
env = { API_KEY = "${NOTES_API_KEY}" }
```

Les secrets passent par des variables d'environnement, pas par le fichier.

## Limites connues

- L'assistant est lancé avec `--strict-mcp-config` : il ne voit **que** le serveur
  MCP de nestord. Les connecteurs attachés à votre compte claude.ai (Gmail,
  Drive…) ne lui sont plus accessibles directement ; pour les retrouver, il faut
  les déclarer ici, derrière la passerelle. Les sous-agents de mission n'ont
  accès à aucun serveur MCP.
- Le mode réduit (Antigravity, `agy`) est un autre programme : ses propres accès
  ne sont pas filtrés par nestord.
- Exposer ou masquer un outil ne change la liste vue par l'assistant qu'à sa
  prochaine session ; le blocage, lui, est immédiat.
- La règle « lecture libre » repose sur l'annonce du serveur : un serveur qui
  déclarerait à tort un outil en lecture seule ne serait pas confirmé. Vérifiez
  la liste dans l'interface avant de vous y fier.
- Une lecture peut ramener un texte piégé (un mail qui donne des instructions) :
  la confirmation des écritures limite les dégâts, elle ne les exclut pas.
- Serveurs à authentification OAuth interactive : non pris en charge (jeton
  statique en en-tête ou variable d'environnement uniquement).
- Un connecteur en erreur (serveur injoignable, processus mort, session expirée) est
  retenté toutes les 30 secondes ; entre-temps ses outils ne sont plus exposés. La
  panne n'est constatée qu'au premier appel qui échoue.
- Deux outils dont les noms exposés coïncident (`list.events` et `list_events`) : le
  second est ignoré, avec un avertissement dans les journaux.

## Essai local

`nestord/tests/fixtures/mcp_notes_server.py` est un serveur de test (carnet de
notes : `lire_notes`, `ajouter_note`, `tout_effacer`), utilisé par les tests
d'intégration de `connectors.rs`.
