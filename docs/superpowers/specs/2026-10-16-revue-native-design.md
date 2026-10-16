# Revue native dans le panneau de droite (OshO Squad, retour C)

Date : 2026-10-16 · Statut : à valider

## But

Un agent qui termine un plan, une spec ou un bilan publie une page à cocher dans le panneau de droite d'OshO Squad. L'utilisateur y répond et clique sur « Envoyer », et l'agent reçoit les réponses et reprend. Cette page remplace l'artefact claude.ai du skill `revue-artefact` (osho-core), qui n'existe pas dans une session lancée par Escouade.

Réussi quand : un agent appelle `show_review`, la page s'ouvre à droite avec la recommandation précochée, « Envoyer » relance l'agent avec les réponses, et la page reste consultable dans la conversation.

## Décisions prises

- La page arrive par un **outil MCP** et utilise le **format JSON de revue-artefact**. Pas de HTML libre.
- L'outil **répond tout de suite** : l'agent termine son tour et attend. Les réponses arrivent comme un message.
- La page est **dessinée par OshO Squad** (Svelte), sans iframe ni changement de CSP.
- Le code reste dans le fork, écrit pour pouvoir être proposé au dev plus tard : style du dépôt, i18n `tr!`/`t()`, tests, et des fichiers nouveaux plutôt que des retouches dans `core.rs`/`agent.rs`.

## Format accepté (celui de revue-artefact)

```json
{
  "titre": "…", "sur": "…", "chapo": "…",
  "puces": [{ "texte": "…", "ton": "alerte" }],
  "sections": [{ "titre": "…", "ton": "neutre", "items": ["…"], "paragraphes": ["…"] }],
  "titreQuestions": "…",
  "questions": [
    { "id": "q1", "titre": "…", "contexte": "…",
      "choix": [{ "valeur": "a", "libelle": "…", "detail": "…", "reco": true }] },
    { "id": "q2", "titre": "…", "placeholder": "…" }
  ]
}
```

- `titre` et `questions` sont obligatoires, le reste est facultatif. `ton` vaut `alerte`, `neutre` ou rien. Une question sans `choix` est un champ texte.
- Le champ texte « Autre chose ? » (`note`) est toujours ajouté.
- Validation à l'entrée de l'outil : titre non vide, de 1 à 20 questions, `id` uniques, au plus 50 Ko. Sinon l'outil renvoie une erreur lisible pour que l'agent corrige.

## Composants

1. **Outil MCP `show_review`** (`src-tauri/src/mcp/`, implémentation dans un nouveau `review.rs`) :
   - réservé à un agent d'Escouade (`Caller::Agent`), pour sa propre conversation ; un appel externe est refusé ;
   - entrée : le JSON ci-dessus ;
   - effet : ajoute à la conversation un élément `review` `{ id, contenu, statut: "attente", reponses: null, ts }` ;
   - réponse : « Page affichée. Termine ton tour : les réponses arriveront dans un message. »
2. **Élément de conversation `review`** (types TS + journal `conv`) :
   - il est gardé dans le journal, comme les autres éléments ;
   - tant qu'il est en attente, l'agent compte comme « attend une réponse » : pastille ambre et notification, même mécanisme que les questions (`pending`) ;
   - il est affiché dans la conversation par une carte `ReviewCard.svelte` « Revue : <titre> · en attente / répondue » ; un clic ouvre la page à droite.
3. **Onglet « Revue » du panneau de droite** (`SidePanel.svelte`, `panelTab: 'review'`, composant `ReviewPage.svelte`) :
   - il affiche la revue choisie, sinon la dernière revue en attente de l'agent courant ;
   - il s'ouvre tout seul (et active la disposition moitié / moitié) quand une revue arrive pour l'agent affiché ;
   - rendu : chapô, puces (rouge pour `alerte`), sections, questions en boutons radio avec un badge « recommandé » et la recommandation précochée, champs texte, « Autre chose ? », bouton « Envoyer » ;
   - couleurs : celles du thème et de la couleur du projet ;
   - une fois répondue, la page est en lecture seule et montre les réponses.
4. **Envoyer** :
   - nouvelle commande `answer_review(agent, review, reponses)` ;
   - elle passe l'élément à `repondue` avec ses `reponses`, puis envoie à l'agent un message lisible :
     ```
     Réponses à « <titre> » :
     - <titre q1> : <libellé choisi> (<valeur>)
     - <titre q2> : <texte>
     - Autre chose : <note>
     ```
   - le message passe par `send_message` : en vue Escouade il relance le tour, en vue Claude Code il est tapé dans le terminal ;
   - une seconde réponse à la même revue est refusée.
5. **Skill `revue-artefact`** (dépôt OshO-Skillz, hors de ce dépôt) : une ligne en tête de la méthode, « Si l'outil MCP `show_review` est disponible (session OshO Squad), l'appeler avec le contenu JSON au lieu de publier un artefact ; les réponses arrivent comme un message ». Le reste du skill ne change pas.

## Erreurs et cas limites

- JSON invalide : erreur renvoyée à l'agent, rien n'est affiché.
- Agent arrêté ou en erreur au moment d'« Envoyer » : les réponses sont gardées sur l'élément, le message est livré par le chemin habituel, qui relance le processus. En cas d'échec, un toast s'affiche et la revue reste en attente.
- Plusieurs revues en attente : la carte de chaque revue reste cliquable, et l'onglet montre la plus récente.
- Application redémarrée : la revue est relue depuis le journal, avec son statut.
- Les réponses sont des données : elles ne sont jamais exécutées ni interprétées par l'app.

## Hors périmètre

HTML libre, maquettes, fenêtre séparée, modification d'une revue après envoi, revues adressées à un autre agent, publication sur claude.ai.

## Tests

- Rust : validation du JSON (cas valides et invalides), refus d'un appel externe, création de l'élément `review` et passage à « attend une réponse », `answer_review` (message produit, double réponse refusée).
- Vitest : `ReviewPage` (recommandation précochée, champ texte, « Envoyer » appelle `answer_review` avec les bonnes valeurs, lecture seule une fois répondue) et `ReviewCard` (ouvre l'onglet).
- Manuel : un agent d'OshO Squad appelle `show_review`, puis on répond et on vérifie qu'il reprend.
