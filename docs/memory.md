# Mémoire de Nestor : modèle de données et récupération

Date : 2026-09-12, implémentée le 2026-10-09
Statut : **en place** (`nestord/src/memory.rs`, SQLite + FTS5). Le modèle
ci-dessous est celui du code ; la section 8 dit ce qui est fait et ce qui
reste.

Place dans la feuille de route : après l'étape 2 du `.agent/VISION.md`
(contexte et sécurité réseau), et avant la boucle proactive, qui a besoin de la
mémoire pour dédoublonner ses alertes.

## 1. Ce que la mémoire doit contenir

Quatre familles, qui cohabitent dans un même graphe :

- **Monsieur et ses préférences** : qui il est, comment il travaille, ce qu'il
  a validé ou refusé (« pas d'attribution IA dans les commits »).
- **État et décisions du projet** : ce qui a été choisi *et pourquoi* (« Piper
  plutôt que Kokoro, car Kokoro v1.0 n'a qu'une voix française, féminine »),
  ainsi que les pièges rencontrés.
- **Historique des missions** : ce qui a été demandé, fait, annulé, avec les
  comptes rendus.
- **Connaissance du code** : le rôle de chaque module, pour répondre sans
  relire le dépôt.

## 2. Le nœud

Un seul type d'enregistrement, discriminé par un champ `kind`. Un schéma
unique évite de multiplier les tables pour un besoin encore mouvant.

| Champ | Rôle |
|---|---|
| `id` | Identifiant stable |
| `kind` | `person`, `preference`, `project`, `decision`, `pitfall`, `mission`, `code`, `place`, `fact` |
| `title` | Une ligne, prononçable telle quelle |
| `body` | Le détail, **500 caractères maximum** (voir §4) |
| `context` | Portée : `projet:nestor`, `perso`, `machine:atelier`… |
| `tags` | Classification libre : `audio`, `git`, `mobile`, `quota`… |
| `source` | D'où vient l'information : `conversation`, `mission:12`, `fichier:nestord/src/mission.rs` |
| `confidence` | Monte quand le fait est confirmé, descend quand il est contredit |
| `created_at` / `updated_at` | Horodatages |
| `valid_until` | Renseigné quand le fait cesse d'être vrai (voir §5) |
| `embedding` | Optionnel, selon le stockage retenu |

`context` et `tags` répondent au besoin de « classifications et contextes
différents » : ce sont eux qui évitent de remonter des préférences de style de
code quand la conversation porte sur l'agenda.

## 3. L'arête

| Champ | Rôle |
|---|---|
| `from`, `to` | Les deux nœuds |
| `relation` | `concerne`, `décide_pour`, `remplace`, `contredit`, `cause`, `fait_partie_de`, `produit_par`, `mentionne` |
| `context` | Même portée que pour les nœuds |
| `source`, `created_at` | Traçabilité |

C'est le graphe qui donne la valeur : une décision pointe vers le projet
qu'elle concerne, vers le piège qui l'a motivée, et vers la mission qui l'a
produite. Une question comme « pourquoi Piper ? » se répond en un saut.

## 4. Récupération : le budget d'abord

Contrainte de tête : la session vocale doit rester courte et rapide. La
mémoire ne doit donc **jamais** être déversée dans le prompt système.

- **Budget dur** : au plus ~1 500 caractères de mémoire injectée par tour,
  soit une dizaine de faits. C'est pour cela que `body` est plafonné : un nœud
  porte un résumé prononçable et un *pointeur* (chemin de fichier, numéro de
  mission) vers le détail.
- **Le modèle tire, on ne pousse pas** : un outil MCP `memory_search(query,
  context?, kinds?, limit)`, à côté des outils de mission. Nestor interroge
  quand il en a besoin.
- **Une exception bornée** : au démarrage de session, une « fiche » de dix
  lignes maximum (identité, préférences permanentes, missions en cours). Sans
  elle, Nestor redemande à chaque redémarrage ce qu'il sait déjà.

Chaîne de récupération :

1. **Points d'entrée** : recherche lexicale (et vectorielle si le stockage le
   permet) sur `title` et `body`, filtrée par `context` et `kinds`.
2. **Expansion de graphe** : 1 à 2 sauts depuis les meilleurs points d'entrée.
   C'est ce qui ramène le *pourquoi* d'une décision sans que la question ne
   l'ait nommé.
3. **Reclassement** : score de recherche × fraîcheur × `confidence` ×
   correspondance de contexte. Les nœuds dont `valid_until` est passé sont
   écartés, sauf demande explicite d'historique.
4. **Troncature au budget**, en préférant la diversité des `kind` plutôt que
   dix variantes du même fait.

## 5. Écriture, doublons et obsolescence

- **Capture** : pas par la session vocale, qui doit rester réactive. Une
  mission de fond (la machinerie existe déjà) extrait les faits d'un tour de
  conversation ou d'un compte rendu et les écrit.
- **Écriture explicite** : `memory_write` pour « retiens que… ».
- **Dédoublonnage obligatoire avant insertion** : on cherche un nœud
  quasi identique ; s'il existe, on met à jour et on augmente `confidence`
  plutôt que de créer un doublon. Sans cette règle, la mémoire se remplit de
  variantes et la récupération se dégrade.
- **Contradiction** : on ne réécrit pas l'histoire. L'ancien nœud reçoit
  `valid_until = maintenant`, le nouveau une arête `remplace` vers lui. Nestor
  peut ainsi dire « vous me disiez l'inverse en juin ».
- **Oubli** : « oublie ça » pose une pierre tombale (`memory_forget`), qui
  exclut le nœud de toute récupération.
- **Hygiène** : les faits jamais récupérés, de faible `confidence` et vieux de
  plusieurs mois sont archivés périodiquement.

## 6. Données sensibles

La position et l'agenda entrent dans le contexte (`VISION.md` §2), pas dans la
mémoire longue. On ne mémorise que le fait dérivé et stable (« domicile dans
tel rayon »), jamais l'historique brut des positions. Aucun corps de nœud n'est
journalisé au niveau `info`.

## 7. Stockage : décision différée

Le modèle ci-dessus se projette sans perte sur les deux candidats, et
l'interface MCP (`memory_search`, `memory_write`, `memory_forget`) est
identique dans les deux cas — le stockage reste donc remplaçable.

**SQLite** : tables `nodes` et `edges`, index `nodes_fts` (FTS5, tokenizer
unicode61 pour le français), vecteurs optionnels plus tard. Aucune
administration, aucun service, fichier unique sauvegardable.

**Elasticsearch local** : index `nestor-nodes` et `nestor-edges`, analyseur
français, recherche hybride BM25 + kNN dans un seul système, filtres et
facettes sur `kind`, `context`, `tags`, introspection par REST ou Kibana. En
contrepartie : une JVM permanente (1 à 2 Go) et un service à faire vivre.

Dans les deux cas, **la traversée du graphe est à écrire dans nestord** : ni
SQLite ni Elasticsearch ne la fournissent (Elasticsearch n'a pas d'API de
graphe utilisable ici). Ce n'est pas un argument pour l'un ou pour l'autre,
c'est un travail incontournable : une requête par saut, en limitant le
nombre de voisins ramenés.

Le choix se joue donc sur l'exploitation, pas sur la capacité : SQLite ne
demande rien, Elasticsearch coûte un service mais offre une mémoire
inspectable et de la place pour indexer bien plus tard (code, mails,
documents).

## 8. État de l'implémentation (2026-10-09)

**Stockage retenu : SQLite** (`~/.local/share/nestord/memory.db`), avec
`rusqlite` déjà présent pour les tâches. Tables `nodes` et `edges`, index
`nodes_fts` (FTS5, `unicode61 remove_diacritics 2`), tenu à jour par
déclencheurs. Aucun service ; l'interface MCP reste celle décrite ici, donc le
stockage est remplaçable.

Fait :

- Nœud et arête du §2 et §3 (`kind`, `context`, `tags`, `source`,
  `confidence`, `valid_until`, pierre tombale `forgotten`, compteur `recalled`).
- Outils MCP `memory_write` (avec `replaces` et `links`), `memory_search`
  (`query`, `context`, `kinds`, `limit`, `include_history`) et `memory_forget`.
- Récupération du §4 : FTS5 (chaque mot compte, aucun n'est obligatoire), un
  saut de graphe depuis les trois meilleurs résultats, reclassement
  pertinence × fraîcheur × confiance × contexte, écart des périmés sauf
  historique, troncature au budget de 1 500 caractères en préférant la
  diversité des types.
- Écriture du §5 : dédoublonnage par titre normalisé dans un contexte (mise à
  jour, confiance + 1), contradiction par `replaces` (`valid_until` sur
  l'ancien, arête `remplace`), oubli par pierre tombale.
- Fiche de démarrage : dix lignes au plus (`person`, `preference`, `project`
  par confiance), ajoutée au prompt système à chaque lancement de la session,
  donc aussi à une relance. Seule mémoire poussée.
- Capture sans modèle : chaque mission terminée devient un nœud `mission`
  (objet, résumé borné, source `mission:<id>`).
- Consigne de prompt : écrire les faits durables, chercher avant de répondre
  sur le passé, oublier sur demande, contredire par `replaces`.

Reste :

- Capture par mission de fond des faits d'un tour de conversation (§5) : pour
  l'instant seule la session écrit, explicitement.
- Hygiène périodique (archivage des faits jamais rappelés, faibles et vieux).
- Vecteurs : la recherche est lexicale seulement.
- Aucun écran dans l'interface web ; `memory_search` est le seul accès.
