//! A review an agent publishes for the user (`show_review`, a tool of the MCP server): a page of
//! questions shown in the side panel, whose answers come back to the agent as the user's message.
//! The page is the item `review` of its conversation: waiting (`attente`) until answered
//! (`repondue`). The answers are data: shown and sent as text, never run.

use crate::agent::{AgentRt, Effects};
use crate::core::Core;
use crate::i18n::{self, Lang};
use crate::model::{new_id, now_ms, AgentMeta, AgentStatus};
use anyhow::{anyhow, bail, Result};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use tauri::Runtime;

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
    #[schemars(
        description = "Key points, each a short text (texte); ton \"alerte\" shows it in red."
    )]
    #[serde(default)]
    pub puces: Vec<Bullet>,
    #[schemars(
        description = "Sections, each with a title (titre) and its items or paragraphs (paragraphes)."
    )]
    #[serde(default)]
    pub sections: Vec<Section>,
    #[schemars(description = "The heading above the questions (Questions when left out).")]
    pub titre_questions: Option<String>,
    #[schemars(
        description = "The questions, 1 to 20, each with an id of its own: with choix, a choice (reco true marks the recommended one, preselected); without, a free text."
    )]
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
            return Err(tr!("Une question n’a pas d’id.", "A question has no id."));
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
        lines.push(tr_in!(
            lang,
            "- Autre chose : {}",
            "- Anything else: {}",
            note
        ));
    }
    lines.join("\n")
}

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
    Some(tr!(
        "Revue à remplir : {titre}",
        "Review to answer: {titre}"
    ))
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
    pub fn answer_review(
        &mut self,
        id: &str,
        answers: &Answers,
        fx: &mut Effects,
    ) -> Result<String> {
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

impl<R: Runtime> Core<R> {
    /// Shows `review` (checked) in the conversation of the agent `id`: its item's id.
    pub fn show_review(self: &Arc<Self>, id: &str, review: &Review) -> Result<String> {
        self.with_agent(id, |rt, fx| Ok(rt.show_review(review, fx)))
    }

    /// The user's answers to the review `review` of the agent `id`: answered, then sent to the
    /// agent as the user's message (`send_message`: a turn in the Escouade view, typed in its
    /// terminal in the Claude Code view, its process started again if it was stopped). Not
    /// delivered, the review waits again with its answers.
    pub async fn answer_review(
        self: &Arc<Self>,
        id: &str,
        review: &str,
        answers: Answers,
    ) -> Result<()> {
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::{json, Value};

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
            questions: [
                ("q1".into(), "inline".into()),
                ("q2".into(), "lexer".into()),
            ]
            .into(),
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
        let blank = Review {
            titre: "  ".into(),
            ..review()
        };
        assert!(checked(blank).unwrap_err().contains("titre"));

        let none = Review {
            questions: Vec::new(),
            ..review()
        };
        let why = checked(none).unwrap_err();
        assert!(
            why.contains("0 questions") && why.contains("1 à 20"),
            "{why}"
        );

        let many = Review {
            questions: questions(21),
            ..review()
        };
        assert!(checked(many).unwrap_err().contains("21 questions"));
        assert!(checked(Review {
            questions: questions(20),
            ..review()
        })
        .is_ok());

        let mut twice = questions(2);
        twice[1].id = " q0 ".into();
        let why = checked(Review {
            questions: twice,
            ..review()
        })
        .unwrap_err();
        assert!(why.contains("« q0 »"), "{why}");

        let mut nameless = questions(1);
        nameless[0].id = " ".into();
        assert!(checked(Review {
            questions: nameless,
            ..review()
        })
        .unwrap_err()
        .contains("id"));

        let heavy = Review {
            chapo: Some("x".repeat(BYTES_MAX)),
            ..review()
        };
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
        assert!(
            text.contains("Ici (inline)") && text.contains("lexer"),
            "{text}"
        );
        let item = a.conv.get(&id).unwrap().clone();
        assert_eq!(item["statut"], "repondue");
        assert_eq!(
            item["reponses"],
            json!({ "questions": { "q1": "inline", "q2": "lexer" }, "note": "Merci" })
        );
        assert!(a.meta.reviews.is_empty());
        assert!(fx.agent_changed && fx.save);
        // A second answer is refused, and changes nothing.
        let again = a.answer_review(&id, &Answers::default(), &mut Effects::default());
        assert!(format!("{:#}", again.unwrap_err()).contains("déjà"));
        assert_eq!(a.conv.get(&id).unwrap()["reponses"]["note"], "Merci");
        // An id that is no review of this conversation.
        assert!(a
            .answer_review("zzz", &answers(), &mut Effects::default())
            .is_err());
        // A turn that ends after it no longer speaks of the review.
        a.push_user("u1", "Suite", 0, &[], &mut Effects::default());
        assert_eq!(ends(&mut a).notify.unwrap().kind, NotifyKind::Done);
    }

    #[test]
    fn answers_that_did_not_reach_the_agent_wait_again_and_are_kept() {
        let dir = test_dir("review-reopen");
        let mut a = agent(&dir);
        let id = a.show_review(&review(), &mut Effects::default());
        a.answer_review(&id, &answers(), &mut Effects::default())
            .unwrap();
        a.reopen_review(&id, &mut Effects::default());
        let item = a.conv.get(&id).unwrap().clone();
        assert_eq!(item["statut"], "attente");
        assert_eq!(item["reponses"]["questions"]["q1"], "inline");
        assert_eq!(a.meta.reviews, [id.clone()]);
        // Answered again then, once.
        assert!(a
            .answer_review(&id, &answers(), &mut Effects::default())
            .is_ok());
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
        assert!(back
            .answer_review(&id, &answers(), &mut Effects::default())
            .is_ok());
        // An agent saved before reviews has none.
        let old: AgentMeta =
            serde_json::from_value(json!({ "id": "a1", "name": "vieux" })).unwrap();
        assert!(old.reviews.is_empty());
    }

    use crate::accounts::PRINCIPAL;
    use crate::core_tests::{agent_on, harness};

    #[tokio::test]
    async fn the_answers_reach_the_agent_as_the_users_message_and_only_once() {
        let h = harness("review-answer");
        let (p, _) = h.project(false).await;
        let a = agent_on(&h, &p, PRINCIPAL).await;
        let id = h
            .core
            .show_review(&a.id, &checked(review()).unwrap())
            .unwrap();
        assert_eq!(h.agent(&a.id).reviews, [id.clone()]);
        // The window is told: the item, then the agent's view with its review.
        assert!(h
            .events
            .lock()
            .iter()
            .any(|e| e["type"] == "conv" && e["ops"][0]["item"]["kind"] == "review"));
        assert!(h
            .events
            .lock()
            .iter()
            .any(|e| e["type"] == "agent" && e["agent"]["reviews"][0] == id.as_str()));

        h.core.answer_review(&a.id, &id, answers()).await.unwrap();
        h.wait("the turn of the answers", |h| {
            h.items(&a.id).iter().any(|i| i["kind"] == "turn") && !h.agent(&a.id).status.is_active()
        })
        .await;
        let items = h.items(&a.id);
        let user = items.iter().find(|i| i["kind"] == "user").unwrap();
        let text = user["text"].as_str().unwrap();
        assert!(
            text.contains("Plan du parseur") && text.contains("Ici (inline)"),
            "{text}"
        );
        let item = items.iter().find(|i| i["id"] == id.as_str()).unwrap();
        assert_eq!(item["statut"], "repondue");
        assert!(h.agent(&a.id).reviews.is_empty());

        let again = h
            .core
            .answer_review(&a.id, &id, answers())
            .await
            .unwrap_err();
        assert!(format!("{again:#}").contains("déjà"), "{again:#}");
        // No second message.
        assert_eq!(
            h.items(&a.id)
                .iter()
                .filter(|i| i["kind"] == "user")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn answers_that_cannot_reach_the_agent_leave_the_review_waiting_with_them() {
        let h = harness("review-answer-fails");
        let (p, _) = h.project(false).await;
        let a = agent_on(&h, &p, PRINCIPAL).await;
        let id = h
            .core
            .show_review(&a.id, &checked(review()).unwrap())
            .unwrap();
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
}
