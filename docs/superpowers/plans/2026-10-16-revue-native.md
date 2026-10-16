# Revue native dans le panneau de droite : plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** un agent appelle l'outil MCP `show_review` avec le JSON de revue-artefact ; la page s'ouvre dans le panneau de droite, l'utilisateur coche et clique « Envoyer », et l'agent reçoit ses réponses comme un message de l'utilisateur.

**Architecture:** la revue est un élément `review` du journal de conversation (`conv.rs`, append puis patch), créé par un nouveau `src-tauri/src/review.rs` (modèle, validation, message de réponse, méthodes `AgentRt`/`Core`). L'attente n'utilise **pas** `AgentStatus::Waiting` (lié aux requêtes du processus et traité comme « actif » partout) : l'agent garde `Done`, et `AgentMeta.reviews` (ids des revues en attente, sauvegardé avec l'agent) suffit à l'afficher en ambre, à compter dans la barre de menus et à transformer la notification de fin de tour en notification de question. Le front affiche la carte `ReviewCard` dans la conversation et la page `ReviewPage` dans un troisième onglet de `SidePanel`.

**Tech Stack:** Rust (Tauri 2, rmcp 3.5 + schemars, serde_json, anyhow, tokio), Svelte 5.56 (runes), TypeScript, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-16-revue-native-design.md`

## Global Constraints

- Format d'entrée : celui de revue-artefact (`titre`, `sur`, `chapo`, `puces[{texte, ton}]`, `sections[{titre, ton, items, paragraphes}]`, `titreQuestions`, `questions[{id, titre, contexte, choix[{valeur, libelle, detail, reco}], placeholder}]`). Pas de HTML libre, pas d'iframe, pas de changement de CSP.
- `titre` et `questions` obligatoires ; `ton` vaut `alerte`, `neutre` ou rien. **Décision : un `ton` inconnu est ignoré** (ramené à rien), pas refusé.
- Validation à l'entrée de l'outil : titre non vide, de 1 à 20 questions, `id` uniques (comparés sans espaces autour) et non vides, au plus 50 Ko (`50 * 1024` octets du JSON réécrit). Sinon erreur lisible renvoyée à l'agent, rien n'est affiché.
- Réponse de l'outil : « Page affichée. Termine ton tour : les réponses arriveront dans un message. »
- Élément de conversation : `{ kind: "review", id, contenu, statut: "attente" | "repondue", reponses: null | { questions: {id: valeur|texte}, note }, ts }`.
- Message envoyé à l'agent (langue des textes pour Claude, `i18n::claude()`) :
  ```
  Réponses à « <titre> » :
  - <titre q1> : <libellé choisi> (<valeur>)
  - <titre q2> : <texte>
  - Autre chose : <note>
  ```
- Le message passe par `Core::send_message` (vue Escouade : relance le tour ; vue Claude Code : tapé dans le terminal). Une seconde réponse à la même revue est refusée.
- `show_review` est réservé à `Caller::Agent` (sa propre conversation) ; `Caller::External` est refusé.
- Style du dépôt : doc comments anglais courts, textes Rust via `tr!("fr", "en")` / `tr_in!(lang, …)`, textes du front via `t('clé')` + `src/lib/i18n/fr/*.ts` et `en/*.ts` (mêmes clés, `npm run check` vérifie). Nouveaux fichiers plutôt que retouches dans `core.rs` / `agent.rs` (seules retouches : 2 visibilités et la notification de fin de tour dans `agent.rs`, une ligne de `update_tray` dans `core.rs`).
- Ne pas toucher `Composer.svelte`, `ToolRow.svelte`, `Core::send` (`core.rs` ~l.2965), ni créer `bang.rs` : un autre agent y travaille.
- Ne jamais toucher `tauri.conf.json`, `paths.rs` (`DATA_DIR_NAME`, `IDENTIFIER`), `mcp/install.rs` (`NAME = "osho-squad"`).
- Les réponses sont des données : jamais exécutées ni interprétées (texte brut affiché, pas de Markdown ni de `{@html}`).

Commandes de test (à utiliser telles quelles) :

```bash
# Rust
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_TARGET_DIR="/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri/target"
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test <filtre>
# Front
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run <fichier>
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npm run check
```

Des tests Rust existants échouent à cause du renommage du fork (`.osho-squad` / `osho-squad` au lieu de `.escouade` / `escouade`, par exemple `claude_outside_escouade_initializes_and_lists_the_tools_with_its_token` qui attend le nom de serveur `escouade`) : c'est attendu, ne pas les « corriger ». Ne regarder que les tests filtrés de chaque tâche.

## Review Focus

1. L'utilisateur tape un message normal au lieu de répondre : la revue reste en attente, le tour suivant se termine et l'agent reste ambre avec une notification « Revue à remplir » ; la carte et la page restent utilisables. Test : Task 2, `a_review_left_unanswered_still_waits_after_the_next_turn`.
2. Redémarrage de l'app : la revue est relue depuis le journal avec son statut, et l'agent reste « en attente » (`AgentMeta.reviews` sauvegardé). Test : Task 2, `a_waiting_review_survives_a_restart`.
3. Double clic sur « Envoyer » (ou deux fenêtres) : un seul message part, la seconde réponse est refusée sous le verrou de l'agent. Tests : Task 3 (`…only_once`) et Task 6 (bouton désactivé pendant l'envoi).
4. Envoi impossible (agent en vue Claude Code sans terminal ouvert, Claude introuvable) : toast, la revue repasse en attente avec les réponses gardées et pré-remplies. Tests : Task 3 (`answers_that_cannot_reach_the_agent_leave_the_review_waiting_with_them`) et Task 6 (`tells a send that failed`).
5. JSON valide mais tordu (deux `choix` de même `valeur`, `ton` inconnu, texte multi-ligne) : la page ne plante pas (clés d'`#each` par index pour les choix), le `ton` inconnu s'affiche neutre. Tests : Task 1 (`ton` ramené à rien) et Task 6 (`survives two choices of the same value`).

---

## Carte du code (où l'on se branche)

| Sujet | Où | Ce qu'on en fait |
|---|---|---|
| Déclaration d'un outil MCP | `src-tauri/src/mcp/tools.rs:737-1262` (`#[tool_router]`, `#[tool(description, annotations)]`, `Parameters<T>`, `ctx.reply`) ; `report_progress` l.1223-1239 | modèle de `show_review` |
| Appelant | `mcp/mod.rs:66-71` (`Caller::External` / `Caller::Agent(id)`) ; `act.rs:87-96` `agent_only` | refus des appels externes |
| Action d'un outil | `act.rs:1702-1724` `report_progress` (`agent_only`, `ToolError::Failed`, `core.emit_agent`) | modèle de `act::show_review` |
| Listes d'outils testées | `mcp/tests.rs:24-52` `EXPOSED`, l.95 `AGENTS_ONLY` ; `act_tests.rs:99-138` (schémas : chaque propriété décrite, `readOnlyHint=false`, `openWorldHint=false`), l.140-157 `required`, l.1251-1275 refus externe | à compléter |
| Helpers de test MCP | `mcp/tools_tests.rs:86` `put_agent`, l.116 `external`, l.122 `as_agent`, l.130 `call`, l.148 `refused` | réutilisés |
| Append/patch d'un élément | `agent.rs:493-510` `push` / `append` / `patch` (privés) → `conv.rs:80-94` `Conv::apply` (journal JSONL, `persist`) ; `core.rs:1979-2031` `Core::apply` (émet `UiEvent::Conv` puis `UiEvent::Agent`, sauvegarde, notifie) ; `core.rs:3088-3106` `with_agent` | `append`/`patch` passent `pub(crate)` |
| Relecture au démarrage | `conv.rs:215-260` `replay` (append + patch rejoués) ; `agent.rs:298-302` `AgentRt::new` remet un statut actif à `Done` | la revue revient avec son `statut` ; `reviews` revient avec `AgentMeta` |
| Requêtes en attente | `agent.rs:176` `pending` (vidé par `attach` l.424, `on_exit` l.814, la fin de tour l.1654) ; `set_status(Waiting)` l.1723 ; `is_active()` = Running/Waiting (`model.rs:727`) utilisé par `push_user` l.586 (message mis en file), `on_exit`, l'arrêt au repos (`core.rs:2581`), les comptes, les tickets | **ne pas** réutiliser pour la revue |
| Fin de tour | `agent.rs:1654-1677` (`set_status(Done)`, `fx.notify` Done/Error) ; vue Claude Code : `agent/transcript.rs:215-247` `end_tui_turn` → même `on_result` | la notification devient « Question » si une revue attend |
| Notifications | `agent.rs:25-50` `NotifyKind`, `AgentAlert` (champs publics) ; `core.rs:2018-2024` (une `Question` passe même pour un agent de ticket) ; `core.rs:2285-2297` `update_tray` | badge de la barre de menus |
| Envoi du message | `core.rs:2930-2937` `send_message` (= texte de l'utilisateur, peut nommer l'agent) ; `core.rs:2953-2955` `send_by_app` ; `tui.rs:804-821` `send_to_tui` échoue sans terminal ouvert | `send_message` |
| Commandes Tauri | `commands.rs:413-422` `send_message`, l.439-442 `answer_question` ; `lib.rs:279-310` `generate_handler!` | `answer_review` |
| Types TS | `src/lib/types.ts:500-560` `Agent` ; l.913-1005 `ConvItem`, `ConvOp` | `ReviewItem`, `Agent.reviews` |
| Ops côté front | `state.svelte.ts:546-548` → `conversations.svelte.ts:124-152` `Conversation.apply` | rien à changer |
| Rendu de la conversation | `Conversation.svelte:640-700` (`{#if item.kind === …}`), `shows()` l.450-454 renvoie `true` pour un kind inconnu | branche `review` |
| Panneau de droite | `SidePanel.svelte` (onglets files/history), `state.svelte.ts:221` `panelTab`, l.265 `split`, l.706-709 `toggleLayout`, `App.svelte:111-115` | onglet `review` |
| Attention / ambre | `state.svelte.ts:154` `ALERT`, l.637-649 `noteAttention`, l.1022-1031 `nextWaiting` ; `Sidebar.svelte:287,295,328-329` ; `TitleBar.svelte:172-174` ; `lib/tab-strip.ts:31` `alertColor` | via `shownStatus()` |
| Libellés d'outils | `src/lib/tools.ts:18-39` `ESCOUADE_TOOLS` ; `i18n/fr/mcp.ts:40-67` | `showReview` |
| Couleurs | `app.css:24-26` `--accent` / `--accent-soft` (= couleur du projet, posée par `lib/theme.ts:19-20`), `--wait`, `--wait-soft`, `--del`, `--panel`, `--elev`, `--line`, `--text`, `--muted`, `--dim`, `--r`, `--r-sm` | styles des composants |
| Tests front | `src/test/ipc.ts:19` `fakeBackend`, l.97 `agent()`, l.255 `resetApp` ; modèles : `components/conv/QuestionCard.test.ts`, `components/SidePanel.test.ts`, `lib/state.test.ts:14-45` `start()`/`emit` | réutilisés |

## Fichiers

- Créer : `src-tauri/src/review.rs` (modèle, validation, message, `impl AgentRt`, `impl Core`, tests)
- Modifier : `src-tauri/src/lib.rs`, `model.rs`, `agent.rs`, `core.rs` (1 ligne), `commands.rs`, `mcp/tools.rs`, `mcp/act.rs`, `mcp/tests.rs`, `mcp/act_tests.rs`
- Créer : `src/lib/review.ts`, `src/lib/review.test.ts`, `src/components/ReviewPage.svelte`, `src/components/ReviewPage.test.ts`, `src/components/conv/ReviewCard.svelte`, `src/components/conv/ReviewCard.test.ts`
- Modifier : `src/lib/types.ts`, `src/lib/ipc.ts`, `src/lib/tools.ts`, `src/lib/state.svelte.ts`, `src/lib/state.test.ts`, `src/test/ipc.ts`, `src/components/SidePanel.svelte`, `src/components/SidePanel.test.ts`, `src/components/Conversation.svelte`, `src/components/Sidebar.svelte`, `src/components/Sidebar.test.ts`, `src/components/TitleBar.svelte`, `src/lib/i18n/{fr,en}/{conv,nav,mcp}.ts`

---

### Task 1 : modèle de revue, validation et message de réponse (Rust, pur)

**Files:**
- Create: `src-tauri/src/review.rs`
- Modify: `src-tauri/src/lib.rs:42` (déclarer le module)
- Test: `src-tauri/src/review.rs` (`mod tests`)

**Interfaces:**
- Consumes : `crate::i18n::Lang`, macros `tr!` / `tr_in!` (`i18n.rs:149-206`).
- Produces :
  - `pub struct Review { titre: String, sur: Option<String>, chapo: Option<String>, puces: Vec<Bullet>, sections: Vec<Section>, titre_questions: Option<String> /* JSON: titreQuestions */, questions: Vec<Question> }` (`Serialize + Deserialize + schemars::JsonSchema`)
  - `pub struct Bullet { texte: String, ton: Option<String> }`, `pub struct Section { titre, ton: Option<String>, items: Vec<String>, paragraphes: Vec<String> }`, `pub struct Question { id, titre, contexte: Option<String>, choix: Vec<Choice>, placeholder: Option<String> }`, `pub struct Choice { valeur, libelle, detail: Option<String>, reco: bool }`
  - `pub struct Answers { questions: BTreeMap<String, String>, note: String }`
  - `pub const QUESTIONS_MAX: usize = 20; pub const BYTES_MAX: usize = 50 * 1024;`
  - `pub fn checked(review: Review) -> Result<Review, String>`
  - `pub fn answer_text(lang: Lang, review: &Review, answers: &Answers) -> String`

- [ ] **Step 1 : déclarer le module**

Dans `src-tauri/src/lib.rs`, après `mod resources;` (l.42) :

```rust
mod review;
```

- [ ] **Step 2 : écrire les tests qui échouent**

Créer `src-tauri/src/review.rs` avec seulement le module de tests (le reste vient à l'étape 4) :

```rust
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// The review of the spec's example: a choice with its recommendation, a free text.
    pub(crate) fn review() -> Review {
        serde_json::from_value(json!({
            "titre": "Plan du parseur", "sur": "Plan", "chapo": "Trois lots.",
            "puces": [
                { "texte": "Migration risquée", "ton": "alerte" },
                { "texte": "Tests verts", "ton": "rouge" }
            ],
            "sections": [{ "titre": "Lots", "items": ["Lot 1"] }],
            "questions": [
                { "id": "q1", "titre": "Exécution ?", "choix": [
                    { "valeur": "sub", "libelle": "Sous-agents", "reco": true },
                    { "valeur": "inline", "libelle": "Ici", "detail": "Plus lent" }
                ] },
                { "id": "q2", "titre": "Nom du module ?", "placeholder": "parser" }
            ]
        }))
        .unwrap()
    }

    pub(crate) fn answers() -> Answers {
        Answers {
            questions: [("q1".into(), "inline".into()), ("q2".into(), "lexer".into())].into(),
            note: "Merci".into(),
        }
    }

    fn questions(n: usize) -> Vec<Question> {
        (0..n)
            .map(|i| Question {
                id: format!("q{i}"),
                titre: "Question".into(),
                contexte: None,
                choix: Vec::new(),
                placeholder: None,
            })
            .collect()
    }

    #[test]
    fn a_review_of_the_skills_format_is_taken_and_an_unknown_tone_is_dropped() {
        let r = checked(review()).unwrap();
        assert_eq!(r.titre, "Plan du parseur");
        assert_eq!(r.puces[0].ton.as_deref(), Some("alerte"));
        // « rouge » is no tone: the bullet shows plainly.
        assert_eq!(r.puces[1].ton, None);
        assert!(r.questions[0].choix[0].reco);
        assert!(r.questions[1].choix.is_empty());
        // Its JSON keeps the skill's names.
        let v = serde_json::to_value(&r).unwrap();
        assert!(v.get("titreQuestions").is_some(), "{v}");
    }

    #[test]
    fn a_review_that_cannot_be_shown_is_refused_in_words_the_agent_can_act_on() {
        let blank = Review { titre: "  ".into(), ..review() };
        assert!(checked(blank).unwrap_err().contains("titre"));

        let none = Review { questions: Vec::new(), ..review() };
        let why = checked(none).unwrap_err();
        assert!(why.contains("0 questions") && why.contains("1 à 20"), "{why}");

        let many = Review { questions: questions(21), ..review() };
        assert!(checked(many).unwrap_err().contains("21 questions"));
        assert!(checked(Review { questions: questions(20), ..review() }).is_ok());

        let mut twice = questions(2);
        twice[1].id = " q0 ".into();
        let why = checked(Review { questions: twice, ..review() }).unwrap_err();
        assert!(why.contains("« q0 »"), "{why}");

        let mut nameless = questions(1);
        nameless[0].id = " ".into();
        assert!(checked(Review { questions: nameless, ..review() }).unwrap_err().contains("id"));

        let heavy = Review { chapo: Some("x".repeat(BYTES_MAX)), ..review() };
        let why = checked(heavy).unwrap_err();
        assert!(why.contains("50 Ko"), "{why}");
    }

    #[test]
    fn the_answers_read_as_the_spec_writes_them() {
        assert_eq!(
            answer_text(Lang::Fr, &review(), &answers()),
            "Réponses à « Plan du parseur » :\n\
             - Exécution ? : Ici (inline)\n\
             - Nom du module ? : lexer\n\
             - Autre chose : Merci"
        );
        assert_eq!(
            answer_text(Lang::En, &review(), &answers()),
            "Answers to “Plan du parseur”:\n\
             - Exécution ?: Ici (inline)\n\
             - Nom du module ?: lexer\n\
             - Anything else: Merci"
        );
    }

    #[test]
    fn a_question_left_blank_says_so_and_an_empty_note_is_left_out() {
        let partial = Answers {
            questions: [("q1".into(), "sub".into()), ("q2".into(), "   ".into())].into(),
            note: " ".into(),
        };
        assert_eq!(
            answer_text(Lang::Fr, &review(), &partial),
            "Réponses à « Plan du parseur » :\n\
             - Exécution ? : Sous-agents (sub)\n\
             - Nom du module ? : (sans réponse)"
        );
    }
}
```

- [ ] **Step 3 : lancer les tests, constater l'échec**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test review::tests`
Expected : erreur de compilation `cannot find type Review in this scope`.

- [ ] **Step 4 : écrire le modèle, la validation et le message**

En tête de `src-tauri/src/review.rs`, au-dessus du module de tests :

```rust
//! A review an agent publishes for the user (`show_review`, a tool of the MCP server): a page of
//! questions shown in the side panel, whose answers come back to the agent as the user's message.
//! The page is the item `review` of its conversation: waiting (`attente`) until answered
//! (`repondue`). The answers are data: shown and sent as text, never run.

use crate::i18n::Lang;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// The most questions a review asks.
pub const QUESTIONS_MAX: usize = 20;
/// The most bytes a review takes, as JSON.
pub const BYTES_MAX: usize = 50 * 1024;

/// What `show_review` is given: the JSON format of the skill `revue-artefact`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    #[schemars(description = "The page's title, on one line: what the user is asked to review.")]
    pub titre: String,
    #[schemars(description = "What it is about (a plan, a spec, a report), as a short line.")]
    pub sur: Option<String>,
    #[schemars(description = "The opening paragraph, the summary the user reads first.")]
    pub chapo: Option<String>,
    #[schemars(description = "Key points, each a short text (texte); ton \"alerte\" shows it in red.")]
    #[serde(default)]
    pub puces: Vec<Bullet>,
    #[schemars(description = "Sections, each with a title (titre) and its items or paragraphs (paragraphes).")]
    #[serde(default)]
    pub sections: Vec<Section>,
    #[schemars(description = "The heading above the questions (Questions when left out).")]
    pub titre_questions: Option<String>,
    #[schemars(description = "The questions, 1 to 20, each with an id of its own: with choix, a choice (reco true marks the recommended one, preselected); without, a free text.")]
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(inline)]
pub struct Bullet {
    pub texte: String,
    /// "alerte" (red) or "neutre"; another value is dropped (`checked`).
    pub ton: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(inline)]
pub struct Section {
    pub titre: String,
    pub ton: Option<String>,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub paragraphes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(inline)]
pub struct Question {
    pub id: String,
    pub titre: String,
    pub contexte: Option<String>,
    /// None: the question is a free text.
    #[serde(default)]
    pub choix: Vec<Choice>,
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(inline)]
pub struct Choice {
    pub valeur: String,
    pub libelle: String,
    pub detail: Option<String>,
    /// The recommended choice: marked, and checked before the user picks.
    #[serde(default)]
    pub reco: bool,
}

/// The user's answers: by question id, a choice's `valeur` or a text; and « Autre chose ? ».
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Answers {
    #[serde(default)]
    pub questions: BTreeMap<String, String>,
    #[serde(default)]
    pub note: String,
}

/// `review` as it may be shown, or why not, in words the agent reads to correct it. A `ton` other
/// than « alerte » or « neutre » is dropped: the text shows plainly.
pub fn checked(mut review: Review) -> Result<Review, String> {
    if review.titre.trim().is_empty() {
        return Err(tr!(
            "Le titre de la revue est vide.",
            "The review's title (titre) is empty."
        ));
    }
    let n = review.questions.len();
    if !(1..=QUESTIONS_MAX).contains(&n) {
        return Err(tr!(
            "La revue pose {n} questions : de 1 à {QUESTIONS_MAX}.",
            "The review asks {n} questions: 1 to {QUESTIONS_MAX}."
        ));
    }
    let mut ids = HashSet::new();
    for q in &review.questions {
        let id = q.id.trim();
        if id.is_empty() {
            return Err(tr!(
                "Une question n’a pas d’id.",
                "A question has no id."
            ));
        }
        if !ids.insert(id) {
            return Err(tr!(
                "L’id « {id} » est donné à plusieurs questions : chacune a le sien.",
                "The id \"{id}\" is given to several questions: each needs its own."
            ));
        }
    }
    let tone = |t: &mut Option<String>| {
        if !matches!(t.as_deref(), Some("alerte" | "neutre")) {
            *t = None;
        }
    };
    review.puces.iter_mut().for_each(|p| tone(&mut p.ton));
    review.sections.iter_mut().for_each(|s| tone(&mut s.ton));
    let bytes = serde_json::to_vec(&review).map_or(usize::MAX, |v| v.len());
    if bytes > BYTES_MAX {
        let kb = bytes / 1024;
        return Err(tr!(
            "La revue fait {kb} Ko : 50 Ko au plus. Raccourcis-la.",
            "The review is {kb} KB: 50 KB at most. Shorten it."
        ));
    }
    Ok(review)
}

/// The message that gives the agent the user's answers to `review`, in `lang`: one line per
/// question (a choice by its label and value, a text as written), then the note if any.
pub fn answer_text(lang: Lang, review: &Review, answers: &Answers) -> String {
    let mut lines = vec![tr_in!(
        lang,
        "Réponses à « {} » :",
        "Answers to “{}”:",
        review.titre.trim()
    )];
    for q in &review.questions {
        let given = answers
            .questions
            .get(&q.id)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let said = match given {
            None => tr_in!(lang, "(sans réponse)", "(no answer)"),
            Some(v) => match q.choix.iter().find(|c| c.valeur == v) {
                Some(c) => format!("{} ({})", c.libelle, c.valeur),
                None => v.to_string(),
            },
        };
        lines.push(tr_in!(lang, "- {} : {}", "- {}: {}", q.titre.trim(), said));
    }
    let note = answers.note.trim();
    if !note.is_empty() {
        lines.push(tr_in!(lang, "- Autre chose : {}", "- Anything else: {}", note));
    }
    lines.join("\n")
}
```

- [ ] **Step 5 : lancer les tests**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test review::tests`
Expected : 4 tests PASS. (Avertissement « unused » attendu sur `Answers`/`checked` hors tests jusqu'à la Task 3 : ignorer.)

- [ ] **Step 6 : commit**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
git add src-tauri/src/review.rs src-tauri/src/lib.rs
git commit -m "feat(revue): modèle, validation et message de réponse"
```

---

### Task 2 : l'élément `review` dans la conversation, l'attente et la notification

**Files:**
- Modify: `src-tauri/src/model.rs:794` (champ `reviews` de `AgentMeta`)
- Modify: `src-tauri/src/agent.rs:498,502` (`append`/`patch` en `pub(crate)`), `agent.rs:1669-1674` (notification de fin de tour)
- Modify: `src-tauri/src/core.rs:2289` (`update_tray`)
- Modify: `src-tauri/src/review.rs` (`impl AgentRt`, `awaits`, `waiting_body`, tests)

**Interfaces:**
- Consumes : `Review`, `Answers`, `answer_text` (Task 1) ; `AgentRt`, `Effects`, `AgentAlert { kind, body }`, `NotifyKind` (`agent.rs:25-77`) ; `new_id`, `now_ms` (`model.rs:1341-1347`).
- Produces :
  - `AgentMeta.reviews: Vec<String>` (JSON `reviews`, sauvegardé, `[]` par défaut ; aplati dans `AgentView`, donc reçu par le front comme `Agent.reviews`).
  - `impl AgentRt { pub fn show_review(&mut self, review: &Review, fx: &mut Effects) -> String; pub fn answer_review(&mut self, id: &str, answers: &Answers, fx: &mut Effects) -> anyhow::Result<String>; pub fn reopen_review(&mut self, id: &str, fx: &mut Effects) }`
  - `pub fn awaits(meta: &AgentMeta) -> bool` (une revue attend et le tour est fini : `Done` ou `Idle`)
  - `pub(crate) fn waiting_body(rt: &mut AgentRt) -> Option<String>`

Décision : l'attente d'une revue **n'est pas** `AgentStatus::Waiting`. `Waiting` est « actif » (`model.rs:727`) : `push_user` mettrait le message de réponse en file (`agent.rs:586`), l'arrêt au repos, les comptes, les tickets et `send_to_tui` (`tui.rs:808`, qui refuse en `Waiting`) le prendraient pour un tour en cours, et `AgentRt::new` l'effacerait au redémarrage. L'agent reste donc `Done`, et `reviews` porte l'attente.

- [ ] **Step 1 : écrire les tests qui échouent**

Ajouter dans `mod tests` de `src-tauri/src/review.rs` :

```rust
    use crate::agent::{AgentAlert, AgentRt, Effects, NotifyKind};
    use crate::model::{AgentMeta, AgentStatus, ConvOp};
    use crate::paths::test_dir;

    fn agent(dir: &std::path::Path) -> AgentRt {
        AgentRt::new(
            AgentMeta {
                id: "a1".into(),
                cwd: "C:/p".into(),
                ..Default::default()
            },
            dir,
        )
    }

    /// A turn that ends as Claude Code ends one.
    fn ends(a: &mut AgentRt) -> Effects {
        let mut fx = Effects::default();
        a.handle_frame(
            &json!({"type":"result","subtype":"success","is_error":false,"duration_ms":1,
                "result":"Revue publiée.","modelUsage":{}}),
            &mut fx,
        );
        fx
    }

    #[test]
    fn a_review_shown_is_an_item_of_the_conversation_that_waits_once_the_turn_is_over() {
        let dir = test_dir("review-show");
        let mut a = agent(&dir);
        a.push_user("u1", "Relis le plan", 0, &[], &mut Effects::default());
        let mut fx = Effects::default();
        let id = a.show_review(&checked(review()).unwrap(), &mut fx);
        assert!(
            matches!(&fx.ops[..], [ConvOp::Append { item }] if item["kind"] == "review" && item["id"] == id.as_str()),
            "{:?}",
            fx.ops
        );
        assert!(fx.agent_changed && fx.save);
        let item = a.conv.get(&id).unwrap().clone();
        assert_eq!(item["statut"], "attente");
        assert_eq!(item["reponses"], Value::Null);
        assert_eq!(item["contenu"]["titre"], "Plan du parseur");
        assert_eq!(a.meta.reviews, [id.clone()]);
        // Mid-turn it runs: it waits once the turn is over.
        assert!(!awaits(&a.meta));
        let fx = ends(&mut a);
        assert_eq!(a.meta.status, AgentStatus::Done);
        assert!(awaits(&a.meta));
        // The end of the turn calls for the user, as a question does.
        assert_eq!(
            fx.notify,
            Some(AgentAlert {
                kind: NotifyKind::Question,
                body: "Revue à remplir : Plan du parseur".into()
            })
        );
    }

    #[test]
    fn a_review_left_unanswered_still_waits_after_the_next_turn() {
        let dir = test_dir("review-unanswered");
        let mut a = agent(&dir);
        a.push_user("u1", "Relis", 0, &[], &mut Effects::default());
        let id = a.show_review(&review(), &mut Effects::default());
        ends(&mut a);
        // The user writes something else instead.
        a.push_user("u2", "Autre chose", 0, &[], &mut Effects::default());
        assert!(!awaits(&a.meta));
        let fx = ends(&mut a);
        assert_eq!(a.meta.reviews, [id]);
        assert!(awaits(&a.meta));
        assert_eq!(fx.notify.unwrap().kind, NotifyKind::Question);
    }

    #[test]
    fn a_review_is_answered_once_and_the_message_tells_the_answers() {
        let dir = test_dir("review-answer-once");
        let mut a = agent(&dir);
        let id = a.show_review(&review(), &mut Effects::default());
        let mut fx = Effects::default();
        let text = a.answer_review(&id, &answers(), &mut fx).unwrap();
        assert!(text.contains("Ici (inline)") && text.contains("lexer"), "{text}");
        let item = a.conv.get(&id).unwrap().clone();
        assert_eq!(item["statut"], "repondue");
        assert_eq!(item["reponses"], json!({ "questions": { "q1": "inline", "q2": "lexer" }, "note": "Merci" }));
        assert!(a.meta.reviews.is_empty());
        assert!(fx.agent_changed && fx.save);
        // A second answer is refused, and changes nothing.
        let again = a.answer_review(&id, &Answers::default(), &mut Effects::default());
        assert!(format!("{:#}", again.unwrap_err()).contains("déjà"));
        assert_eq!(a.conv.get(&id).unwrap()["reponses"]["note"], "Merci");
        // An id that is no review of this conversation.
        assert!(a.answer_review("zzz", &answers(), &mut Effects::default()).is_err());
        // A turn that ends after it no longer speaks of the review.
        a.push_user("u1", "Suite", 0, &[], &mut Effects::default());
        assert_eq!(ends(&mut a).notify.unwrap().kind, NotifyKind::Done);
    }

    #[test]
    fn answers_that_did_not_reach_the_agent_wait_again_and_are_kept() {
        let dir = test_dir("review-reopen");
        let mut a = agent(&dir);
        let id = a.show_review(&review(), &mut Effects::default());
        a.answer_review(&id, &answers(), &mut Effects::default()).unwrap();
        a.reopen_review(&id, &mut Effects::default());
        let item = a.conv.get(&id).unwrap().clone();
        assert_eq!(item["statut"], "attente");
        assert_eq!(item["reponses"]["questions"]["q1"], "inline");
        assert_eq!(a.meta.reviews, [id.clone()]);
        // Answered again then, once.
        assert!(a.answer_review(&id, &answers(), &mut Effects::default()).is_ok());
    }

    #[test]
    fn a_waiting_review_survives_a_restart() {
        let dir = test_dir("review-restart");
        let mut a = agent(&dir);
        a.push_user("u1", "Relis", 0, &[], &mut Effects::default());
        let id = a.show_review(&review(), &mut Effects::default());
        // What is saved of the agent (mid-turn), read back as the app does when it starts again.
        let saved: AgentMeta =
            serde_json::from_value(serde_json::to_value(&a.meta).unwrap()).unwrap();
        drop(a);
        let mut back = AgentRt::new(saved, &dir);
        assert_eq!(back.meta.status, AgentStatus::Done);
        assert_eq!(back.meta.reviews, [id.clone()]);
        assert!(awaits(&back.meta));
        assert_eq!(back.conv.get(&id).unwrap()["statut"], "attente");
        assert!(back.answer_review(&id, &answers(), &mut Effects::default()).is_ok());
        // An agent saved before reviews has none.
        let old: AgentMeta = serde_json::from_value(json!({ "id": "a1", "name": "vieux" })).unwrap();
        assert!(old.reviews.is_empty());
    }
```

Ajouter aussi `use serde_json::Value;` au début de `mod tests` (à côté de `use serde_json::json;`).

- [ ] **Step 2 : lancer, constater l'échec**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test review::tests`
Expected : erreur de compilation `no field reviews on type AgentMeta` / `no method named show_review`.

- [ ] **Step 3 : le champ `reviews`**

Dans `src-tauri/src/model.rs`, juste après `pub progress_line: Option<String>,` (l.794) :

```rust
    /// The reviews it published (`show_review`) that wait for the user's answers, oldest first:
    /// the ids of their items in its conversation. Saved, so that it still waits after a restart.
    pub reviews: Vec<String>,
```

(`AgentMeta` a `#[serde(rename_all = "camelCase", default)]` : un état sauvegardé avant n'a pas le champ et le lit vide. Toutes les constructions de `AgentMeta` du code utilisent `..Default::default()`.)

- [ ] **Step 4 : `append` et `patch` visibles du crate**

Dans `src-tauri/src/agent.rs`, l.498 et l.502 :

```rust
    pub(crate) fn append(&mut self, item: Value, fx: &mut Effects) {
```

```rust
    pub(crate) fn patch(&mut self, id: &str, patch: Value, fx: &mut Effects) {
```

- [ ] **Step 5 : la notification de fin de tour**

Dans `src-tauri/src/agent.rs`, remplacer (l.1669-1674) :

```rust
        if !interrupted && !had_queue {
            fx.notify = Some(match &failure {
                Some(reason) => AgentAlert::new(NotifyKind::Error, notify::error_body(reason)),
                None => AgentAlert::new(NotifyKind::Done, notify::done_body(&reply)),
            });
        }
```

par :

```rust
        if !interrupted && !had_queue {
            fx.notify = Some(match &failure {
                Some(reason) => AgentAlert::new(NotifyKind::Error, notify::error_body(reason)),
                // A review it published waits: the user is called as for a question.
                None => match crate::review::waiting_body(self) {
                    Some(body) => AgentAlert::new(NotifyKind::Question, body),
                    None => AgentAlert::new(NotifyKind::Done, notify::done_body(&reply)),
                },
            });
        }
```

(La vue Claude Code passe par le même `on_result` : `agent/transcript.rs:247`.)

- [ ] **Step 6 : `impl AgentRt`, `awaits`, `waiting_body`**

Dans `src-tauri/src/review.rs`, compléter les `use` du haut :

```rust
use crate::agent::{AgentRt, Effects};
use crate::i18n::{self, Lang};
use crate::model::{new_id, now_ms, AgentMeta, AgentStatus};
use anyhow::{anyhow, bail, Result};
use serde_json::json;
```

(remplacer la ligne `use crate::i18n::Lang;` de la Task 1), puis ajouter avant `#[cfg(test)]` :

```rust
/// The agent waits for the user's answers to a review it published, its turn over: shown as
/// waiting (amber in the window, counted in the tray) though nothing runs. Not `Waiting`, which
/// is a turn under way for the rest of the app.
pub fn awaits(meta: &AgentMeta) -> bool {
    !meta.reviews.is_empty() && matches!(meta.status, AgentStatus::Done | AgentStatus::Idle)
}

/// What the notification at the end of a turn says while a review of the agent waits: its latest
/// one's title. None when none waits.
pub(crate) fn waiting_body(rt: &mut AgentRt) -> Option<String> {
    let id = rt.meta.reviews.last()?.clone();
    let titre: String = rt.conv.get(&id)?["contenu"]["titre"]
        .as_str()?
        .trim()
        .chars()
        .take(100)
        .collect();
    Some(tr!("Revue à remplir : {titre}", "Review to answer: {titre}"))
}

impl AgentRt {
    /// Appends `review` (checked) to the conversation, waiting for the user's answers: its id.
    pub fn show_review(&mut self, review: &Review, fx: &mut Effects) -> String {
        let id = new_id();
        let item = json!({
            "kind": "review", "id": id, "contenu": review, "statut": "attente",
            "reponses": null, "ts": now_ms(),
        });
        self.append(item, fx);
        self.meta.reviews.push(id.clone());
        fx.agent_changed = true;
        fx.save = true;
        id
    }

    /// Takes the user's `answers` to the review `id`: answered then, and the message that tells
    /// the agent. Under the agent's lock, so a second answer (a double click, two windows) is
    /// refused.
    pub fn answer_review(&mut self, id: &str, answers: &Answers, fx: &mut Effects) -> Result<String> {
        let item = self
            .conv
            .get(id)
            .filter(|v| v["kind"] == "review")
            .cloned()
            .ok_or_else(|| anyhow!(tr!("revue introuvable", "review not found")))?;
        if item["statut"] != "attente" {
            bail!(tr!(
                "Cette revue a déjà sa réponse.",
                "This review was already answered."
            ));
        }
        let review: Review = serde_json::from_value(item["contenu"].clone())?;
        self.patch(id, json!({ "statut": "repondue", "reponses": answers }), fx);
        self.meta.reviews.retain(|r| r != id);
        fx.agent_changed = true;
        fx.save = true;
        Ok(answer_text(i18n::claude(), &review, answers))
    }

    /// The answers to the review `id` did not reach the agent: it waits again, its answers kept
    /// for the page to show them.
    pub fn reopen_review(&mut self, id: &str, fx: &mut Effects) {
        self.patch(id, json!({ "statut": "attente" }), fx);
        if !self.meta.reviews.iter().any(|r| r == id) {
            self.meta.reviews.push(id.to_string());
        }
        fx.agent_changed = true;
        fx.save = true;
    }
}
```

- [ ] **Step 7 : la barre de menus compte une revue en attente**

Dans `src-tauri/src/core.rs`, `update_tray` (l.2285-2290), remplacer :

```rust
            .filter(|h| h.lock().meta.status == AgentStatus::Waiting)
```

par :

```rust
            .filter(|h| {
                let rt = h.lock();
                rt.meta.status == AgentStatus::Waiting || crate::review::awaits(&rt.meta)
            })
```

- [ ] **Step 8 : lancer les tests**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test review::tests && cargo test agent::tests::the_end_of_a_turn`
Expected : les 9 tests de `review::tests` PASS ; `the_end_of_a_turn_notifies_with_the_first_line_of_the_final_reply` PASS (sans revue, rien ne change).

- [ ] **Step 9 : commit**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
git add src-tauri/src/review.rs src-tauri/src/model.rs src-tauri/src/agent.rs src-tauri/src/core.rs
git commit -m "feat(revue): élément review de la conversation, attente et notification"
```

---

### Task 3 : `Core::show_review`, `Core::answer_review` et la commande `answer_review`

**Files:**
- Modify: `src-tauri/src/review.rs` (`impl Core`, tests d'intégration)
- Modify: `src-tauri/src/commands.rs:455` (après `answer_permission`)
- Modify: `src-tauri/src/lib.rs:309` (`generate_handler!`)

**Interfaces:**
- Consumes : `AgentRt::show_review/answer_review/reopen_review` (Task 2) ; `Core::with_agent` (`core.rs:3088`, `pub(crate)`) ; `Core::send_message` (`core.rs:2930`).
- Produces :
  - `impl<R: Runtime> Core<R> { pub fn show_review(self: &Arc<Self>, id: &str, review: &Review) -> Result<String>; pub async fn answer_review(self: &Arc<Self>, id: &str, review: &str, answers: Answers) -> Result<()> }`
  - commande Tauri `answer_review(agent_id: String, review_id: String, answers: Answers) -> Result<(), String>` (côté JS : `invoke('answer_review', { agentId, reviewId, answers })`).

Décision : `send_message` et non `send_by_app`. Le texte vient de l'utilisateur (ses réponses) : il doit se lire comme son message, sans en-tête « Message de … », et peut nommer un agent encore sans nom. `send_message` gère les deux vues (`core.rs:2973-2994` : tapé dans le terminal en vue Claude Code) et relance un processus arrêté (`ensure_process`).

- [ ] **Step 1 : écrire les tests qui échouent**

Ajouter dans `mod tests` de `src-tauri/src/review.rs` :

```rust
    use crate::accounts::PRINCIPAL;
    use crate::core_tests::{agent_on, harness};

    #[tokio::test]
    async fn the_answers_reach_the_agent_as_the_users_message_and_only_once() {
        let h = harness("review-answer");
        let (p, _) = h.project(false).await;
        let a = agent_on(&h, &p, PRINCIPAL).await;
        let id = h.core.show_review(&a.id, &checked(review()).unwrap()).unwrap();
        assert_eq!(h.agent(&a.id).reviews, [id.clone()]);
        // The window is told: the item, then the agent's view with its review.
        assert!(h.events.lock().iter().any(|e| e["type"] == "conv"
            && e["ops"][0]["item"]["kind"] == "review"));
        assert!(h.events.lock().iter().any(|e| e["type"] == "agent"
            && e["agent"]["reviews"][0] == id.as_str()));

        h.core.answer_review(&a.id, &id, answers()).await.unwrap();
        h.wait("the turn of the answers", |h| {
            h.items(&a.id).iter().any(|i| i["kind"] == "turn") && !h.agent(&a.id).status.is_active()
        })
        .await;
        let items = h.items(&a.id);
        let user = items.iter().find(|i| i["kind"] == "user").unwrap();
        let text = user["text"].as_str().unwrap();
        assert!(text.contains("Plan du parseur") && text.contains("Ici (inline)"), "{text}");
        let item = items.iter().find(|i| i["id"] == id.as_str()).unwrap();
        assert_eq!(item["statut"], "repondue");
        assert!(h.agent(&a.id).reviews.is_empty());

        let again = h.core.answer_review(&a.id, &id, answers()).await.unwrap_err();
        assert!(format!("{again:#}").contains("déjà"), "{again:#}");
        // No second message.
        assert_eq!(h.items(&a.id).iter().filter(|i| i["kind"] == "user").count(), 1);
    }

    #[tokio::test]
    async fn answers_that_cannot_reach_the_agent_leave_the_review_waiting_with_them() {
        let h = harness("review-answer-fails");
        let (p, _) = h.project(false).await;
        let a = agent_on(&h, &p, PRINCIPAL).await;
        let id = h.core.show_review(&a.id, &checked(review()).unwrap()).unwrap();
        // In the Claude Code view with no terminal open: nothing to type the answers into.
        h.core.agent(&a.id).unwrap().lock().meta.terminal = true;
        assert!(h.core.answer_review(&a.id, &id, answers()).await.is_err());
        let item = h
            .items(&a.id)
            .into_iter()
            .find(|i| i["id"] == id.as_str())
            .unwrap();
        assert_eq!(item["statut"], "attente");
        assert_eq!(item["reponses"]["questions"]["q1"], "inline");
        assert_eq!(h.agent(&a.id).reviews, [id]);
    }
```

- [ ] **Step 2 : lancer, constater l'échec**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test review::tests`
Expected : erreur de compilation `no method named show_review found for struct Arc<Core<MockRuntime>>`.

- [ ] **Step 3 : `impl Core`**

Dans `src-tauri/src/review.rs`, ajouter aux `use` :

```rust
use crate::core::Core;
use std::sync::Arc;
use tauri::Runtime;
```

puis, après `impl AgentRt { … }` :

```rust
impl<R: Runtime> Core<R> {
    /// Shows `review` (checked) in the conversation of the agent `id`: its item's id.
    pub fn show_review(self: &Arc<Self>, id: &str, review: &Review) -> Result<String> {
        self.with_agent(id, |rt, fx| Ok(rt.show_review(review, fx)))
    }

    /// The user's answers to the review `review` of the agent `id`: answered, then sent to the
    /// agent as the user's message (`send_message`: a turn in the Escouade view, typed in its
    /// terminal in the Claude Code view, its process started again if it was stopped). Not
    /// delivered, the review waits again with its answers.
    pub async fn answer_review(self: &Arc<Self>, id: &str, review: &str, answers: Answers) -> Result<()> {
        let text = self.with_agent(id, |rt, fx| rt.answer_review(review, &answers, fx))?;
        if let Err(e) = self.send_message(id, text, Vec::new()).await {
            let _ = self.with_agent(id, |rt, fx| {
                rt.reopen_review(review, fx);
                Ok(())
            });
            return Err(e);
        }
        Ok(())
    }
}
```

- [ ] **Step 4 : la commande Tauri**

Dans `src-tauri/src/commands.rs`, après la fonction `answer_permission` (fin l.455) :

```rust
/// « Envoyer » of a review's page: the user's answers, sent to the agent that published it.
#[tauri::command]
pub async fn answer_review(
    core: CoreState<'_>,
    agent_id: String,
    review_id: String,
    answers: crate::review::Answers,
) -> Res<()> {
    core.answer_review(&agent_id, &review_id, answers)
        .await
        .map_err(err)
}
```

Dans `src-tauri/src/lib.rs`, après `commands::answer_permission,` (l.309) :

```rust
            commands::answer_review,
```

- [ ] **Step 5 : lancer les tests**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test review::tests`
Expected : 11 tests PASS.

- [ ] **Step 6 : commit**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
git add src-tauri/src/review.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(revue): commande answer_review, réponse unique et retour en attente si l'envoi échoue"
```

---

### Task 4 : l'outil MCP `show_review`

**Files:**
- Modify: `src-tauri/src/mcp/tools.rs` (après `report_progress`, l.1239)
- Modify: `src-tauri/src/mcp/act.rs` (après `report_progress`, l.1724 ; `use` l.53-67)
- Modify: `src-tauri/src/mcp/tests.rs:24-52` (`EXPOSED`), l.95 (`AGENTS_ONLY`)
- Test: `src-tauri/src/mcp/act_tests.rs` (l.6-11 imports, l.154 `required`, l.1251-1275 refus externe, nouveau test)

**Interfaces:**
- Consumes : `review::{Review, checked}` (Task 1), `Core::show_review` (Task 3), `act::agent_only` (`act.rs:87`), `ToolError` (`tools.rs:73-79`), `summary` (`tools.rs:163`).
- Produces : outil `show_review` (arguments = le JSON de revue, réponse texte « Page affichée… ») ; `pub(super) fn act::show_review<R: Runtime>(core: &Arc<Core<R>>, caller: &Caller, review: Review) -> Result<String, ToolError>`.

- [ ] **Step 1 : écrire les tests qui échouent**

Dans `src-tauri/src/mcp/tests.rs`, ajouter `"show_review",` dans `EXPOSED` entre `"set_board",` et `"split_ticket",`, et remplacer l.95 par :

```rust
pub(super) const AGENTS_ONLY: &[&str] = &["report_progress", "show_review", "split_ticket"];
```

Dans `src-tauri/src/mcp/act_tests.rs` :
- l.8-11, ajouter `call` à la liste importée de `super::tools_tests` ;
- après `assert_eq!(required("report_progress"), ["line"]);` (l.154) :

```rust
    assert_eq!(required("show_review"), ["questions", "titre"]);
```

- dans `the_tools_for_agents_are_refused_to_claude_outside_escouade_and_logged` (l.1257-1260), ajouter au tableau :

```rust
        ("show_review", json!({ "titre": "Plan", "questions": [{ "id": "q1", "titre": "Ok ?" }] })),
```

  et après `assert_eq!(h.agent(&a.id).progress_line, None);` :

```rust
    assert!(h.items(&a.id).iter().all(|i| i["kind"] != "review"));
```

- nouveau test, après `an_agent_reports_what_it_does_on_one_line_and_a_new_report_replaces_it` :

```rust
#[tokio::test]
async fn an_agent_shows_a_review_in_its_own_conversation_and_is_told_to_end_its_turn() {
    let h = harness("mcp-act-show-review");
    let (p, _) = h.project(false).await;
    let a = put_agent(&h, &p, "relecteur", 1);
    let c = as_agent(&h, &a.id).await;
    let review = json!({
        "titre": "Plan du parseur", "chapo": "Trois lots.",
        "puces": [{ "texte": "Migration risquée", "ton": "alerte" }],
        "questions": [
            { "id": "q1", "titre": "Exécution ?", "choix": [
                { "valeur": "sub", "libelle": "Sous-agents", "reco": true },
                { "valeur": "inline", "libelle": "Ici" }
            ] },
            { "id": "q2", "titre": "Nom du module ?" }
        ]
    });
    let (text, error) = call(&c, "show_review", review).await;
    assert!(!error, "{text}");
    assert_eq!(
        text,
        "Page affichée. Termine ton tour : les réponses arriveront dans un message."
    );
    let items = h.items(&a.id);
    let shown: Vec<_> = items.iter().filter(|i| i["kind"] == "review").collect();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0]["statut"], "attente");
    assert_eq!(shown[0]["contenu"]["questions"][0]["choix"][0]["reco"], true);
    assert_eq!(h.agent(&a.id).reviews, [shown[0]["id"].as_str().unwrap()]);
    let logged = entry(&h);
    assert_eq!(
        (logged.caller.as_str(), logged.tool.as_str(), logged.outcome, logged.summary.as_str()),
        ("relecteur", "show_review", "ok", "titre: Plan du parseur")
    );

    // A review that cannot be shown: the agent reads why, nothing is shown.
    let why = refused(&c, "show_review", json!({ "titre": "Vide", "questions": [] })).await;
    assert!(why.contains("1 à 20"), "{why}");
    let why = refused(
        &c,
        "show_review",
        json!({ "titre": "Double", "questions": [{ "id": "q1", "titre": "A" }, { "id": "q1", "titre": "B" }] }),
    )
    .await;
    assert!(why.contains("« q1 »"), "{why}");
    // Not even JSON of the format: refused by the server before the tool.
    let why = refused(&c, "show_review", json!({ "titre": 3 })).await;
    assert!(!why.is_empty());
    assert_eq!(h.items(&a.id).iter().filter(|i| i["kind"] == "review").count(), 1);
    c.cancel().await.unwrap();
    h.core.mcp.stop();
}
```

- [ ] **Step 2 : lancer, constater l'échec**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test mcp::act_tests::an_agent_shows_a_review mcp::act_tests::the_tools_for_agents_are_refused mcp::act_tests::the_acting_tools_say`

(`cargo test` ne prend qu'un filtre : lancer les trois commandes séparément.)
Expected : FAIL, l'outil `show_review` est inconnu du serveur (`unwrap()` sur `None` dans `the_acting_tools_say…`, `Call refused by the server` ailleurs).

- [ ] **Step 3 : l'action**

Dans `src-tauri/src/mcp/act.rs`, ajouter aux `use` (après `use crate::model::{…};`, l.61) :

```rust
use crate::review::{self, Review};
```

et après `report_progress` (l.1724) :

```rust
/// `show_review`: the calling agent's review, checked, shown in its own conversation; the agent
/// is told to end its turn, the answers coming as a message.
pub(super) fn show_review<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    asked: Review,
) -> Result<String, ToolError> {
    let id = agent_only(caller)?;
    let review = review::checked(asked).map_err(ToolError::Failed)?;
    core.show_review(id, &review).map_err(failure)?;
    Ok(tr!(
        "Page affichée. Termine ton tour : les réponses arriveront dans un message.",
        "Page shown. End your turn: the answers will come in a message."
    ))
}
```

Compléter la dernière phrase du doc du module (l.38-39) : « `report_progress`, `show_review` and `split_ticket` are for the agents of Escouade alone. »

- [ ] **Step 4 : l'outil**

Dans `src-tauri/src/mcp/tools.rs`, après la méthode `report_progress` (l.1239) :

```rust
    #[tool(
        description = "Only for Escouade's own agents (refused to any other client). Show the user a review page in the side panel of your own conversation, rather than asking in text: a title (titre), optional summary (chapo) and key points (puces, ton \"alerte\" in red), sections, then 1 to 20 questions with ids of their own, each a choice (choix; reco true marks the recommended one, preselected) or a free text; a field « Autre chose ? » is always added. 50 KB at most. Answers at once: end your turn then, the user's answers come later as a message (Réponses à « <titre> » : …).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn show_review(
        &self,
        ctx: ToolCtx<R>,
        Parameters(review): Parameters<crate::review::Review>,
    ) -> CallToolResult {
        let asked = summary(&[("titre", Some(review.titre.trim()))]);
        let result = act::show_review(&ctx.core, &ctx.caller, review);
        ctx.reply("show_review", &asked, result)
    }
```

- [ ] **Step 5 : lancer les tests**

Run (séparément) :
- `cargo test mcp::act_tests::an_agent_shows_a_review`
- `cargo test mcp::act_tests::the_tools_for_agents_are_refused`
- `cargo test mcp::act_tests::the_acting_tools_say`
- `cargo test mcp::tests::the_server_offers_exactly_its_tools`

Expected : PASS pour les quatre. (`claude_outside_escouade_initializes…` échoue déjà avant ce plan à cause du nom `osho-squad` : attendu.)

- [ ] **Step 6 : commit**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
git add src-tauri/src/mcp/tools.rs src-tauri/src/mcp/act.rs src-tauri/src/mcp/tests.rs src-tauri/src/mcp/act_tests.rs
git commit -m "feat(mcp): outil show_review réservé aux agents"
```

---

### Task 5 : types TS, `ipc`, fonctions de `lib/review.ts`, libellé de l'outil

**Files:**
- Modify: `src/lib/types.ts:553` (`Agent.reviews`), l.990-1000 (`ReviewItem`, union `ConvItem`)
- Modify: `src/lib/ipc.ts:142` (`answerReview`)
- Modify: `src/test/ipc.ts:137` (`agent()` : `reviews: []`), l.284 (`resetApp`), nouvelle fixture `reviewItem`
- Modify: `src/lib/tools.ts:38`, `src/lib/i18n/fr/mcp.ts:61`, `src/lib/i18n/en/mcp.ts` (même place)
- Create: `src/lib/review.ts`, `src/lib/review.test.ts`
- Modify: `src/lib/state.svelte.ts:221` (`reviewShown`, type de `panelTab`) — seulement les deux champs ici, le reste en Task 7

**Interfaces:**
- Consumes : JSON produit par les Tasks 2-3 (`kind: 'review'`, `contenu`, `statut`, `reponses`), commande `answer_review { agentId, reviewId, answers }`.
- Produces :
  - `interface ReviewItem`, `ReviewContent`, `ReviewQuestion`, `ReviewChoice`, `ReviewBullet`, `ReviewSection`, `ReviewAnswers` dans `types.ts` ; `Agent.reviews: string[]`.
  - `api.answerReview(agentId: string, reviewId: string, answers: ReviewAnswers): Promise<void>`
  - `shownStatus(a: Agent): AgentStatus`, `shownReview(items: ConvItem[], chosen: string | undefined): ReviewItem | null`, `initialAnswers(item: ReviewItem): ReviewAnswers` dans `src/lib/review.ts`.
  - `app.reviewShown: Record<string, string>` (revue choisie par agent), `app.panelTab: 'files' | 'history' | 'review'`.
  - fixture `reviewItem(over?: Partial<ReviewItem>): ReviewItem` dans `src/test/ipc.ts`.

- [ ] **Step 1 : les types**

Dans `src/lib/types.ts`, après `progressLine: string | null;` (l.553) :

```ts
  /** The reviews it published (`show_review`) that wait for the user's answers, oldest first: ids of their items. */
  reviews: string[];
```

Avant `export type ConvItem` (l.1000) :

```ts
export interface ReviewBullet {
  texte: string;
  /** 'alerte' shows it in red; anything else was dropped by the backend. */
  ton: 'alerte' | 'neutre' | null;
}
export interface ReviewSection {
  titre: string;
  ton: 'alerte' | 'neutre' | null;
  items: string[];
  paragraphes: string[];
}
export interface ReviewChoice {
  valeur: string;
  libelle: string;
  detail: string | null;
  /** The recommended choice: marked, and checked before the user picks. */
  reco: boolean;
}
export interface ReviewQuestion {
  id: string;
  titre: string;
  contexte: string | null;
  /** Empty: a free text. */
  choix: ReviewChoice[];
  placeholder: string | null;
}
/** A review as the agent wrote it (the format of the skill `revue-artefact`). */
export interface ReviewContent {
  titre: string;
  sur: string | null;
  chapo: string | null;
  puces: ReviewBullet[];
  sections: ReviewSection[];
  titreQuestions: string | null;
  questions: ReviewQuestion[];
}
/** The user's answers: by question id, a choice's `valeur` or a text; and « Autre chose ? ». */
export interface ReviewAnswers {
  questions: Record<string, string>;
  note: string;
}
/** A review an agent published (`show_review`): its page in the side panel, waiting until answered. */
export interface ReviewItem extends Base {
  kind: 'review';
  contenu: ReviewContent;
  statut: 'attente' | 'repondue';
  /** Kept on a review that waits again too (its answers could not be sent). */
  reponses: ReviewAnswers | null;
  ts: number;
}
```

et ajouter `| ReviewItem` à l'union :

```ts
export type ConvItem = UserItem | TextItem | ThinkingItem | ToolItem | QuestionItem | PermissionItem | TurnItem | NoticeItem | EventItem | ReviewItem;
```

- [ ] **Step 2 : `ipc` et l'état**

Dans `src/lib/ipc.ts`, après `answerPermission` (l.141-142), et ajouter `ReviewAnswers` à l'import de `./types` :

```ts
  /** « Envoyer » of a review's page: the answers go to the agent as the user's message (refused once answered). */
  answerReview: (agentId: string, reviewId: string, answers: ReviewAnswers) =>
    invoke<void>('answer_review', { agentId, reviewId, answers }),
```

Dans `src/lib/state.svelte.ts`, remplacer l.220-221 :

```ts
  /** Tab of the side panel: uncommitted files, the repository history, or an agent's review. */
  panelTab = $state<'files' | 'history' | 'review'>('files');
  /** The review the side panel shows for an agent (its card clicked, or the one it just published), by agent. */
  reviewShown = $state<Record<string, string>>({});
```

Dans `src/test/ipc.ts` : dans `agent()`, après `progressLine: null,` ajouter `reviews: [],` ; dans `resetApp`, après `app.panelTab = 'files';` ajouter `app.reviewShown = {};` ; ajouter `ReviewItem` à l'import de types et, après `agent()` :

```ts
export function reviewItem(over: Partial<ReviewItem> = {}): ReviewItem {
  return {
    kind: 'review',
    id: 'rv1',
    ts: 1,
    statut: 'attente',
    reponses: null,
    contenu: {
      titre: 'Plan du parseur',
      sur: 'Plan',
      chapo: 'Trois lots.',
      puces: [
        { texte: 'Migration risquée', ton: 'alerte' },
        { texte: 'Tests verts', ton: null },
      ],
      sections: [{ titre: 'Lots', ton: null, items: ['Lot 1'], paragraphes: ['Le lot 1 crée le lexer.'] }],
      titreQuestions: null,
      questions: [
        {
          id: 'q1',
          titre: 'Exécution ?',
          contexte: 'Deux façons de faire.',
          placeholder: null,
          choix: [
            { valeur: 'sub', libelle: 'Sous-agents', detail: null, reco: true },
            { valeur: 'inline', libelle: 'Ici', detail: 'Plus lent', reco: false },
          ],
        },
        { id: 'q2', titre: 'Nom du module ?', contexte: null, placeholder: 'parser', choix: [] },
      ],
    },
    ...over,
  };
}
```

- [ ] **Step 3 : écrire les tests qui échouent**

Créer `src/lib/review.test.ts` :

```ts
import { describe, expect, it } from 'vitest';
import { agent, reviewItem } from '../test/ipc';
import { initialAnswers, shownReview, shownStatus } from './review';
import type { ConvItem } from './types';

describe('reviews', () => {
  it('shows an agent whose review waits as waiting, once its turn is over', () => {
    expect(shownStatus(agent({ status: 'done', reviews: ['rv1'] }))).toBe('waiting');
    expect(shownStatus(agent({ status: 'idle', reviews: ['rv1'] }))).toBe('waiting');
    // At work or failed, it says so.
    expect(shownStatus(agent({ status: 'running', reviews: ['rv1'] }))).toBe('running');
    expect(shownStatus(agent({ status: 'error', reviews: ['rv1'] }))).toBe('error');
    expect(shownStatus(agent({ status: 'done' }))).toBe('done');
  });

  it('picks the review chosen, else the latest that waits', () => {
    const items: ConvItem[] = [
      reviewItem({ id: 'rv1' }),
      { kind: 'notice', id: 'n1', ts: 2, level: 'info', text: 'x' },
      reviewItem({ id: 'rv2' }),
      reviewItem({ id: 'rv3', statut: 'repondue' }),
    ];
    expect(shownReview(items, undefined)?.id).toBe('rv2');
    expect(shownReview(items, 'rv3')?.id).toBe('rv3');
    // A choice that is no review of this conversation (another agent's) is not shown.
    expect(shownReview(items, 'zzz')?.id).toBe('rv2');
    expect(shownReview([reviewItem({ statut: 'repondue' })], undefined)).toBeNull();
  });

  it('starts from the recommendation, else from the answers kept', () => {
    expect(initialAnswers(reviewItem())).toEqual({ questions: { q1: 'sub' }, note: '' });
    const kept = { questions: { q1: 'inline', q2: 'lexer' }, note: 'Merci' };
    expect(initialAnswers(reviewItem({ reponses: kept }))).toEqual(kept);
  });
});
```

- [ ] **Step 4 : lancer, constater l'échec**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run src/lib/review.test.ts`
Expected : FAIL, `Failed to resolve import "./review"`.

- [ ] **Step 5 : `src/lib/review.ts`**

```ts
// The reviews agents publish (`show_review`): what the window shows of them.

import type { Agent, AgentStatus, ConvItem, ReviewAnswers, ReviewItem } from './types';

/** The status an agent shows: « waiting » while a review it published waits for the user's answers, its turn over. */
export function shownStatus(a: Agent): AgentStatus {
  return a.reviews.length && (a.status === 'done' || a.status === 'idle') ? 'waiting' : a.status;
}

/** The review the side panel shows: the one chosen (its card clicked), else the latest that waits. */
export function shownReview(items: ConvItem[], chosen: string | undefined): ReviewItem | null {
  const reviews = items.filter((i): i is ReviewItem => i.kind === 'review');
  return reviews.find((r) => r.id === chosen) ?? [...reviews].reverse().find((r) => r.statut === 'attente') ?? null;
}

/** The answers a page starts from: those kept (a send that failed, or its answers once answered), else each recommended choice. */
export function initialAnswers(item: ReviewItem): ReviewAnswers {
  if (item.reponses) return { questions: { ...item.reponses.questions }, note: item.reponses.note };
  const questions: Record<string, string> = {};
  for (const q of item.contenu.questions) {
    const reco = q.choix.find((c) => c.reco);
    if (reco) questions[q.id] = reco.valeur;
  }
  return { questions, note: '' };
}
```

- [ ] **Step 6 : le libellé de l'outil dans la conversation**

`src/lib/tools.ts`, après `split_ticket: 'mcp.tools.splitTicket',` (l.38) :

```ts
  show_review: 'mcp.tools.showReview',
```

`src/lib/i18n/fr/mcp.ts`, après `splitTicket: 'Découper son ticket',` : `showReview: 'Afficher une revue',`
`src/lib/i18n/en/mcp.ts`, après `splitTicket: …,` : `showReview: 'Show a review',`

- [ ] **Step 7 : lancer les tests et le typage**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run src/lib/review.test.ts src/lib/tools.test.ts && npm run check`
Expected : PASS ; `svelte-check` 0 erreur.

- [ ] **Step 8 : commit**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
git add src/lib/types.ts src/lib/ipc.ts src/lib/review.ts src/lib/review.test.ts src/lib/state.svelte.ts src/test/ipc.ts src/lib/tools.ts src/lib/i18n/fr/mcp.ts src/lib/i18n/en/mcp.ts
git commit -m "feat(revue): types, ipc et choix de la revue affichée"
```

---

### Task 6 : la page `ReviewPage.svelte`

**Files:**
- Create: `src/components/ReviewPage.svelte`, `src/components/ReviewPage.test.ts`
- Modify: `src/lib/i18n/fr/conv.ts`, `src/lib/i18n/en/conv.ts` (bloc `review`, avant la fermeture)

**Interfaces:**
- Consumes : `ReviewItem`, `initialAnswers` (Task 5), `api.answerReview` (Task 5), `app.run` (`state.svelte.ts:1047`, toast d'erreur).
- Produces : composant `ReviewPage` aux props `{ item: ReviewItem; agentId: string }`, à monter dans un `{#key item.id}` (l'état du formulaire est pris au montage). Clés `conv.review.{card, waiting, answered, pill, none, questions, recommended, note, send}`.

- [ ] **Step 1 : les textes**

`src/lib/i18n/fr/conv.ts`, avant `} as const satisfies Tree;` :

```ts
  /** A review an agent published (`show_review`): its card in the conversation, its page in the side panel. `{title}`: its title. */
  review: {
    card: 'Revue : {title}',
    waiting: 'en attente',
    answered: 'répondue',
    pill: 'Revue',
    none: 'Aucune revue à afficher pour cet agent.',
    questions: 'Questions',
    recommended: 'recommandé',
    note: 'Autre chose ?',
    send: 'Envoyer',
  },
```

`src/lib/i18n/en/conv.ts`, avant `});` :

```ts
  review: {
    card: 'Review: {title}',
    waiting: 'waiting',
    answered: 'answered',
    pill: 'Review',
    none: 'No review to show for this agent.',
    questions: 'Questions',
    recommended: 'recommended',
    note: 'Anything else?',
    send: 'Send',
  },
```

- [ ] **Step 2 : écrire les tests qui échouent**

Créer `src/components/ReviewPage.test.ts` :

```ts
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { setLang } from '../lib/i18n';
import { app } from '../lib/state.svelte';
import { fakeBackend, resetApp, reviewItem } from '../test/ipc';
import ReviewPage from './ReviewPage.svelte';

describe('ReviewPage', () => {
  beforeEach(() => resetApp());

  it('shows the page and preselects the recommended choice', () => {
    fakeBackend();
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    expect(screen.getByRole('heading', { name: 'Plan du parseur' })).toBeInTheDocument();
    expect(screen.getByText('Trois lots.')).toBeInTheDocument();
    expect(screen.getByText('Migration risquée')).toHaveClass('alerte');
    expect(screen.getByText('Tests verts')).not.toHaveClass('alerte');
    expect(screen.getByText('Le lot 1 crée le lexer.')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Sous-agents/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /Ici/ })).not.toBeChecked();
    expect(screen.getByText('recommandé')).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Nom du module ?' })).toHaveAttribute('placeholder', 'parser');
    expect(screen.getByText('en attente')).toBeInTheDocument();
  });

  it('sends the choices, the texts and the note', async () => {
    const backend = fakeBackend();
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    await userEvent.click(screen.getByRole('radio', { name: /Ici/ }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Nom du module ?' }), 'lexer');
    await userEvent.type(screen.getByRole('textbox', { name: 'Autre chose ?' }), 'Merci');
    await userEvent.click(screen.getByRole('button', { name: 'Envoyer' }));
    expect(backend.called('answer_review')).toEqual([
      {
        cmd: 'answer_review',
        args: { agentId: 'a1', reviewId: 'rv1', answers: { questions: { q1: 'inline', q2: 'lexer' }, note: 'Merci' } },
      },
    ]);
  });

  it('is read-only once answered, with its answers', () => {
    fakeBackend();
    const reponses = { questions: { q1: 'inline', q2: 'lexer' }, note: 'Merci' };
    render(ReviewPage, { item: reviewItem({ statut: 'repondue', reponses }), agentId: 'a1' });
    expect(screen.getByRole('radio', { name: /Ici/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /Ici/ })).toBeDisabled();
    expect(screen.getByRole('textbox', { name: 'Nom du module ?' })).toHaveValue('lexer');
    expect(screen.getByRole('textbox', { name: 'Autre chose ?' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Envoyer' })).not.toBeInTheDocument();
    expect(screen.getByText('répondue')).toBeInTheDocument();
  });

  it('tells a send that failed and can send again', async () => {
    fakeBackend({
      answer_review: () => {
        throw 'L’agent est en vue Claude Code : ouvre-le pour démarrer son terminal.';
      },
    });
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    const send = screen.getByRole('button', { name: 'Envoyer' });
    await userEvent.click(send);
    await waitFor(() => expect(app.toasts.at(-1)).toMatchObject({ kind: 'error', text: expect.stringMatching(/vue Claude Code/) }));
    expect(send).toBeEnabled();
  });

  it('survives two choices of the same value', () => {
    fakeBackend();
    const item = reviewItem();
    item.contenu.questions[0].choix[1].valeur = 'sub';
    render(ReviewPage, { item, agentId: 'a1' });
    expect(screen.getAllByRole('radio')).toHaveLength(2);
  });

  it('speaks English', () => {
    setLang('en');
    fakeBackend();
    render(ReviewPage, { item: reviewItem(), agentId: 'a1' });
    expect(screen.getByRole('button', { name: 'Send' })).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Anything else?' })).toBeInTheDocument();
    expect(screen.getByText('recommended')).toBeInTheDocument();
  });
});
```

- [ ] **Step 3 : lancer, constater l'échec**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run src/components/ReviewPage.test.ts`
Expected : FAIL, `Failed to resolve import "./ReviewPage.svelte"`.

- [ ] **Step 4 : le composant**

Créer `src/components/ReviewPage.svelte` :

```svelte
<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../lib/i18n';
  import { api } from '../lib/ipc';
  import { initialAnswers } from '../lib/review';
  import { app } from '../lib/state.svelte';
  import type { ReviewItem } from '../lib/types';

  // A review an agent published (`show_review`), answered here once and read-only after. Mounted under `{#key item.id}`:
  // the answers start from its item when it mounts. Its texts are shown as written, never as Markdown or HTML.
  let { item, agentId }: { item: ReviewItem; agentId: string } = $props();

  const start = untrack(() => initialAnswers(item));
  let choices = $state<Record<string, string>>(start.questions);
  let note = $state(start.note);
  let sending = $state(false);
  const c = $derived(item.contenu);
  const answered = $derived(item.statut === 'repondue');

  async function send() {
    sending = true;
    await app.run(api.answerReview(agentId, item.id, { questions: $state.snapshot(choices), note }));
    sending = false;
  }
</script>

<div class="review">
  <header>
    {#if c.sur}<span class="sur">{c.sur}</span>{/if}
    <h2>{c.titre}</h2>
    <span class="state" class:done={answered}>{answered ? t('conv.review.answered') : t('conv.review.waiting')}</span>
  </header>
  {#if c.chapo}<p class="chapo">{c.chapo}</p>{/if}
  {#if c.puces.length}
    <ul class="puces">
      {#each c.puces as p, i (i)}<li class:alerte={p.ton === 'alerte'}>{p.texte}</li>{/each}
    </ul>
  {/if}
  {#each c.sections as s, i (i)}
    <section class:alerte={s.ton === 'alerte'}>
      <h3>{s.titre}</h3>
      {#each s.paragraphes as para, j (j)}<p>{para}</p>{/each}
      {#if s.items.length}
        <ul>{#each s.items as it, j (j)}<li>{it}</li>{/each}</ul>
      {/if}
    </section>
  {/each}
  <h3 class="qhead">{c.titreQuestions ?? t('conv.review.questions')}</h3>
  {#each c.questions as q (q.id)}
    <fieldset class="q" disabled={answered || sending}>
      <legend>{q.titre}</legend>
      {#if q.contexte}<p class="ctx">{q.contexte}</p>{/if}
      {#if q.choix.length}
        {#each q.choix as ch, ci (ci)}
          <label class="opt">
            <input type="radio" name="{item.id}-{q.id}" value={ch.valeur} bind:group={choices[q.id]} />
            <span class="lib">{ch.libelle}</span>
            {#if ch.reco}<span class="reco">{t('conv.review.recommended')}</span>{/if}
            {#if ch.detail}<span class="detail">{ch.detail}</span>{/if}
          </label>
        {/each}
      {:else}
        <textarea rows="3" aria-label={q.titre} placeholder={q.placeholder ?? ''} bind:value={choices[q.id]}></textarea>
      {/if}
    </fieldset>
  {/each}
  <label class="note">
    <span>{t('conv.review.note')}</span>
    <textarea rows="3" bind:value={note} disabled={answered || sending}></textarea>
  </label>
  {#if !answered}
    <button class="btn primary send" disabled={sending} onclick={send}>{t('conv.review.send')}</button>
  {/if}
</div>

<style>
  .review {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 4px 18px 18px;
    color: var(--text);
    font-size: 13px;
    line-height: 1.55;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-left: 3px solid var(--accent);
    padding-left: 10px;
  }
  .sur {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  .state {
    align-self: flex-start;
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 99px;
    background: var(--wait-soft);
    color: var(--wait);
  }
  .state.done {
    background: var(--accent-soft);
    color: var(--text);
  }
  .chapo,
  p,
  li,
  .detail,
  .ctx {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .chapo {
    margin: 0;
    font-size: 14px;
  }
  ul {
    margin: 0;
    padding-left: 18px;
  }
  .alerte {
    color: var(--del);
  }
  section {
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }
  section.alerte {
    border-top-color: var(--del);
  }
  h3 {
    margin: 0 0 4px;
    font-size: 13px;
  }
  .q {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--r);
    background: var(--elev);
  }
  legend {
    font-weight: 600;
    padding: 0 4px;
  }
  .ctx {
    margin: 0;
    color: var(--muted);
  }
  .opt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .opt input {
    accent-color: var(--accent);
  }
  .reco {
    font-size: 10.5px;
    padding: 1px 7px;
    border-radius: 99px;
    background: var(--accent-soft);
    color: var(--text);
  }
  .detail {
    flex-basis: 100%;
    padding-left: 22px;
    color: var(--dim);
  }
  textarea {
    width: 100%;
    resize: vertical;
    font: inherit;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    padding: 6px 8px;
  }
  .note {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-weight: 600;
  }
  .send {
    align-self: flex-end;
  }
</style>
```

- [ ] **Step 5 : lancer les tests et le typage**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run src/components/ReviewPage.test.ts src/lib/i18n && npm run check`
Expected : PASS (les tests i18n `catalogs.test.ts` / `hardcoded.test.ts` aussi) ; `svelte-check` 0 erreur.

- [ ] **Step 6 : commit**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
git add src/components/ReviewPage.svelte src/components/ReviewPage.test.ts src/lib/i18n/fr/conv.ts src/lib/i18n/en/conv.ts
git commit -m "feat(revue): page de revue à cocher"
```

---

### Task 7 : carte dans la conversation, onglet « Revue » et ouverture automatique

**Files:**
- Create: `src/components/conv/ReviewCard.svelte`, `src/components/conv/ReviewCard.test.ts`
- Modify: `src/components/Conversation.svelte:31` (import), l.689-690 (branche `review`)
- Modify: `src/components/SidePanel.svelte` (onglet, contenu), `src/components/SidePanel.test.ts`
- Modify: `src/lib/state.svelte.ts` (`openReview` après `toggleLayout` l.706-709 ; `noteReview` appelé dans `onEvent` `case 'agent'` l.476-484)
- Modify: `src/lib/state.test.ts`
- Modify: `src/lib/i18n/fr/nav.ts:94`, `src/lib/i18n/en/nav.ts:85`

**Interfaces:**
- Consumes : `ReviewItem`, `shownReview` (Task 5), `ReviewPage` (Task 6), `conversationOf` (`conversations.svelte.ts:158`), `app.toggleLayout`, `app.split`.
- Produces : `app.openReview(agentId: string, reviewId: string): void` (choisit la revue, ouvre l'onglet `review`, passe en moitié / moitié) ; composant `ReviewCard` aux props `{ item: ReviewItem; agentId: string }`.

- [ ] **Step 1 : écrire les tests qui échouent**

Créer `src/components/conv/ReviewCard.test.ts` :

```ts
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { app } from '../../lib/state.svelte';
import { fakeBackend, resetApp, reviewItem } from '../../test/ipc';
import ReviewCard from './ReviewCard.svelte';

describe('ReviewCard', () => {
  beforeEach(() => resetApp());

  it('says whether the review waits and opens its page in the right half', async () => {
    fakeBackend();
    render(ReviewCard, { item: reviewItem(), agentId: 'a1' });
    const card = screen.getByRole('button', { name: /Revue : Plan du parseur/ });
    expect(card).toHaveTextContent('en attente');
    await userEvent.click(card);
    expect(app.panelTab).toBe('review');
    expect(app.reviewShown).toEqual({ a1: 'rv1' });
    expect(app.split).toBe(true);
  });

  it('says once it is answered', () => {
    fakeBackend();
    render(ReviewCard, { item: reviewItem({ statut: 'repondue' }), agentId: 'a1' });
    expect(screen.getByRole('button', { name: /Revue : Plan du parseur/ })).toHaveTextContent('répondue');
  });
});
```

Ajouter dans `src/components/SidePanel.test.ts`, dans `describe('SidePanel')` (et `reviewItem` à l'import de `../test/ipc`) :

```ts
  it('shows the latest review that waits, else the one its card chose', async () => {
    const a9 = agent({ id: 'a9', reviews: ['rv2'] });
    app.agents.a9 = a9;
    fakeBackend({
      git_files: () => [],
      get_conversation: () => [
        reviewItem({ id: 'rv1', statut: 'repondue', contenu: { ...reviewItem().contenu, titre: 'Spec' } }),
        reviewItem({ id: 'rv2', contenu: { ...reviewItem().contenu, titre: 'Plan' } }),
      ],
    });
    app.panelTab = 'review';
    render(SidePanel, { project: project(), agent: app.agents.a9, docked: true });
    expect(screen.getByRole('tab', { name: /Revue\s*1/ })).toHaveAttribute('aria-selected', 'true');
    expect(await screen.findByRole('heading', { name: 'Plan' })).toBeInTheDocument();
    app.reviewShown = { a9: 'rv1' };
    expect(await screen.findByRole('heading', { name: 'Spec' })).toBeInTheDocument();
  });

  it('says when the agent has no review', async () => {
    fakeBackend({ git_files: () => [], get_conversation: () => [] });
    app.agents.a8 = agent({ id: 'a8' });
    app.panelTab = 'review';
    render(SidePanel, { project: project(), agent: app.agents.a8, docked: true });
    expect(await screen.findByText('Aucune revue à afficher pour cet agent.')).toBeInTheDocument();
  });
```

(Ids `a8` / `a9` : les conversations sont gardées par agent dans `conversations.svelte.ts` d'un test à l'autre.)

Ajouter à la fin de `src/lib/state.test.ts` :

```ts
describe('reviews', () => {
  beforeEach(() => resetApp());

  it('opens the review the agent on screen publishes, not another agent’s', async () => {
    const { emit } = await start();
    expect(app.agent?.id).toBe('a1');
    emit({ type: 'agent', agent: agent({ reviews: ['rv1'] }) });
    expect(app.panelTab).toBe('review');
    expect(app.reviewShown).toEqual({ a1: 'rv1' });
    expect(app.split).toBe(true);
    // Another agent's review does not take the panel.
    emit({ type: 'agent', agent: agent({ id: 'a2', name: 'tests-e2e', createdAt: 2, reviews: ['rv2'] }) });
    expect(app.reviewShown).toEqual({ a1: 'rv1' });
  });
});
```

`src/lib/state.test.ts` importe déjà `agent` et `resetApp` ? Vérifier la ligne 2 : `agent, board, fakeBackend, gitInfo, project, resetApp, SETTINGS, ticket` — oui.

- [ ] **Step 2 : lancer, constater l'échec**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run src/components/conv/ReviewCard.test.ts src/components/SidePanel.test.ts src/lib/state.test.ts -t "review|Review"`
Expected : FAIL (`ReviewCard.svelte` introuvable, pas d'onglet « Revue », `panelTab` reste `files`).

- [ ] **Step 3 : l'état**

Dans `src/lib/state.svelte.ts`, ajouter après `toggleLayout()` (l.706-709) :

```ts
  /** Shows the review `reviewId` of the agent `agentId` in the right half of the window (its tab, the split layout). */
  openReview(agentId: string, reviewId: string) {
    this.reviewShown[agentId] = reviewId;
    this.panelTab = 'review';
    if (!this.split) this.toggleLayout();
  }

  /** A review the agent on screen just published opens by itself. */
  private noteReview(prev: Agent | undefined, next: Agent) {
    const fresh = next.reviews.find((id) => !prev?.reviews.includes(id));
    if (fresh && this.ui.view === 'project' && this.agent?.id === next.id) this.openReview(next.id, fresh);
  }
```

Dans `onEvent`, `case 'agent'`, après `this.noteAttention(prev, e.agent);` :

```ts
        this.noteReview(prev, e.agent);
```

- [ ] **Step 4 : la carte et la conversation**

Créer `src/components/conv/ReviewCard.svelte` :

```svelte
<script lang="ts">
  import { t } from '../../lib/i18n';
  import { app } from '../../lib/state.svelte';
  import type { ReviewItem } from '../../lib/types';

  // A review's card in the conversation: its title and whether it waits; a click opens its page on the right.
  let { item, agentId }: { item: ReviewItem; agentId: string } = $props();
  const waiting = $derived(item.statut === 'attente');
</script>

<button class="card" class:waiting onclick={() => app.openReview(agentId, item.id)}>
  <span class="title">{t('conv.review.card', { title: item.contenu.titre })}</span>
  <span class="state">{waiting ? t('conv.review.waiting') : t('conv.review.answered')}</span>
</button>

<style>
  .card {
    margin-left: 34px;
    display: flex;
    align-items: center;
    gap: 10px;
    width: calc(100% - 34px);
    padding: 10px 14px;
    border: 1px solid var(--line);
    border-left: 3px solid var(--accent);
    border-radius: var(--r);
    background: var(--elev);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .card.waiting {
    border-color: var(--wait);
    border-left-color: var(--wait);
    background: var(--wait-soft);
  }
  .title {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    font-size: 11px;
    color: var(--muted);
  }
  .waiting .state {
    color: var(--wait);
  }
</style>
```

Dans `src/components/Conversation.svelte`, après `import QuestionCard from './conv/QuestionCard.svelte';` (l.27) :

```ts
  import ReviewCard from './conv/ReviewCard.svelte';
```

et avant `{:else if item.kind === 'notice'}` (l.689) :

```svelte
              {:else if item.kind === 'review'}
                <ReviewCard {item} agentId={agent.id} />
```

- [ ] **Step 5 : l'onglet du panneau**

`src/lib/i18n/fr/nav.ts:94` : `sidePanel: { uncommitted: 'Non commités', history: 'Historique', review: 'Revue' },`
`src/lib/i18n/en/nav.ts:85` : `sidePanel: { uncommitted: 'Uncommitted', history: 'History', review: 'Review' },`

Dans `src/components/SidePanel.svelte`, script :

```ts
  import { conversationOf } from '../lib/conversations.svelte';
  import { shownReview } from '../lib/review';
  import ReviewPage from './ReviewPage.svelte';
```

et après `const tab = $derived(app.panelTab);` :

```ts
  // The review of the agent shown: the one chosen, else its latest that waits (its conversation is read for it).
  const conv = $derived(agent ? conversationOf(agent.id) : null);
  const review = $derived(agent && conv ? shownReview(conv.items, app.reviewShown[agent.id]) : null);
```

Dans le gabarit, après le bouton « Historique » :

```svelte
      <button role="tab" aria-selected={tab === 'review'} class:on={tab === 'review'} onclick={() => (app.panelTab = 'review')}>
        {t('nav.sidePanel.review')}
        {#if agent?.reviews.length}<span class="count">{agent.reviews.length}</span>{/if}
      </button>
```

et remplacer :

```svelte
  {#if tab === 'files'}
    <FilesPanel {project} {agent} {docked} bind:count />
  {:else}
    <GitGraph {project} {agent} />
  {/if}
```

par :

```svelte
  {#if tab === 'files'}
    <FilesPanel {project} {agent} {docked} bind:count />
  {:else if tab === 'history'}
    <GitGraph {project} {agent} />
  {:else if agent && review}
    {#key `${agent.id}/${review.id}`}
      <ReviewPage item={review} agentId={agent.id} />
    {/key}
  {:else}
    <p class="none">{t('conv.review.none')}</p>
  {/if}
```

et dans `<style>` :

```css
  .none {
    margin: 0;
    padding: 0 14px;
    color: var(--dim);
    font-size: 13px;
  }
```

- [ ] **Step 6 : lancer les tests et le typage**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run src/components/conv/ReviewCard.test.ts src/components/SidePanel.test.ts src/lib/state.test.ts src/components/Conversation.test.ts && npm run check`
Expected : PASS (les tests existants de `SidePanel` aussi : l'onglet « Historique » garde son nom exact) ; `svelte-check` 0 erreur.

- [ ] **Step 7 : commit**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
git add src/components/conv/ReviewCard.svelte src/components/conv/ReviewCard.test.ts src/components/Conversation.svelte src/components/SidePanel.svelte src/components/SidePanel.test.ts src/lib/state.svelte.ts src/lib/state.test.ts src/lib/i18n/fr/nav.ts src/lib/i18n/en/nav.ts
git commit -m "feat(revue): carte dans la conversation et onglet Revue du panneau de droite"
```

---

### Task 8 : l'agent en attente de revue s'affiche en ambre

**Files:**
- Modify: `src/lib/state.svelte.ts:637-649` (`noteAttention`), l.1022-1031 (`nextWaiting`)
- Modify: `src/components/Sidebar.svelte:19` (import), l.287, l.295, l.328-329
- Modify: `src/components/TitleBar.svelte:172-174`
- Test: `src/components/Sidebar.test.ts`, `src/lib/state.test.ts`

**Interfaces:**
- Consumes : `shownStatus` (Task 5), clé `conv.review.pill` (Task 6).
- Produces : rien de nouveau ; `Composer.svelte`, `TurnCard.svelte`, `Conversation.svelte` gardent `agent.status` (l'agent n'a pas de tour en cours : le compositeur doit rester libre).

- [ ] **Step 1 : écrire le test qui échoue**

Dans `src/components/Sidebar.test.ts`, `describe('Sidebar')` :

```ts
  it('shows an agent whose review waits as waiting for an answer', () => {
    fakeBackend();
    app.agents.a1 = { ...app.agents.a1, status: 'done', reviews: ['rv1'] };
    render(Sidebar, { project: project() });
    const card = screen.getByRole('button', { name: /refacto-auth/ });
    expect(within(card).getByText('Revue')).toBeInTheDocument();
    expect(card.style.getPropertyValue('--alert')).toBe('var(--wait)');
  });
```

Dans `src/lib/state.test.ts`, `describe('reviews')` (créé en Task 7) :

```ts
  it('calls for a look at an agent out of sight whose review waits, and Ctrl+J goes to it', async () => {
    const { emit } = await start();
    // a2 is not on screen (a1 is): its turn ends with a review that waits.
    emit({ type: 'agent', agent: agent({ id: 'a2', name: 'tests-e2e', createdAt: 2, status: 'done', reviews: ['rv2'] }) });
    expect(app.attention.a2).toBe(true);
    app.nextWaiting();
    expect(app.agent?.id).toBe('a2');
  });
```

- [ ] **Step 2 : lancer, constater l'échec**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run src/components/Sidebar.test.ts src/lib/state.test.ts -t "review waits"`
Expected : FAIL, `Unable to find an element with the text: Revue` et `expected undefined to be true` (`attention.a2`).

- [ ] **Step 3 : Sidebar et TitleBar**

`src/components/Sidebar.svelte`, après `import { planView } from '../lib/plan';` :

```ts
  import { shownStatus } from '../lib/review';
```

l.287 :

```svelte
        style:--alert={shownStatus(a) === 'error' ? 'var(--del)' : shownStatus(a) === 'waiting' ? 'var(--wait)' : 'var(--ok)'}
```

l.295 :

```svelte
          <StatusDot status={shownStatus(a)} size={8} />
```

l.328-329, remplacer :

```svelte
          {#if a.status === 'waiting'}
            <span class="pill">{statusLabel('waiting')}</span>
```

par :

```svelte
          {#if a.status === 'waiting'}
            <span class="pill">{statusLabel('waiting')}</span>
          {:else if shownStatus(a) === 'waiting'}
            <span class="pill">{t('conv.review.pill')}</span>
```

`src/components/TitleBar.svelte`, ajouter l'import `import { shownStatus } from '../lib/review';` puis l.172-174 :

```ts
    const alert = mostPressing(seen.map((a) => alertColor(shownStatus(a))));
    return {
      waiting: agents.filter((a) => shownStatus(a) === 'waiting').length,
```

- [ ] **Step 4 : l'attention et Ctrl+J**

`src/lib/state.svelte.ts`, ajouter l'import `import { shownStatus } from './review';` puis remplacer `noteAttention` (l.637-649) :

```ts
  /** An agent that comes to ask, finish or fail out of sight needs a look (a review that waits asks too). */
  private noteAttention(prev: Agent | undefined, next: Agent) {
    const status = shownStatus(next);
    if (!ALERT.has(status) || next.archived) {
      delete this.attention[next.id];
    } else if (
      prev &&
      shownStatus(prev) !== status &&
      !this.onScreen(next.id) &&
      (status === 'waiting' || !this.ticketDoing(next.id))
    ) {
      this.attention[next.id] = true;
    }
  }
```

et dans `nextWaiting` (l.1024) :

```ts
      .filter((a) => !a.archived && (shownStatus(a) === 'waiting' || this.attention[a.id]))
```

- [ ] **Step 5 : lancer les tests et le typage**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npx vitest run src/components/Sidebar.test.ts src/components/TitleBar.test.ts src/lib/state.test.ts && npm run check`
Expected : PASS ; `svelte-check` 0 erreur.

- [ ] **Step 6 : commit**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
git add src/lib/state.svelte.ts src/components/Sidebar.svelte src/components/Sidebar.test.ts src/components/TitleBar.svelte
git commit -m "feat(revue): l'agent dont la revue attend s'affiche en ambre"
```

---

### Task 9 : vérification complète et essai manuel dans le bac à sable

**Files:**
- Aucun fichier du dépôt (hors correctifs éventuels).

- [ ] **Step 1 : toute la suite front et le typage**

Run : `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad" && npm run check && npx vitest run`
Expected : 0 erreur de typage ; tous les tests Vitest PASS.

- [ ] **Step 2 : les tests Rust du périmètre**

Run (séparément) :
- `cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad/src-tauri" && cargo test review::`
- `cargo test mcp::act_tests`
- `cargo test mcp::tests::the_server_offers_exactly_its_tools`
- `cargo test agent::tests`
- `cargo clippy --all-targets -- -D warnings`

Expected : PASS. Les seuls échecs admis sont ceux qui attendent `escouade` / `.escouade` (renommage du fork). En cas de doute, relancer le même test sur le commit d'avant la Task 1 (`git log --oneline -12` pour le trouver, `git worktree add /tmp/avant <sha>`) : s'il échouait déjà, ce n'est pas une régression.

- [ ] **Step 3 : lancer l'app dans le bac à sable**

```bash
cd "/Users/robin-le-gal/dev/Nobo Squad/OshO Squad"
ESCOUADE_DATA_DIR="$TMPDIR/osho-sandbox" npm run tauri dev -- --config src-tauri/tauri.osho.conf.json
```

1. Réglages → activer « Claude et les agents peuvent piloter Escouade » (le serveur MCP doit tourner : `mcpEnabled`).
2. Ajouter un projet (n'importe quel dépôt git de test), créer un agent **en vue Escouade**.
3. Lui envoyer :
   ```
   Appelle l'outil show_review avec ce JSON, puis termine ton tour :
   {"titre":"Plan du parseur","sur":"Plan","chapo":"Trois lots.","puces":[{"texte":"Migration risquée","ton":"alerte"}],"questions":[{"id":"q1","titre":"Exécution ?","choix":[{"valeur":"sub","libelle":"Sous-agents","reco":true},{"valeur":"inline","libelle":"Ici","detail":"Plus lent"}]},{"id":"q2","titre":"Nom du module ?","placeholder":"parser"}]}
   ```
   Si Claude Code demande l'autorisation d'utiliser l'outil, l'accepter.
4. Vérifier : la disposition passe en moitié / moitié, l'onglet « Revue » montre la page, « Sous-agents » est coché avec « recommandé », la puce est rouge, la carte « Revue : Plan du parseur · en attente » est dans la conversation, l'outil s'affiche « Escouade · Afficher une revue » (ou `osho-squad·show_review` : voir risques).
5. Fin du tour : la carte de l'agent dans la barre latérale est ambre avec la pastille « Revue », une notification « Revue à remplir : Plan du parseur » arrive (fenêtre en arrière-plan), le badge de la barre de menus compte 1.
6. Choisir « Ici », taper `lexer`, « Autre chose ? » = `Merci`, cliquer « Envoyer » : un message « Réponses à « Plan du parseur » : … » apparaît comme le tien, l'agent reprend, la page passe en lecture seule « répondue », l'ambre disparaît.
7. Recliquer la carte : la page répondue s'affiche ; aucun bouton « Envoyer ».
8. Refaire 3 sans répondre, quitter l'app, la relancer (même commande) : l'agent est ambre, la carte « en attente » est là, la page se remplit et s'envoie.
9. Basculer un agent en vue Claude Code avec une revue en attente, **sans** ouvrir son terminal, et cliquer « Envoyer » : un toast dit d'ouvrir le terminal, la page garde les réponses et reste « en attente ». Ouvrir le terminal, renvoyer : le message est tapé dans le terminal.

- [ ] **Step 4 : le skill revue-artefact (hors de ce dépôt)**

Dans le dépôt OshO-Skillz, en tête de la méthode du skill `revue-artefact`, ajouter la ligne :

```
Si l'outil MCP `show_review` est disponible (session OshO Squad), l'appeler avec le contenu JSON au lieu de publier un artefact ; les réponses arrivent comme un message.
```

Ne rien changer d'autre au skill. Ce changement se commite dans OshO-Skillz, pas ici.

- [ ] **Step 5 : commit éventuel**

S'il a fallu corriger quelque chose pendant l'essai : committer ces corrections avec un message `fix(revue): …` décrivant le problème vu. Sinon, rien à committer.

---

## Hors plan (volontairement non fait)

- `Overview.svelte` et `StatusBar.svelte` comptent encore `status === 'waiting'` seul : une revue en attente n'y apparaît pas comme « question ». À brancher sur `shownStatus` si le besoin se montre.
- L'outil `list_agents` / `list_requests` ne dit rien des revues en attente.
- Pas de doc du site (`website/content/docs/*/mcp.md`) pour `show_review`.
- Pas de contrôle d'unicité des `valeur` d'une même question (la page reste stable grâce aux clés par index).
