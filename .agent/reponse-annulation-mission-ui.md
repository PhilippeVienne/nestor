# Reponse au rapport « appels d'outils affiches en cours apres annulation »

Date : 2026-09-12
Repond a : `.agent/rapport-annulation-mission-ui.md`

## Diagnostic confirme, et plus large que le symptome observe

L'hypothese du rapport etait juste : `nestord` arretait le sous-agent sans
refermer les `tool_use` restes ouverts. Mais la cause va au-dela de
l'annulation.

Le parseur du flux Claude (`mission.rs`) n'interpretait que les messages
`assistant` : il emettait un `tool_call` en `running` a chaque `tool_use` et
**ne lisait jamais les `tool_result`**, qui arrivent dans les messages de type
`user` du flux stream-json. Consequence : les appels d'outils d'une mission
`claude` restaient ouverts dans l'UI **meme quand la mission reussissait**.
Le cas visible etait l'annulation, mais le meme defaut touchait les missions
terminees et celles en echec.

A noter : le backend `agy` n'etait pas concerne, parce qu'il publie l'etat
`DONE` de ses outils, que le parseur exploitait deja.

## Corrections appliquees

1. `parse_claude_line` traite desormais les messages `user` et leurs blocs
   `tool_result` : chaque outil est associe a son `tool_use_id` puis clos en
   `completed` a l'arrivee de son resultat.
2. Les outils encore ouverts sont suivis dans `StreamOutcome::open_tools` et
   refermes explicitement des que le flux s'arrete, quelle qu'en soit la
   raison (annulation, process tue, flux tronque, echec). La cloture se fait
   en `completed` plutot qu'avec un statut d'outil dedie : c'est le statut de
   la *mission* qui porte l'information d'annulation, et les autres clients
   (mobile) n'ont pas a connaitre un nouveau statut.
3. Filet de securite cote web, comme suggere au point 2 du rapport : quand une
   mission quitte l'etat `started`, l'UI referme d'office ses appels d'outils
   encore en cours. Le meme filet reste a ajouter cote mobile.

## Ce qui manquait aussi, et qui est desormais en place

Le rapport signalait implicitement le vrai manque : « annulee avant la fin »
sans rien d'autre.

- **Avancement** : l'evenement `mission` porte un champ `progress` alimente a
  chaque appel d'outil du sous-agent (« Read : nestord/src/audio/vad.rs »).
  `list_missions` le renvoie aussi, donc la question « ou en est la mission ? »
  a maintenant une reponse vocale.
- **Motif d'annulation** : `stop_mission` accepte un `reason`, repris dans le
  compte rendu. L'UI envoie « arret demande depuis l'interface » par defaut.
- **Resultat partiel** : le texte deja produit par le sous-agent n'est plus
  jete, il est joint au compte rendu d'annulation (tronque a 600 caracteres,
  puisqu'il est lu a voix haute).

## Verification

Test de bout en bout : mission d'audit lancee, 14 evenements de progression
recus (commandes et fichiers lus), annulation avec motif au bout de 20 s.
Resultat : statut `cancelled`, compte rendu « annulee avant la fin (test
d'annulation motivee). Travail deja produit : Je commence par localiser les
fichiers du daemon. », et **aucun** appel d'outil laisse ouvert.

Le test suggere au point 3 du rapport (mission avec commande longue puis
annulation) est couvert par ce scenario ; il n'est pas encore automatise.
