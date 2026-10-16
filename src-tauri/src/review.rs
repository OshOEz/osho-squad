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
}
