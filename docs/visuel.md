# Système visuel

Nestor est un majordome, pas un cockpit. L'interface web et l'application
mobile partagent le même système, défini en un seul endroit par plateforme :
`web/src/index.css` (jetons Tailwind v4, `@theme`) et `mobile/src/theme.ts`.

## Principes

- **Fond encre, texte ivoire, accent laiton.** Le laiton marque ce qui compte
  (titre de carte, action principale, voix de Nestor) ; il n'est jamais
  décoratif.
- **Serif pour le nom et les titres** (Fraunces sur le web, serif de
  l'appareil sur mobile), humaniste pour le corps, monospace réservé aux
  nombres, identifiants et commandes. Plus de capitales espacées ni de
  halos lumineux.
- **Quatre états de parole assourdis** : écoute en sarcelle, réflexion en
  violet, parole en laiton, veille en ivoire éteinte. L'orbe, la pastille
  d'état et la barre de niveau suivent les mêmes couleurs sur le web et le
  mobile.
- **Trois tons de signal** : alerte (ambre) pour ce qui attend l'utilisateur,
  danger (rose) pour interrompre ou refuser, ok (sauge) pour ce qui est en
  place.
- **Une valeur inconnue est dite inconnue**, en ivoire éteinte, jamais
  inventée.

## Web

- Colonne gauche : la journée de Monsieur. « Situation » met l'heure et le
  prochain rendez-vous en tête, puis lieu, présence, veille de la machine.
  « Alertes » montre ce que Nestor a signalé de lui-même. « Tâches ».
- Centre : orbe, bloc Voix, dialogue (les réponses de Nestor en serif), journal.
- Colonne droite : missions, conscience, appareils, système (quotas en jauge,
  mémoire longue, latences, modèles).
- Classes partagées dans `index.css` : `card`, `card-title`, `row-label`,
  `pill`, `btn`, `btn-primary`, `btn-quiet`, `btn-icon`, `field`, `attention`.

## Mobile

- Accueil : orbe, nom, devise, bouton d'appel laiton en premier, puis une
  carte « Hors appel » (joignable, position) et une carte « Daemon » repliée
  une fois l'adresse réglée.
- Appel : même palette, bulles de dialogue sobres, bouton de fin d'appel rose.

## Règles d'accessibilité tenues

- Texte courant et libellés : contraste d'au moins 4,5 sur le fond (ivoire 100,
  300 et 500) ; l'ivoire 700 (`#7a7466`), le plus éteint, reste au-dessus de 4,5
  et sert aux valeurs inconnues et aux aides.
- Contours des composants interactifs (boutons, champs, capsules) en
  `ink-500` (`#5b6780`), 3:1 sur le fond ; les cartes gardent un contour plus
  discret, décoratif.
- Cibles tactiles de 44 px, 36 px au minimum pour les actions secondaires.
- Chaque état de couleur est doublé d'un mot.
- `prefers-reduced-motion` (web) et le réglage de mouvement réduit (Android)
  figent la respiration des pastilles et l'orbe.
- Les boutons de l'en-tête web portent leur étiquette dès 640 px ; sous
  1280 px, un bandeau rappelle l'heure, le prochain rendez-vous, le lieu et la
  présence au-dessus du dialogue.
