# Connecteurs : serveurs MCP externes

Nestor peut utiliser des serveurs MCP externes (messagerie, agenda, notes…).
L'assistant tourne sans demander de permission pour ses outils : lui donner
directement un accès personnel reviendrait à le laisser écrire sans contrôle.
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

Aucun connecteur externe n'est démarré sans jeton d'accès. Sans lui, tout
processus local pourrait se connecter à `/ws` ou `/mcp` et utiliser ces accès.

```bash
nestord onboard        # génère le jeton, à saisir dans Réglages > Accès au daemon
```

Avec un jeton, `/mcp` l'exige aussi (`Authorization: Bearer …`) ; nestord le
transmet lui-même à l'assistant, dans un fichier lisible par vous seul.

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

- Exposer ou masquer un outil ne change la liste vue par l'assistant qu'à sa
  prochaine session ; le blocage, lui, est immédiat.
- La règle « lecture libre » repose sur l'annonce du serveur : un serveur qui
  déclarerait à tort un outil en lecture seule ne serait pas confirmé. Vérifiez
  la liste dans l'interface avant de vous y fier.
- Une lecture peut ramener un texte piégé (un mail qui donne des instructions) :
  la confirmation des écritures limite les dégâts, elle ne les exclut pas.
- Serveurs à authentification OAuth interactive : non pris en charge (jeton
  statique en en-tête ou variable d'environnement uniquement).
- Pas de reconnexion automatique si un serveur externe tombe.

## Essai local

`nestord/tests/fixtures/mcp_notes_server.py` est un serveur de test (carnet de
notes : `lire_notes`, `ajouter_note`, `tout_effacer`), utilisé par les tests
d'intégration de `connectors.rs`.
