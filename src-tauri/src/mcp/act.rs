//! What the tools that act do (`tools`): what the window does, through the same functions of the
//! core (so the window is told, the external tickets kept in step and the board scheduled as for
//! an action made there), under the guardrails of the window and the autopilot:
//!
//! - a ticket or an agent is not started while the autopilot has no account to start it on: every
//!   account the project allows (the one it prefers, else any active one) is held back, by a quota
//!   window over the threshold, the pause after a usage limit or an agent waiting for its quota.
//!   One account free is enough. A message is not sent to an agent whose own account is held back.
//!   The refusal says until when;
//! - a project's most tickets in parallel (« En parallèle » in its Kanban settings) are not
//!   exceeded by a ticket launched, nor its most agents working at once by an agent created;
//! - a ticket that comes after tickets not done is launched by hand only once the user agreed in
//!   the window (a confirmation): here it is refused, and the refusal says which tickets;
//! - an agent never writes to nor stops itself, and acts on its own project alone (it reads the
//!   others: the tools that read are open);
//! - a project's Kanban settings are set as the window's form sets them (`set_board`, the same
//!   function of the core), under its checks; what runs without a question stays the user's, given
//!   in the window: the mode `bypassPermissions` (kept when the Kanban is on it already) and the
//!   test command, which tests first needs (the server sets none);
//! - a message sent to an agent, and the first one of an agent created, reach it under its author's
//!   name (« Message de <author> : »): never as the user's own words;
//! - an agent's request (a permission, a question, a plan) is answered as the window's card does,
//!   by any client but never for good (« Toujours autoriser » is refused: a lasting rule is the
//!   user's), never by the agent that asked, and only once: a request that no longer waits (the user
//!   or another client answered it, the agent stopped) or one of an agent in the Claude Code view
//!   (its interface asks in its terminal) is refused with the reason. The card of the conversation
//!   says who answered, and a refusal tells Claude who refused;
//! - the tools of the board's flow (`approve_ticket`, `reject_ticket`, `resume_ticket`,
//!   `resolve_conflict`, `dismiss_block`, `delete_ticket`) act as the window's buttons do and are
//!   refused where the window refuses, in its words (`delete_ticket` also while the ticket's approval
//!   runs: it would go on without it); all but the last answer the ticket and its `since`, for
//!   `wait_ticket`. An agent never approves nor sends back its own ticket, nor acts on it with the
//!   others: that is the user's, or another client's. What the flow sends to an agent (a ticket sent
//!   back, resumed, a conflict handed over) goes only while the agent's account is not held back, and
//!   a ticket sent back says who sent it back.
//!
//! A refusal (`ToolError::Refused`) is a guard saying no; a failure (`ToolError::Failed`) is
//! something asked that does not fit (a name, an empty title…). `report_progress`,
//! `show_review` and `split_ticket` are for the agents of Escouade alone.

use super::activity;
use super::images;
use super::read::{
    answerable, asked_questions, board_view, json, request_type, ticket_view, todo_keys, TicketView,
};
use super::resolve;
use super::tools::{
    AnswerPermissionArgs, AnswerQuestionArgs, ApproveTicketArgs, CreateAgentArgs, CreateTicketArgs,
    DecisionArg, DeleteTicketArgs, DismissBlockArgs, MoveTicketArgs, PositionArg, QuestionAnswer,
    RejectTicketArgs, ReportProgressArgs, ResolveConflictArgs, ResumeTicketArgs, SendMessageArgs,
    SetBoardArgs, SplitTicketArgs, StartTicketArgs, StopAgentArgs, SubTicket, ToolError,
    UpdateTicketArgs, SPLIT_MAX,
};
use super::wait;
use super::Caller;
use crate::accounts;
use crate::agent::Waiting;
use crate::board::{Held, MoveTo};
use crate::core::{supports_effort, AgentOptions, Core};
use crate::git;
use crate::i18n;
use crate::model::{BoardSettings, Column, PauseReason, Project, Ticket, PROGRESS_LINE_MAX};
use crate::review::{self, Review};
use crate::telemetry::Event;
use crate::tickets::{
    comment_missing, only_todo_is_edited, ticket_agent_options, title_missing, Refusal, TicketDraft,
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;
use tauri::Runtime;

/// The longest title a ticket made here has, in characters.
const TITLE_MAX: usize = 200;
/// The longest description, and criterion, in characters; the most criteria.
const DESCRIPTION_MAX: usize = 10_000;
const CRITERION_MAX: usize = 500;
const CRITERIA_MAX: usize = 30;

/// A guard said no.
fn refusal(e: anyhow::Error) -> ToolError {
    ToolError::Refused(format!("{e:#}"))
}

/// Something asked does not fit.
fn failure(e: anyhow::Error) -> ToolError {
    ToolError::Failed(format!("{e:#}"))
}

/// A tool for the agents of Escouade alone: the calling agent's id.
fn agent_only(caller: &Caller) -> Result<&str, ToolError> {
    match caller {
        Caller::Agent(id) => Ok(id),
        Caller::External => Err(ToolError::Refused(tr!(
            "Cet outil est réservé aux agents d’Escouade : Claude hors Escouade ne peut pas l’appeler.",
            "This tool is reserved to Escouade’s own agents: Claude outside Escouade cannot call it."
        ))),
    }
}

/// An agent of Escouade acts on its own project alone (it may read the others); Claude outside
/// Escouade acts on any.
pub(super) fn own_project_only<R: Runtime>(
    core: &Core<R>,
    caller: &Caller,
    project_id: &str,
) -> Result<(), ToolError> {
    let Caller::Agent(id) = caller else {
        return Ok(());
    };
    let (name, own) = match core.agent(id) {
        Ok(h) => {
            let rt = h.lock();
            (rt.meta.name.clone(), rt.meta.project_id.clone())
        }
        Err(_) => (id.clone(), String::new()),
    };
    if own == project_id {
        return Ok(());
    }
    let project_name = |id: &str| core.project(id).map_or_else(|_| id.to_string(), |p| p.name);
    let (own_name, other) = (project_name(&own), project_name(project_id));
    Err(ToolError::Refused(tr!(
        "L’agent {name} n’agit que dans son propre projet ({own_name}) : le projet {other} n’est pas le sien.",
        "The agent {name} only acts in its own project ({own_name}): the project {other} is not its own."
    )))
}

// ---------- what is asked ----------

/// A title: on one line, not blank, `TITLE_MAX` characters at most.
fn title(asked: &str) -> Result<String, ToolError> {
    let line = one_line(asked);
    if line.is_empty() {
        return Err(failure(title_missing()));
    }
    let n = line.chars().count();
    if n > TITLE_MAX {
        return Err(ToolError::Failed(tr!(
            "Le titre fait {n} caractères : {TITLE_MAX} au plus.",
            "The title is {n} characters long: {TITLE_MAX} at most."
        )));
    }
    Ok(line)
}

/// A description: trimmed, `DESCRIPTION_MAX` characters at most.
fn description(asked: &str) -> Result<String, ToolError> {
    let text = asked.trim();
    let n = text.chars().count();
    if n > DESCRIPTION_MAX {
        return Err(ToolError::Failed(tr!(
            "La description fait {n} caractères : {DESCRIPTION_MAX} au plus.",
            "The description is {n} characters long: {DESCRIPTION_MAX} at most."
        )));
    }
    Ok(text.to_string())
}

/// The criteria, one per line (as the window's form takes them), blank lines dropped.
fn criteria(asked: &[String]) -> Result<Vec<String>, ToolError> {
    let lines: Vec<String> = asked
        .iter()
        .flat_map(|c| c.lines())
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() > CRITERIA_MAX {
        return Err(ToolError::Failed(tr!(
            "{count} critères : {CRITERIA_MAX} au plus.",
            "{count} criteria: {CRITERIA_MAX} at most.",
            count = lines.len()
        )));
    }
    if let Some(long) = lines.iter().find(|l| l.chars().count() > CRITERION_MAX) {
        let n = long.chars().count();
        return Err(ToolError::Failed(tr!(
            "Un critère fait {n} caractères : {CRITERION_MAX} au plus.",
            "A criterion is {n} characters long: {CRITERION_MAX} at most."
        )));
    }
    Ok(lines)
}

/// The line that ends the description of a ticket made through the server, « Créé par <origin> via
/// Escouade »: the card shows it, and so does the protocol its agent is given, so that nobody
/// takes the ticket for the user's own. `origin` is the caller as the activity log names it.
pub(super) fn made_by(lang: i18n::Lang, origin: &str) -> String {
    let origin = one_line(origin);
    tr_in!(
        lang,
        "Créé par {origin} via Escouade",
        "Created by {origin} through Escouade"
    )
}

/// The line that ends the description of a ticket edited through the server, « Modifié par
/// <origin> via Escouade »: it takes the place of the one that was there (`without_origin`).
pub(super) fn changed_by(lang: i18n::Lang, origin: &str) -> String {
    let origin = one_line(origin);
    tr_in!(
        lang,
        "Modifié par {origin} via Escouade",
        "Modified by {origin} through Escouade"
    )
}

/// `description` followed, after a blank line, by the line `line` (alone when it has no text).
pub(super) fn with_origin(description: &str, line: &str) -> String {
    if description.is_empty() {
        line.to_string()
    } else {
        format!("{description}\n\n{line}")
    }
}

/// `description` without the line `with_origin` put at its end (« Créé par … via Escouade » or
/// « Modifié par … via Escouade », in either language: the interface's may have changed since) and
/// the blank line before it. Only a line set apart by a blank line, or alone, is one: the user's
/// own last line that looks like it stays.
pub(super) fn without_origin(description: &str) -> &str {
    const LINES: [(&str, &str); 4] = [
        ("Créé par ", " via Escouade"),
        ("Modifié par ", " via Escouade"),
        ("Created by ", " through Escouade"),
        ("Modified by ", " through Escouade"),
    ];
    let text = description.trim_end();
    let (before, last) = match text.rfind('\n') {
        Some(at) => (&text[..at], &text[at + 1..]),
        None => ("", text),
    };
    let is_origin = LINES.iter().any(|(start, end)| {
        last.len() > start.len() + end.len() && last.starts_with(start) && last.ends_with(end)
    });
    // `before` ends with the newline of the blank line, or is empty when the line is alone.
    if is_origin && ((before.is_empty() && text == last) || before.ends_with('\n')) {
        before.trim_end()
    } else {
        description
    }
}

/// The description of a ticket made through the server: what the caller wrote, then who made it.
/// The limit of `description` is on what was written, not on this line.
fn described<R: Runtime>(
    core: &Core<R>,
    caller: &Caller,
    written: &str,
) -> Result<String, ToolError> {
    let origin = activity::caller_name(core, Some(caller));
    Ok(with_origin(
        &description(written)?,
        &made_by(i18n::ui(), &origin),
    ))
}

/// `text` on one line: breaks, tabs and runs of spaces are single spaces.
fn one_line(text: &str) -> String {
    let spaced: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A character that is not drawn, or drawn as something else than itself: the controls, the
/// format characters (zero-width marks, direction marks and overrides…), the line and paragraph
/// separators and the ones ignored when drawn (variation selectors, tag characters…). Written as
/// ranges as the window's `revealHidden` has them (it decides by Unicode property, which `std`
/// does not know).
fn is_hidden(c: char) -> bool {
    const RANGES: &[(u32, u32)] = &[
        (0x00AD, 0x00AD),   // soft hyphen
        (0x034F, 0x034F),   // combining grapheme joiner
        (0x0600, 0x0605),   // Arabic number signs
        (0x061C, 0x061C),   // Arabic letter mark
        (0x06DD, 0x06DD),   // Arabic end of ayah
        (0x070F, 0x070F),   // Syriac abbreviation mark
        (0x0890, 0x0891),   // Arabic pound and piastre marks
        (0x08E2, 0x08E2),   // Arabic disputed end of ayah
        (0x115F, 0x1160),   // hangul fillers
        (0x17B4, 0x17B5),   // Khmer inherent vowels
        (0x180B, 0x180F),   // Mongolian free variation selectors
        (0x200B, 0x200F),   // zero-width marks, direction marks
        (0x2028, 0x202E),   // separators, direction embeddings and overrides
        (0x2060, 0x206F),   // word joiner, invisible operators, deprecated format characters
        (0x2800, 0x2800),   // braille blank
        (0x3164, 0x3164),   // hangul filler
        (0xFE00, 0xFE0F),   // variation selectors
        (0xFEFF, 0xFEFF),   // zero-width no-break space
        (0xFFA0, 0xFFA0),   // halfwidth hangul filler
        (0xFFF0, 0xFFFB),   // unassigned, interlinear annotation
        (0x110BD, 0x110BD), // Kaithi number sign
        (0x110CD, 0x110CD), // Kaithi number sign above
        (0x13430, 0x1343F), // Egyptian hieroglyph format controls
        (0x1BCA0, 0x1BCA3), // shorthand format controls
        (0x1D173, 0x1D17A), // musical format controls
        (0xE0000, 0xE0FFF), // tag characters, variation selectors supplement
    ];
    c.is_control()
        || RANGES
            .iter()
            .any(|&(from, to)| (from..=to).contains(&(c as u32)))
}

/// `text` as a status line is kept: on one line (breaks, tabs and runs of spaces are single
/// spaces) and with none of the characters that are not drawn (`is_hidden`), which could hide
/// part of it or reverse its reading (U+202E).
pub(crate) fn visible_line(text: &str) -> String {
    let spaced: String = text
        .chars()
        .filter_map(|c| match c {
            c if c.is_whitespace() || c.is_control() => Some(' '),
            c if is_hidden(c) => None,
            c => Some(c),
        })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The ids of the tickets of `project` the keys (or ids) name.
fn after_ids<R: Runtime>(
    core: &Core<R>,
    project: &Project,
    asked: &[String],
) -> Result<Vec<String>, ToolError> {
    asked
        .iter()
        .map(|k| resolve::ticket_in(core, k, Some(project)).map(|t| t.id))
        .collect()
}

/// The project a ticket is in.
fn project_of<R: Runtime>(core: &Core<R>, project_id: &str) -> Result<Project, ToolError> {
    core.project(project_id).map_err(failure)
}

// ---------- guardrails ----------

/// A time as the refusals say it: "14:30" today, "2026-10-12 14:30" another day.
fn when<R: Runtime>(core: &Core<R>, ms: i64) -> String {
    use chrono::TimeZone;
    let at = chrono::Local.timestamp_millis_opt(ms).single();
    let now = chrono::Local
        .timestamp_millis_opt(core.pause_now())
        .single();
    match (at, now) {
        (Some(at), Some(now)) if at.date_naive() == now.date_naive() => {
            at.format("%H:%M").to_string()
        }
        (Some(at), _) => at.format("%Y-%m-%d %H:%M").to_string(),
        _ => String::new(),
    }
}

/// What `held` says, and until when: the pause of the account (or of the first account to be free
/// again), else an agent waiting for its quota.
fn why<R: Runtime>(core: &Core<R>, held: &Held) -> String {
    let p = &held.pause;
    let until = when(core, p.until);
    let pct = p.pct.map_or(0, |pct| pct.round() as i64);
    if held.by_agent {
        return tr!(
            "Un agent attend son quota et reprend à {until} : rien ne démarre d’ici là. Réessaie à ce moment-là.",
            "An agent is waiting for its quota and resumes at {until}: nothing starts until then. Try again then."
        );
    }
    match p.reason {
        PauseReason::Limit => tr!(
            "Le pilote auto est en pause jusqu’à {until} : la limite d’usage est atteinte. Réessaie à ce moment-là.",
            "The autopilot is paused until {until}: the usage limit is reached. Try again then."
        ),
        PauseReason::FiveHour => tr!(
            "Le pilote auto est en pause jusqu’à {until} : la fenêtre de 5 h est utilisée à {pct} %. Réessaie à ce moment-là.",
            "The autopilot is paused until {until}: the 5-hour window is {pct}% used. Try again then."
        ),
        PauseReason::Week => tr!(
            "Le pilote auto est en pause jusqu’à {until} : la fenêtre hebdomadaire est utilisée à {pct} %. Réessaie à ce moment-là.",
            "The autopilot is paused until {until}: the weekly window is {pct}% used. Try again then."
        ),
    }
}

/// A ticket, or a new agent, of the project does not start while every account it may go to is
/// held back (`Core::project_hold`: the one the project prefers, else any active one): one that is
/// free takes it, as the scheduler has it. The refusal says why, and until when.
fn not_held<R: Runtime>(core: &Core<R>, project_id: &str) -> Result<(), ToolError> {
    match core.project_hold(project_id) {
        Some(held) => Err(ToolError::Refused(why(core, &held))),
        None => Ok(()),
    }
}

/// A message to an agent does not go out while the agent's own account is held back
/// (`Core::account_hold`): it would spend that account's quota. An agent on another account is not
/// held by it. The refusal says why, until when, and which account when there are several.
fn account_not_held<R: Runtime>(
    core: &Core<R>,
    agent: &resolve::AgentRef,
) -> Result<(), ToolError> {
    let Some(account) = core
        .agent(&agent.id)
        .ok()
        .map(|h| h.lock().meta.account.clone())
    else {
        return Ok(());
    };
    let Some(held) = core.account_hold(&account) else {
        return Ok(());
    };
    let mut refusal = why(core, &held);
    let settings = core.settings.read().clone();
    if settings.accounts.len() > 1 {
        let (name, account) = (
            &agent.name,
            accounts::name_in(i18n::ui(), &accounts::get(&settings, &account)),
        );
        refusal.push(' ');
        refusal.push_str(&tr!(
            "C’est le compte « {account} » de l’agent {name} ; un agent d’un autre compte peut encore recevoir un message.",
            "The agent {name} is on the “{account}” account; an agent on another account can still take a message."
        ));
    }
    Err(ToolError::Refused(refusal))
}

// ---------- tickets ----------

/// `create_ticket`: a ticket « À faire » at the end of the project's column.
pub(super) async fn create_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: CreateTicketArgs,
) -> Result<String, ToolError> {
    let project = resolve::project(core, &a.project)?;
    own_project_only(core, caller, &project.id)?;
    let draft = TicketDraft {
        title: title(&a.title)?,
        description: described(core, caller, a.description.as_deref().unwrap_or_default())?,
        criteria: criteria(a.criteria.as_deref().unwrap_or_default())?,
        // The default number of loops, as the window's form starts with.
        max_loops: 0,
        after: after_ids(core, &project, a.after.as_deref().unwrap_or_default())?,
        // Its own branch: the tools do not name an existing one.
        branch: String::new(),
        // Last: files named by a path are read only once the rest is checked.
        images: match a.images {
            Some(list) => Some(images::drafts(core, caller, list, 0).await?),
            None => None,
        },
        // The tools do not join files (another ticket's).
        files: None,
    };
    let made = core
        .ticket_create(&project.id, draft)
        .await
        .map_err(failure)?;
    json(&ticket_view(core, &made))
}

/// `update_ticket`: the fields given replace the ticket's; the others stay. Its images are added to
/// (`addImages`) or taken from (`removeImages`), the ones not named kept.
pub(super) async fn update_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: UpdateTicketArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    // An empty list is none given.
    let (add, remove) = (
        a.add_images.unwrap_or_default(),
        a.remove_images.unwrap_or_default(),
    );
    if a.title.is_none()
        && a.description.is_none()
        && a.criteria.is_none()
        && a.after.is_none()
        && add.is_empty()
        && remove.is_empty()
    {
        return Err(ToolError::Failed(tr!(
            "Rien à modifier : donne au moins un des champs title, description, criteria, after, addImages ou removeImages.",
            "Nothing to change: give at least one of the fields title, description, criteria, after, addImages or removeImages."
        )));
    }
    let project = project_of(core, &t.project_id)?;
    // The words of the ticket changed (not its dependencies alone): the description ends with who
    // changed them, in the place of the line that was there (« Créé par… », or an earlier edit's).
    let edited = a.title.is_some() || a.description.is_some() || a.criteria.is_some();
    // A description given back as `get_ticket` answered it ends with that line too: it goes, and
    // the limit counts what is left.
    let written = match &a.description {
        Some(new) => description(without_origin(new))?,
        None if edited => without_origin(&t.description).to_string(),
        None => t.description.clone(),
    };
    let mut draft = TicketDraft {
        title: match &a.title {
            Some(new) => title(new)?,
            None => t.title.clone(),
        },
        description: if edited {
            let origin = activity::caller_name(core, Some(caller));
            with_origin(&written, &changed_by(i18n::ui(), &origin))
        } else {
            written
        },
        criteria: match &a.criteria {
            Some(new) => criteria(new)?,
            None => t.criteria.iter().map(|c| c.text.clone()).collect(),
        },
        max_loops: t.max_loops,
        after: match &a.after {
            Some(new) => after_ids(core, &project, new)?,
            None => t.after.clone(),
        },
        // Not the tools' to change: the ticket keeps the branch the window gave it.
        branch: t.branch.clone(),
        // Its images stay as they are unless a list is built below.
        images: None,
        // Nor its files: the tools do not join them.
        files: None,
    };
    if !add.is_empty() || !remove.is_empty() {
        // The core refuses a ticket that has started too, but only once the files are read: said
        // first, as the core says it, before a file or a reference is looked at.
        if t.column != Column::Todo {
            return Err(refusal(only_todo_is_edited()));
        }
        draft.images = Some(images::amended(core, caller, &t, add, &remove).await?);
    }
    // It writes and removes the images' files: off the async workers, as the window's command does.
    let (c, id) = (core.clone(), t.id.clone());
    let updated = tokio::task::spawn_blocking(move || c.ticket_update(&id, draft))
        .await
        .map_err(|e| failure(e.into()))?
        .map_err(refusal)?;
    json(&ticket_view(core, &updated))
}

/// `move_ticket`: the ticket's place in « À faire »; answers the column's keys in their order.
pub(super) fn move_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: MoveTicketArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    let before = a.before.as_deref().map(str::trim).filter(|b| !b.is_empty());
    let target;
    let to = match (a.position, before) {
        (PositionArg::Top, None) => MoveTo::Top,
        (PositionArg::Bottom, None) => MoveTo::Bottom,
        (PositionArg::Before, Some(asked)) => {
            let project = project_of(core, &t.project_id)?;
            target = resolve::ticket_in(core, asked, Some(&project))?.id;
            MoveTo::Before(&target)
        }
        (PositionArg::Before, None) => {
            return Err(ToolError::Failed(tr!(
                "Avec la position « before », donne aussi le ticket devant lequel placer celui-ci (argument before).",
                "With the position “before”, also give the ticket to put this one before (argument before)."
            )))
        }
        (_, Some(_)) => {
            return Err(ToolError::Failed(tr!(
                "L’argument before ne va qu’avec la position « before ».",
                "The argument before only goes with the position “before”."
            )))
        }
    };
    core.ticket_move(&t.id, to).map_err(refusal)?;
    json(&json!({ "ticket": t.key, "todo": todo_keys(core, &t.project_id) }))
}

/// `start_ticket`: the window's « Lancer ». Refused where the window would not offer it, or ask
/// the user first.
pub(super) fn start_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: StartTicketArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    // Before anything: a model or an effort that does not fit starts nothing, writes nothing.
    let model = model_setting(core, a.model.as_deref())?;
    let effort = effort_setting(i18n::ui(), a.effort.as_deref())?;
    // A ticket that left « À faire » is told so by the core.
    if t.column == Column::Todo {
        not_held(core, &t.project_id)?;
        let project = project_of(core, &t.project_id)?;
        let issue = core.board_issues.lock().get(&project.id).cloned();
        if let Some(issue) = issue {
            return Err(ToolError::Refused(tr!(
                "Le Kanban du projet {project} ne démarre rien pour l’instant : {issue}",
                "The project {project}’s board starts nothing for now: {issue}",
                project = project.name
            )));
        }
        let all = core.tickets.read().clone();
        let max = project.board.max_parallel.clamp(1, 6);
        let busy = all
            .iter()
            .filter(|x| {
                x.project_id == project.id && x.column == Column::Doing && x.blocked.is_none()
            })
            .count();
        if busy >= max as usize {
            return Err(ToolError::Refused(tr!(
                "Le maximum de tickets en parallèle du projet {project} est atteint ({max}, réglage « En parallèle » du Kanban) : réessaie quand l’un d’eux est fini.",
                "The most tickets in parallel for the project {project} is reached ({max}, the Kanban’s “In parallel” setting): try again when one is done.",
                project = project.name
            )));
        }
        let awaited: Vec<&str> = t
            .after
            .iter()
            .filter_map(|id| all.iter().find(|x| x.id == *id))
            .filter(|x| x.project_id == project.id && x.column != Column::Done)
            .map(|x| x.key.as_str())
            .collect();
        if !awaited.is_empty() {
            return Err(ToolError::Refused(tr!(
                "{key} vient après des tickets qui ne sont pas terminés : {awaited}. Le pilote auto le démarre quand ils le sont ; pour le lancer avant, retire ces dépendances avec update_ticket.",
                "{key} comes after tickets that are not done yet: {awaited}. The autopilot starts it once they are; to launch it sooner, take these dependencies off with update_ticket.",
                key = t.key,
                awaited = awaited.join(", ")
            )));
        }
    }
    core.ticket_start_with(&t.id, model, effort)
        .map_err(refusal)?;
    // What its agent goes with: its ticket's own, else its Kanban's, else the app's.
    let started = core.ticket(&t.id).unwrap_or(t);
    let board = project_of(core, &started.project_id)?.board;
    let (model, effort, _) = ticket_agent_options(&started, &board, &core.settings.read());
    let mut answer = json!({ "key": started.key, "starting": true, "model": model });
    // A model that takes no effort (Haiku) goes without one: none is said.
    if supports_effort(&model) {
        answer["effort"] = json!(effort);
    }
    json(&answer)
}

// ---------- the Kanban's settings ----------

/// The efforts Claude Code takes (`--effort`).
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// The modes a Kanban may be set to here: `bypassPermissions` is the user's to choose, in the
/// window (`mode_setting`).
const MODES: [&str; 4] = ["auto", "default", "plan", "acceptEdits"];
/// The most branches an error lists: a repository may have hundreds.
const BRANCHES_LISTED: usize = 20;

/// A model given for a Kanban's agents or a ticket's: `None` when left out, an empty one when it
/// goes back to the default, else one the app offers (`model`).
fn model_setting<R: Runtime>(
    core: &Core<R>,
    asked: Option<&str>,
) -> Result<Option<String>, ToolError> {
    asked
        .map(|m| model(core, Some(m)).map(Option::unwrap_or_default))
        .transpose()
}

/// An effort given for a Kanban's agents or a ticket's: `None` when left out, an empty one when it
/// goes back to the default, else one of `EFFORTS` (in any case).
fn effort_setting(lang: i18n::Lang, asked: Option<&str>) -> Result<Option<String>, ToolError> {
    let Some(asked) = asked.map(str::trim) else {
        return Ok(None);
    };
    if asked.is_empty() {
        return Ok(Some(String::new()));
    }
    match EFFORTS.iter().find(|e| e.eq_ignore_ascii_case(asked)) {
        Some(effort) => Ok(Some(effort.to_string())),
        None => Err(ToolError::Failed(tr_in!(
            lang,
            "Effort « {asked} » inconnu. Efforts : {choices}.",
            "Unknown effort “{asked}”. Efforts: {choices}.",
            choices = EFFORTS.join(", ")
        ))),
    }
}

/// A mode given for a Kanban's agents: `None` when left out, an empty one when it goes back to the
/// default, else one of `MODES`. `bypassPermissions` (no permission request at all) is the user's
/// decision, taken in the window: refused, unless `current`, the Kanban's own, already is it.
fn mode_setting(
    lang: i18n::Lang,
    current: &str,
    asked: Option<&str>,
) -> Result<Option<String>, ToolError> {
    let Some(asked) = asked.map(str::trim) else {
        return Ok(None);
    };
    if asked.is_empty() {
        return Ok(Some(String::new()));
    }
    if asked.eq_ignore_ascii_case("bypassPermissions") {
        if current == "bypassPermissions" {
            return Ok(Some(current.to_string()));
        }
        return Err(ToolError::Refused(tr_in!(
            lang,
            "Lancer des agents sans aucune demande de permission est une décision de l’utilisateur : le mode bypassPermissions se règle dans la fenêtre (Réglages › Kanban).",
            "Running agents with no permission request at all is the user’s decision: set the mode bypassPermissions in the window (Settings › Kanban)."
        )));
    }
    match MODES.iter().find(|m| m.eq_ignore_ascii_case(asked)) {
        Some(mode) => Ok(Some(mode.to_string())),
        None => Err(ToolError::Failed(tr_in!(
            lang,
            "Mode « {asked} » inconnu. Modes : {choices}.",
            "Unknown mode “{asked}”. Modes: {choices}.",
            choices = MODES.join(", ")
        ))),
    }
}

/// A target branch given for a Kanban: `None` when left out or when it leaves the Kanban's as it
/// is, else one of the project's local `branches`, trimmed. It leaves it as it is when it is the
/// Kanban's own (`kept`, even a branch deleted since, or none) or what `get_board` shows while the
/// Kanban has none (`current`, the branch the project is on): giving back what `get_board` said
/// is no change, nor a refusal.
fn target_setting(
    lang: i18n::Lang,
    kept: &str,
    current: &str,
    branches: &[String],
    asked: Option<&str>,
) -> Result<Option<String>, ToolError> {
    let Some(asked) = asked.map(str::trim) else {
        return Ok(None);
    };
    if asked == kept || (kept.is_empty() && asked == current) {
        return Ok(None);
    }
    if asked.is_empty() {
        return Err(ToolError::Failed(tr_in!(
            lang,
            "La branche cible ne peut pas être vide : donne une branche locale du projet, ou laisse target de côté pour garder l’actuelle.",
            "The target branch cannot be empty: give a local branch of the project, or leave target out to keep the current one."
        )));
    }
    if branches.iter().any(|b| b == asked) {
        return Ok(Some(asked.to_string()));
    }
    let listed = branches
        .iter()
        .take(BRANCHES_LISTED)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let choices = match branches.len() {
        n if n > BRANCHES_LISTED => {
            tr_in!(
                lang,
                "{listed}, … ({n} au total)",
                "{listed}, … ({n} in all)"
            )
        }
        _ => listed,
    };
    Err(ToolError::Failed(tr_in!(
        lang,
        "« {asked} » n’est pas une branche locale du projet. Branches : {choices}.",
        "“{asked}” is not a local branch of the project. Branches: {choices}."
    )))
}

/// The Kanban settings `a` asks for, over `board`: only the fields given change. Checked as the
/// window's form is (the model through `model_setting`, given as `model`: empty is the app's), and
/// what is the user's alone stays so (`mode_setting`, and tests first, which needs a test command
/// the server does not set). `branches` are the project's local ones and `current` the one it is on
/// (empty on a detached HEAD): git is read only for a target that is not the Kanban's (`set_board`).
/// Nothing is returned for a change when one field is refused.
pub(super) fn board_change(
    lang: i18n::Lang,
    board: &BoardSettings,
    branches: &[String],
    current: &str,
    a: &SetBoardArgs,
    model: Option<String>,
) -> Result<BoardSettings, ToolError> {
    let nothing = a.action.is_none()
        && a.target.is_none()
        && a.strategy.is_none()
        && a.draft.is_none()
        && a.cleanup.is_none()
        && a.conventional.is_none()
        && a.conflict.is_none()
        && a.max_parallel.is_none()
        && a.autopilot.is_none()
        && model.is_none()
        && a.effort.is_none()
        && a.mode.is_none()
        && a.tests_first.is_none();
    if nothing {
        return Err(ToolError::Failed(tr_in!(
            lang,
            "Donne au moins un réglage à changer : action, target, strategy, draft, cleanup, conventional, conflict, maxParallel, autopilot, model, effort, mode ou testsFirst.",
            "Give at least one setting to change: action, target, strategy, draft, cleanup, conventional, conflict, maxParallel, autopilot, model, effort, mode or testsFirst."
        )));
    }
    let effort = effort_setting(lang, a.effort.as_deref())?;
    let mode = mode_setting(lang, &board.mode, a.mode.as_deref())?;
    let target = target_setting(lang, &board.target, current, branches, a.target.as_deref())?;
    if let Some(n) = a.max_parallel.filter(|n| !(1..=6).contains(n)) {
        return Err(ToolError::Failed(tr_in!(
            lang,
            "Le maximum de tickets en parallèle va de 1 à 6 ({n} donné).",
            "The most tickets in parallel goes from 1 to 6 ({n} given)."
        )));
    }
    // Only turning it on needs a command: one already on (the window shows it off with no command,
    // as `get_board` does) is taken as it is.
    if a.tests_first == Some(true) && !board.tests_first && board.test_command.trim().is_empty() {
        return Err(ToolError::Refused(tr_in!(
            lang,
            "Le Kanban n’a pas de commande de test, et le serveur n’en lance aucune : règle-la dans la fenêtre (Réglages › Kanban), puis active « Relancer les tests avant ».",
            "The Kanban has no test command, and the server runs none: set it in the window (Settings › Kanban), then turn on “Re-run the tests first”."
        )));
    }
    let mut s = board.clone();
    if let Some(action) = a.action {
        s.action = action.name().into();
    }
    if let Some(target) = target {
        s.target = target;
    }
    if let Some(strategy) = a.strategy {
        s.strategy = strategy.name().into();
    }
    if let Some(conflict) = a.conflict {
        s.conflict = conflict.name().into();
    }
    s.draft = a.draft.unwrap_or(s.draft);
    s.cleanup = a.cleanup.unwrap_or(s.cleanup);
    s.conventional = a.conventional.unwrap_or(s.conventional);
    s.max_parallel = a.max_parallel.unwrap_or(s.max_parallel);
    s.autopilot = a.autopilot.unwrap_or(s.autopilot);
    // Asked as `get_board` shows it: left as it is.
    if let Some(on) = a.tests_first.filter(|on| *on != board.tests_first_on()) {
        s.tests_first = on;
    }
    if let Some(model) = model {
        s.model = model;
    }
    if let Some(effort) = effort {
        s.effort = effort;
    }
    if let Some(mode) = mode {
        s.mode = mode;
    }
    Ok(s)
}

/// What `after` changes of `before`, as the activity log says it: each setting `set_board` sets that
/// has changed, « name: before → after », in the order of the tool's arguments; else that nothing
/// did. A model, an effort or a mode left empty is the app's (« (défaut) »), a target left empty
/// is none. Tests first is the one `get_board` shows: on only with a test command to run.
pub(super) fn board_changes(
    lang: i18n::Lang,
    before: &BoardSettings,
    after: &BoardSettings,
) -> String {
    let default = tr_in!(lang, "(défaut)", "(default)");
    let none = tr_in!(lang, "(aucune)", "(none)");
    let said = |value: &str, empty: &str| match value {
        "" => empty.to_string(),
        value => value.to_string(),
    };
    let on = |b: &BoardSettings| b.tests_first_on().to_string();
    let rows = [
        ("action", before.action.clone(), after.action.clone()),
        (
            "target",
            said(&before.target, &none),
            said(&after.target, &none),
        ),
        ("strategy", before.strategy.clone(), after.strategy.clone()),
        ("draft", before.draft.to_string(), after.draft.to_string()),
        (
            "cleanup",
            before.cleanup.to_string(),
            after.cleanup.to_string(),
        ),
        (
            "conventional",
            before.conventional.to_string(),
            after.conventional.to_string(),
        ),
        ("conflict", before.conflict.clone(), after.conflict.clone()),
        (
            "maxParallel",
            before.max_parallel.to_string(),
            after.max_parallel.to_string(),
        ),
        (
            "autopilot",
            before.autopilot.to_string(),
            after.autopilot.to_string(),
        ),
        (
            "model",
            said(&before.model, &default),
            said(&after.model, &default),
        ),
        (
            "effort",
            said(&before.effort, &default),
            said(&after.effort, &default),
        ),
        (
            "mode",
            said(&before.mode, &default),
            said(&after.mode, &default),
        ),
        ("testsFirst", on(before), on(after)),
    ];
    let changed: Vec<String> = rows
        .into_iter()
        .filter(|(_, from, to)| from != to)
        .map(|(name, from, to)| format!("{name}: {from} → {to}"))
        .collect();
    if changed.is_empty() {
        return tr_in!(
            lang,
            "Aucun changement : le Kanban était déjà réglé ainsi.",
            "No change: the Kanban was already set so."
        );
    }
    changed.join(" · ")
}

/// `set_board`: the window's Settings › Kanban, through the same function of the core (the window
/// is told, the board saved and scheduled again). Answers the settings as `get_board` does, and
/// what changed, for the activity log (`board_changes`).
pub(super) async fn set_board<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: SetBoardArgs,
) -> Result<(String, String), ToolError> {
    let project = resolve::project(core, &a.project)?;
    own_project_only(core, caller, &project.id)?;
    let model = model_setting(core, a.model.as_deref())?;
    // Git is read only for a target that is not the Kanban's own: giving that one back needs no
    // repository at all.
    let kept = core.project(&project.id).map_err(failure)?.board.target;
    let (branches, current) = match a.target.as_deref().map(str::trim) {
        Some(asked) if asked != kept => (
            git::branches(&project.path).await.map_err(failure)?,
            git::head_branch(&project.path).await,
        ),
        _ => (Vec::new(), String::new()),
    };
    // The Kanban as it is now: what the window changed meanwhile is not written over.
    let board = core.project(&project.id).map_err(failure)?.board;
    let changed = board_change(i18n::ui(), &board, &branches, &current, &a, model)?;
    let saved = core.board_set(&project.id, changed).map_err(failure)?;
    let changes = board_changes(i18n::ui(), &board, &saved.board);
    Ok((json(&board_view(core, &saved).await)?, changes))
}

// ---------- the board's flow ----------

/// An agent does not act on its own ticket: the one it works on (its ticket names it, or its own
/// record names the ticket). `why` says it, in the words of the tool.
fn not_own_ticket<R: Runtime>(
    core: &Core<R>,
    caller: &Caller,
    t: &Ticket,
    why: String,
) -> Result<(), ToolError> {
    let Caller::Agent(id) = caller else {
        return Ok(());
    };
    let own = t.agent_id.as_deref() == Some(id.as_str())
        || core
            .agent(id)
            .is_ok_and(|h| h.lock().meta.ticket_id.as_deref() == Some(t.id.as_str()));
    if own {
        return Err(ToolError::Refused(why));
    }
    Ok(())
}

/// What an agent is told when it acts on its own ticket with a tool of the flow that is neither the
/// approval nor the sending back.
fn own_ticket_with_this_tool() -> String {
    tr!(
        "Un agent n’agit pas sur son propre ticket avec cet outil : c’est à l’utilisateur ou à un autre client de le faire.",
        "An agent does not act on its own ticket with this tool: the user or another client does."
    )
}

/// The ticket's agent, unless it is gone or archived: the one a message of the flow reaches.
fn live_agent_of<R: Runtime>(core: &Core<R>, t: &Ticket) -> Option<resolve::AgentRef> {
    let h = core.agent(t.agent_id.as_deref()?).ok()?;
    let rt = h.lock();
    (!rt.meta.archived).then(|| resolve::AgentRef::of(&rt.meta))
}

/// The agent is at work: the core refuses first, in the window's words, held account or not.
fn at_work<R: Runtime>(core: &Core<R>, agent: &resolve::AgentRef) -> bool {
    core.agent(&agent.id)
        .is_ok_and(|h| h.lock().meta.status.is_active())
}

/// The error of a call of the core that ends by sending the ticket's agent a message
/// (`Core::send_or_block`): the ticket moves first (sent back, unblocked, handed over), then is
/// blocked with the reason when the message fails. Once it differs from `before` in its column or
/// its block, the call is told as taken into account; else it is the refusal it looks like.
fn unsent<R: Runtime>(core: &Core<R>, before: &Ticket, e: anyhow::Error) -> ToolError {
    let moved = core
        .ticket(&before.id)
        .is_ok_and(|now| now.column != before.column || now.blocked != before.blocked);
    if !moved {
        return refusal(e);
    }
    // (The error's own full stop is the sentence's.)
    let (key, e) = (&before.key, format!("{e:#}"));
    let e = e.trim_end().trim_end_matches('.');
    ToolError::Failed(tr!(
        "{key} a été pris en compte, mais son agent n’a pas reçu le message : {e}. Le ticket est bloqué avec cette raison.",
        "{key} was taken into account, but its agent did not get the message: {e}. The ticket is blocked with that reason."
    ))
}

/// What an action of the flow answers: the ticket as `get_ticket` gives it, and its `since`.
#[derive(Serialize)]
struct Answered {
    #[serde(flatten)]
    ticket: TicketView,
    since: String,
}

/// The ticket as `get_ticket` answers it now, once the core did what was asked, and `since`: the
/// `wait_ticket` cursor of the ticket as it was before (`wait::since`). What the action sets off
/// may be over before the client waits for it (a quick approval): from this cursor the wait is told
/// of it all the same.
fn answer<R: Runtime>(core: &Core<R>, id: &str, since: String) -> Result<String, ToolError> {
    let ticket = ticket_view(core, &core.ticket(id).map_err(failure)?);
    json(&Answered { ticket, since })
}

/// `approve_ticket`: the window's « Valider… ». Answers once the approval has started: its tests,
/// commit, merge or push go on in the background, and are followed on the ticket.
pub(super) fn approve_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: ApproveTicketArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    // Its own work merged without a review: that is the user's call.
    not_own_ticket(
        core,
        caller,
        &t,
        tr!(
            "Un agent ne valide pas son propre ticket : il fusionnerait son travail sans relecture.",
            "An agent does not approve its own ticket: it would merge its work without a review."
        ),
    )?;
    let since = wait::since(core, &t);
    core.ticket_approve_detached(&t.id).map_err(refusal)?;
    // Counted in the usage statistics once it started, as the window's.
    core.telemetry.track(Event::TicketApproved);
    answer(core, &t.id, since)
}

/// `reject_ticket`: the window's « Renvoyer », the comment going to the ticket's agent followed by
/// who sent the ticket back.
pub(super) async fn reject_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: RejectTicketArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    not_own_ticket(
        core,
        caller,
        &t,
        tr!(
            "Un agent ne renvoie pas son propre ticket.",
            "An agent does not send back its own ticket."
        ),
    )?;
    let said = a.comment.trim().trim_end_matches('.').trim_end();
    if said.is_empty() {
        return Err(failure(comment_missing()));
    }
    // Where the window would go ahead, the comment reaches the agent: not while its account is held,
    // unless the agent works, which the core refuses first.
    if t.column == Column::Review && t.step.is_none() {
        if let Some(agent) = live_agent_of(core, &t).filter(|a| !at_work(core, a)) {
            account_not_held(core, &agent)?;
        }
    }
    // Under its author's name: the agent is not to take it for the user's own words.
    let origin = activity::caller_name(core, Some(caller));
    let comment = tr_in!(
        core.lang().claude,
        "{said} (renvoyé par {origin} via Escouade)",
        "{said} (sent back by {origin} through Escouade)"
    );
    let since = wait::since(core, &t);
    core.ticket_reject(&t.id, &comment)
        .await
        .map_err(|e| unsent(core, &t, e))?;
    answer(core, &t.id, since)
}

/// `resume_ticket`: the window's « Reprendre » on a blocked ticket « En cours ».
pub(super) async fn resume_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: ResumeTicketArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    not_own_ticket(core, caller, &t, own_ticket_with_this_tool())?;
    // Where the window would go ahead: a message to the ticket's agent, else a new agent. One that
    // waits for its quota is refused by the core, which says until when.
    if t.column == Column::Doing && t.blocked.is_some() {
        match live_agent_of(core, &t) {
            Some(agent) => {
                let waits = core
                    .agent(&agent.id)
                    .is_ok_and(|h| h.lock().meta.resume_at.is_some());
                if !waits {
                    account_not_held(core, &agent)?;
                }
            }
            None => not_held(core, &t.project_id)?,
        }
    }
    let since = wait::since(core, &t);
    core.ticket_resume(&t.id)
        .await
        .map_err(|e| unsent(core, &t, e))?;
    answer(core, &t.id, since)
}

/// `resolve_conflict`: the window's « L'agent résout » on a ticket « À tester » stopped on a
/// conflict.
pub(super) async fn resolve_conflict<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: ResolveConflictArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    not_own_ticket(core, caller, &t, own_ticket_with_this_tool())?;
    if t.column != Column::Review {
        return Err(refusal(Refusal::NotInReview.into()));
    }
    if t.step.is_some() {
        return Err(refusal(Refusal::ApprovingAlready.into()));
    }
    if !t.conflict {
        return Err(ToolError::Refused(tr!(
            "Ce ticket n’a pas de conflit à résoudre.",
            "This ticket has no conflict to resolve."
        )));
    }
    // Not while its account is held, unless the agent works, which the core refuses first.
    if let Some(agent) = live_agent_of(core, &t).filter(|a| !at_work(core, a)) {
        account_not_held(core, &agent)?;
    }
    let since = wait::since(core, &t);
    core.ticket_resolve_conflict(&t.id)
        .await
        .map_err(|e| unsent(core, &t, e))?;
    answer(core, &t.id, since)
}

/// `dismiss_block`: the window's « Annuler » on a blocked ticket « À tester ».
pub(super) fn dismiss_block<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: DismissBlockArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    not_own_ticket(core, caller, &t, own_ticket_with_this_tool())?;
    if t.column != Column::Review {
        return Err(refusal(Refusal::NotInReview.into()));
    }
    // The block of a step under way is not there yet, or is the last try's.
    if t.step.is_some() {
        return Err(refusal(Refusal::ApprovingAlready.into()));
    }
    if t.blocked.is_none() && !t.conflict {
        return Err(ToolError::Refused(tr!(
            "Ce ticket n’est pas bloqué.",
            "This ticket isn’t blocked."
        )));
    }
    let since = wait::since(core, &t);
    core.ticket_dismiss(&t.id).map_err(refusal)?;
    answer(core, &t.id, since)
}

/// `delete_ticket`: the window's « Supprimer », whatever the ticket's column.
pub(super) async fn delete_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: DeleteTicketArgs,
) -> Result<String, ToolError> {
    let t = resolve::ticket(core, &a.ticket)?;
    own_project_only(core, caller, &t.project_id)?;
    not_own_ticket(core, caller, &t, own_ticket_with_this_tool())?;
    // Its approval would go on without it: merging a ticket that is gone.
    if t.step.is_some() {
        return Err(ToolError::Refused(tr!(
            "La validation de ce ticket est en cours : attends sa fin pour le supprimer.",
            "This ticket is being approved: wait for the end of its approval to delete it."
        )));
    }
    // A ticket under way has its agent archived with it.
    let agent = matches!(t.column, Column::Doing | Column::Review)
        .then(|| live_agent_of(core, &t))
        .flatten();
    core.ticket_delete(&t.id).await.map_err(failure)?;
    // Said as it is: an agent that could not be archived is only logged by the core.
    let archived = agent.is_some_and(|a| core.agent(&a.id).is_ok_and(|h| h.lock().meta.archived));
    json(&json!({ "key": t.key, "deleted": true, "agentArchived": archived }))
}

// ---------- agents ----------

/// A model as Claude Code names it, one the window offers when it knows the list.
fn model<R: Runtime>(core: &Core<R>, asked: Option<&str>) -> Result<Option<String>, ToolError> {
    let Some(m) = asked.map(str::trim).filter(|m| !m.is_empty()) else {
        return Ok(None);
    };
    // It rides on Claude Code's command line: nothing a flag could be made of.
    let shaped = m.chars().count() <= 100
        && m.starts_with(|c: char| c.is_ascii_alphanumeric())
        && m.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-:[]/".contains(c));
    if !shaped {
        return Err(ToolError::Failed(tr!(
            "« {m} » n’est pas un nom de modèle.",
            "“{m}” is not a model name."
        )));
    }
    let known = core.models.read().clone();
    if known.is_empty() {
        return Ok(Some(m.to_string()));
    }
    if let Some(k) = known.iter().find(|k| k.value.eq_ignore_ascii_case(m)) {
        return Ok(Some(k.value.clone()));
    }
    if known
        .iter()
        .any(|k| k.resolved_model.eq_ignore_ascii_case(m))
    {
        return Ok(Some(m.to_string()));
    }
    let choices = known
        .iter()
        .map(|k| k.value.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(ToolError::Failed(tr!(
        "Modèle « {m} » inconnu. Modèles : {choices}.",
        "Unknown model “{m}”. Models: {choices}."
    )))
}

/// `create_agent`: a new agent of the project, and its first message. The window's selection is
/// the user's: the agent is not selected.
pub(super) async fn create_agent<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: CreateAgentArgs,
) -> Result<String, ToolError> {
    let project = resolve::project(core, &a.project)?;
    own_project_only(core, caller, &project.id)?;
    let message = a.message.trim();
    if message.is_empty() {
        return Err(ToolError::Failed(tr!(
            "Écris le message qui lance l’agent.",
            "Write the message that starts the agent."
        )));
    }
    let model = model(core, a.model.as_deref())?;
    not_held(core, &project.id)?;
    // One at a time from the count to the first message: two calls at once do not both find a
    // place free.
    let _one = core.mcp.acting.lock().await;
    let max = project.board.max_parallel.clamp(1, 6) as usize;
    let working = core
        .agents
        .read()
        .values()
        .filter(|h| {
            let rt = h.lock();
            rt.meta.project_id == project.id && !rt.meta.archived && rt.meta.status.is_active()
        })
        .count();
    if working >= max {
        return Err(ToolError::Refused(tr!(
            "Le maximum d’agents qui travaillent en même temps dans le projet {project} est atteint ({max}, réglage « En parallèle » du Kanban) : réessaie quand l’un d’eux a fini son tour.",
            "The most agents working at once in the project {project} is reached ({max}, the Kanban’s “In parallel” setting): try again when one has finished its turn.",
            project = project.name
        )));
    }
    let view = core
        .create_agent_with(
            &project.id,
            AgentOptions {
                model,
                isolated: a.worktree,
                // Its Claude account is the project's, else the first usable one: one of those
                // `not_held` found free.
                ..Default::default()
            },
        )
        .await
        .map_err(failure)?;
    let (id, name) = (view.meta.id, view.meta.name);
    // Under its author's name, as every message from the server: not taken for the user's words.
    let origin = activity::caller_name(core, Some(caller));
    if let Err(e) = core
        .send_message_from(&id, Some(&origin), message.to_string(), vec![])
        .await
    {
        return Err(ToolError::Failed(tr!(
            "L’agent {name} (id {id}) est créé, mais son message n’a pas pu être envoyé : {e:#}",
            "The agent {name} (id {id}) is created, but its message could not be sent: {e:#}"
        )));
    }
    json(&json!({ "id": id, "name": name }))
}

/// The agent `asked` names (in the project `project` when given), who is not the caller and,
/// for an agent calling, is of its project.
fn other_agent<R: Runtime>(
    core: &Core<R>,
    caller: &Caller,
    asked: &str,
    project: Option<&str>,
    itself: String,
) -> Result<resolve::AgentRef, ToolError> {
    let scope = project.map(|p| resolve::project(core, p)).transpose()?;
    let found = resolve::agent(core, asked, scope.as_ref())?;
    if matches!(caller, Caller::Agent(id) if *id == found.id) {
        return Err(ToolError::Refused(itself));
    }
    own_project_only(core, caller, &found.project_id)?;
    Ok(found)
}

/// `send_message`: a message to an agent, delivered under its author's name (« Message de
/// <author> : »), never as the user's own words.
pub(super) async fn send_message<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: SendMessageArgs,
) -> Result<String, ToolError> {
    let found = other_agent(
        core,
        caller,
        &a.agent,
        a.project.as_deref(),
        tr!(
            "Un agent ne s’envoie pas de message à lui-même.",
            "An agent does not send a message to itself."
        ),
    )?;
    if found.archived {
        let name = &found.name;
        return Err(ToolError::Refused(tr!(
            "L’agent {name} est archivé : il ne reçoit plus de message.",
            "The agent {name} is archived: it no longer takes messages."
        )));
    }
    let text = a.text.trim();
    if text.is_empty() {
        return Err(ToolError::Failed(tr!(
            "Écris le message à envoyer.",
            "Write the message to send."
        )));
    }
    account_not_held(core, &found)?;
    // A message sent to an agent at work waits for the end of its turn.
    let queued = core
        .agent(&found.id)
        .is_ok_and(|h| h.lock().meta.status.is_active());
    // Under its author's name: the agent and the conversation tell it from the user's words.
    let origin = activity::caller_name(core, Some(caller));
    core.send_message_from(&found.id, Some(&origin), text.to_string(), vec![])
        .await
        .map_err(failure)?;
    json(&json!({ "id": found.id, "name": found.name, "queued": queued }))
}

/// `stop_agent`: the agent's running turn is interrupted, as the window's stop button does.
pub(super) async fn stop_agent<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: StopAgentArgs,
) -> Result<String, ToolError> {
    let found = other_agent(
        core,
        caller,
        &a.agent,
        a.project.as_deref(),
        tr!(
            "Un agent ne peut pas arrêter son propre tour.",
            "An agent cannot stop its own turn."
        ),
    )?;
    let interrupted = core
        .agent(&found.id)
        .is_ok_and(|h| h.lock().meta.status.is_active());
    core.interrupt(&found.id).await.map_err(failure)?;
    json(&json!({ "id": found.id, "name": found.name, "interrupted": interrupted }))
}

// ---------- the requests of the agents ----------

/// The request `asked` of the agent `found`, still waiting for an answer. Refused when it no longer
/// waits (the user answered it in the window, or another client did, or the agent stopped), and
/// for an agent in the Claude Code view, whose requests are in its terminal.
fn waiting_request<R: Runtime>(
    core: &Core<R>,
    found: &resolve::AgentRef,
    asked: &str,
) -> Result<Waiting, ToolError> {
    let name = &found.name;
    let h = core.agent(&found.id).map_err(|_| {
        ToolError::Failed(tr!(
            "L’agent {name} n’existe plus.",
            "The agent {name} is gone."
        ))
    })?;
    let rt = h.lock();
    if rt.meta.terminal {
        return Err(ToolError::Refused(tr!(
            "L’agent {name} est en vue Claude Code : ses demandes sont dans son terminal, c’est là qu’on y répond.",
            "The agent {name} is in the Claude Code view: its requests are in its terminal, where they are answered."
        )));
    }
    let asked = asked.trim();
    let mut waiting = answerable(&rt);
    let Some(i) = waiting.iter().position(|w| w.view.id == asked) else {
        // The ones that wait are told: an id mistyped is no answer someone else gave.
        let ids = waiting
            .iter()
            .map(|w| w.view.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let ids = if ids.is_empty() {
            tr!("aucune", "none")
        } else {
            ids
        };
        return Err(ToolError::Refused(tr!(
            "Cette demande n’attend plus de réponse : l’utilisateur ou un autre client y a déjà répondu, ou l’agent ne tourne plus (ou ce n’est pas l’identifiant d’une de ses demandes). Demandes de l’agent {name} qui attendent : {ids}.",
            "This request no longer waits for an answer: the user or another client has already answered it, or the agent no longer runs (or it is not the id of one of its requests). Requests of the agent {name} that wait: {ids}."
        )));
    };
    Ok(waiting.swap_remove(i))
}

/// Why the core could not answer `request` of `found`: it no longer waits (the user, or another
/// client, answered it between the check and the answer), else what the core said.
pub(super) fn not_answered<R: Runtime>(
    core: &Core<R>,
    found: &resolve::AgentRef,
    request: &str,
    why: anyhow::Error,
) -> ToolError {
    match waiting_request(core, found, request) {
        Err(gone) => gone,
        Ok(_) => refusal(why),
    }
}

/// `answer_permission`: the window's « Autoriser » or « Refuser » for a permission, or the approval
/// of a plan. Never « Toujours autoriser ».
pub(super) fn answer_permission<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: AnswerPermissionArgs,
) -> Result<String, ToolError> {
    if a.decision == DecisionArg::Always {
        return Err(ToolError::Refused(tr!(
            "« Toujours autoriser » n’est pas offert par le serveur : une règle de permission qui dure reste la décision de l’utilisateur, à prendre dans la fenêtre d’Escouade. Réponds allow ou deny, pour cette demande seulement.",
            "“Always allow” is not offered by the server: a lasting permission rule stays the user’s decision, to make in Escouade’s window. Answer allow or deny, for this request only."
        )));
    }
    let found = other_agent(
        core,
        caller,
        &a.agent,
        a.project.as_deref(),
        tr!(
            "Un agent ne répond pas à ses propres demandes.",
            "An agent does not answer its own requests."
        ),
    )?;
    let waiting = waiting_request(core, &found, &a.request)?;
    let request = waiting.view.id;
    if waiting.view.kind != "permission" {
        return Err(ToolError::Failed(tr!(
            "La demande {request} est une question : réponds-y avec answer_question.",
            "The request {request} is a question: answer it with answer_question."
        )));
    }
    let message = a
        .message
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string);
    if a.decision == DecisionArg::Allow && message.is_some() {
        return Err(ToolError::Failed(tr!(
            "Un message ne va qu’avec un refus (decision deny) : l’agent n’en reçoit pas quand tu autorises.",
            "A message only goes with a refusal (decision deny): the agent gets none when you allow."
        )));
    }
    // Under its author's name: the card and, for a refusal, Claude are told who answered.
    let origin = activity::caller_name(core, Some(caller));
    let decision = a.decision.name();
    core.answer_permission_by(&found.id, &request, decision, message, Some(&origin))
        .map_err(|e| not_answered(core, &found, &request, e))?;
    json(&json!({
        "id": found.id, "name": found.name, "request": request, "decision": decision,
    }))
}

/// `answer_question`: what the window's question card sends, `{ <question>: <answer> }`, for the
/// answers given.
pub(super) fn answer_question<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: AnswerQuestionArgs,
) -> Result<String, ToolError> {
    let found = other_agent(
        core,
        caller,
        &a.agent,
        a.project.as_deref(),
        tr!(
            "Un agent ne répond pas à ses propres demandes.",
            "An agent does not answer its own requests."
        ),
    )?;
    let waiting = waiting_request(core, &found, &a.request)?;
    let request = waiting.view.id.clone();
    if request_type(&waiting.view) != "question" {
        return Err(ToolError::Failed(tr!(
            "La demande {request} est une demande de permission (ou un plan) : réponds-y avec answer_permission.",
            "The request {request} is a permission (or a plan): answer it with answer_permission."
        )));
    }
    let answers = question_answers(&waiting.input, &a.answers)?;
    let origin = activity::caller_name(core, Some(caller));
    core.answer_question_by(
        &found.id,
        &request,
        serde_json::Value::Object(answers.clone()),
        Some(&origin),
    )
    .map_err(|e| not_answered(core, &found, &request, e))?;
    json(&json!({
        "id": found.id, "name": found.name, "request": request, "answers": answers,
    }))
}

/// The answers of a question card as the window sends them (`{ <question>: <answer> }`, the
/// question as Claude Code asked it, the labels of several options joined by « , »): every question
/// answered once, by options it offers (several only where it allows them) or by a free text.
fn question_answers(
    input: &serde_json::Value,
    given: &[QuestionAnswer],
) -> Result<serde_json::Map<String, serde_json::Value>, ToolError> {
    let questions = asked_questions(input);
    let n = questions.len();
    let all = questions
        .iter()
        .map(|q| format!("« {} »", q.question))
        .collect::<Vec<_>>()
        .join(", ");
    if n == 0 {
        return Err(ToolError::Failed(tr!(
            "Cette demande ne pose aucune question.",
            "This request asks no question."
        )));
    }
    if given.is_empty() {
        return Err(ToolError::Failed(tr!(
            "Donne une réponse à chaque question de la demande : elle en pose {n}. Questions : {all}",
            "Give an answer to each question of the request: it asks {n}. Questions: {all}"
        )));
    }
    let mut picked: Vec<Option<String>> = vec![None; n];
    for answer in given {
        let named = answer
            .question
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty());
        let i = match named {
            None if n == 1 => 0,
            None => {
                return Err(ToolError::Failed(tr!(
                    "La demande pose {n} questions : donne la question de chaque réponse (argument question). Questions : {all}",
                    "The request asks {n} questions: give the question each answer is for (argument question). Questions: {all}"
                )))
            }
            Some(text) => {
                let lower = text.to_lowercase();
                questions
                    .iter()
                    .position(|q| q.question.trim() == text)
                    .or_else(|| {
                        questions
                            .iter()
                            .position(|q| q.question.trim().to_lowercase() == lower)
                    })
                    .ok_or_else(|| {
                        ToolError::Failed(tr!(
                            "Aucune question « {text} » dans la demande. Questions : {all}",
                            "No question “{text}” in the request. Questions: {all}"
                        ))
                    })?
            }
        };
        let q = &questions[i];
        let question = &q.question;
        if picked[i].is_some() {
            return Err(ToolError::Failed(tr!(
                "Deux réponses à la question « {question} » : une seule suffit.",
                "Two answers to the question “{question}”: one is enough."
            )));
        }
        let text = answer
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let choices = answer.choices.as_deref().filter(|c| !c.is_empty());
        picked[i] = Some(match (choices, text) {
            (None, Some(text)) => text.to_string(),
            (Some(choices), None) => {
                // The options named, as the question gave them, each once.
                let mut labels: Vec<&str> = Vec::new();
                for c in choices {
                    let c = c.trim();
                    let lower = c.to_lowercase();
                    let option = q.options.iter().find(|o| o.label.trim() == c).or_else(|| {
                        q.options
                            .iter()
                            .find(|o| o.label.trim().to_lowercase() == lower)
                    });
                    let Some(option) = option else {
                        let options = q
                            .options
                            .iter()
                            .map(|o| o.label.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Err(ToolError::Failed(tr!(
                            "« {c} » n’est pas une option de la question « {question} ». Options : {options}. Pour une réponse libre, donne text.",
                            "“{c}” is not an option of the question “{question}”. Options: {options}. For a free answer, give text."
                        )));
                    };
                    if !labels.contains(&option.label.as_str()) {
                        labels.push(&option.label);
                    }
                }
                if labels.len() > 1 && !q.multi_select {
                    let count = labels.len();
                    return Err(ToolError::Failed(tr!(
                        "La question « {question} » n’accepte qu’un choix ({count} donnés).",
                        "The question “{question}” takes one choice only ({count} given)."
                    )));
                }
                labels.join(", ")
            }
            _ => {
                return Err(ToolError::Failed(tr!(
                    "La réponse à la question « {question} » doit donner choices ou text, un seul des deux.",
                    "The answer to the question “{question}” must give choices or text, one of the two."
                )))
            }
        });
    }
    let mut answers = serde_json::Map::new();
    for (q, answer) in questions.iter().zip(picked) {
        let Some(answer) = answer else {
            let question = &q.question;
            return Err(ToolError::Failed(tr!(
                "La question « {question} » n’a pas de réponse : chaque question en veut une.",
                "The question “{question}” has no answer: each question needs one."
            )));
        };
        answers.insert(q.question.clone(), serde_json::Value::String(answer));
    }
    Ok(answers)
}

/// What the activity log says `answer_question` was asked: each answer, « question → answer » (the
/// answer alone when the question is not named), one after the other.
pub(super) fn answers_summary(answers: &[QuestionAnswer]) -> String {
    answers
        .iter()
        .map(|a| {
            let said = match (&a.choices, &a.text) {
                (Some(choices), _) if !choices.is_empty() => choices.join(", "),
                (_, Some(text)) => text.trim().to_string(),
                _ => String::new(),
            };
            match a
                .question
                .as_deref()
                .map(str::trim)
                .filter(|q| !q.is_empty())
            {
                Some(question) => format!("{question} → {said}"),
                None => said,
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

// ---------- for the agents of Escouade ----------

/// `report_progress`: the status line on the agent's card and its ticket's, until its next call.
pub(super) fn report_progress<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: ReportProgressArgs,
) -> Result<String, ToolError> {
    let id = agent_only(caller)?;
    // As the window would show it, nothing hidden in it: a direction override could reverse how
    // a line reads on the cards, a zero-width mark hide part of it.
    let line = visible_line(&a.line);
    let n = line.chars().count();
    if n > PROGRESS_LINE_MAX {
        return Err(ToolError::Failed(tr!(
            "La ligne d’état fait {n} caractères : {PROGRESS_LINE_MAX} au plus. Raccourcis-la.",
            "The status line is {n} characters long: {PROGRESS_LINE_MAX} at most. Shorten it."
        )));
    }
    let h = core.agent(id).map_err(failure)?;
    let line = (!line.is_empty()).then_some(line);
    h.lock().meta.progress_line = line.clone();
    core.emit_agent(&h);
    core.request_save();
    json(&json!({ "line": line }))
}

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

/// One of `split_ticket`'s tickets, checked: its draft.
fn sub_draft<R: Runtime>(
    core: &Core<R>,
    caller: &Caller,
    sub: &SubTicket,
    after: Vec<String>,
) -> Result<TicketDraft, ToolError> {
    Ok(TicketDraft {
        title: title(&sub.title)?,
        description: described(core, caller, sub.description.as_deref().unwrap_or_default())?,
        criteria: criteria(sub.criteria.as_deref().unwrap_or_default())?,
        max_loops: 0,
        after,
        branch: String::new(),
        images: None,
        files: None,
    })
}

/// `e` as the `n`th ticket of `split_ticket`'s list.
fn of_list(n: usize, e: ToolError) -> ToolError {
    match e {
        ToolError::Failed(why) | ToolError::Refused(why) => ToolError::Failed(tr!(
            "Ticket {n} de la liste : {why}",
            "Ticket {n} of the list: {why}"
        )),
    }
}

/// `split_ticket`: tickets « À faire » after the calling agent's own, one after the other (`chain`,
/// the default) or all after it alone; answers their keys. Each gets copies of the images of the
/// agent's ticket it asked for, all of them by default: by default an image whose file is gone is
/// left out and said (the answer is then the keys with a note for each), while one that is named
/// and cannot be copied fails the call.
pub(super) async fn split_ticket<R: Runtime>(
    core: &Arc<Core<R>>,
    caller: &Caller,
    a: SplitTicketArgs,
) -> Result<String, ToolError> {
    let agent_id = agent_only(caller)?;
    let (name, ticket_id) = {
        let h = core.agent(agent_id).map_err(failure)?;
        let rt = h.lock();
        (rt.meta.name.clone(), rt.meta.ticket_id.clone())
    };
    let own = ticket_id
        .and_then(|id| core.ticket(&id).ok())
        .ok_or_else(|| {
            ToolError::Refused(tr!(
                "L’agent {name} n’a pas de ticket : il n’a rien à découper.",
                "The agent {name} has no ticket: it has nothing to split."
            ))
        })?;
    own_project_only(core, caller, &own.project_id)?;
    let count = a.tickets.len();
    if count == 0 {
        return Err(ToolError::Failed(tr!(
            "Donne au moins un ticket à créer.",
            "Give at least one ticket to make."
        )));
    }
    if count > SPLIT_MAX {
        return Err(ToolError::Failed(tr!(
            "{SPLIT_MAX} tickets au plus par appel ({count} donnés).",
            "{SPLIT_MAX} tickets at most per call ({count} given)."
        )));
    }
    // All checked before any is made: their words, the images of the ticket they take, and those
    // images' files, which are read once.
    let mut drafts = Vec::with_capacity(count);
    let mut picks = Vec::with_capacity(count);
    for (i, sub) in a.tickets.iter().enumerate() {
        let n = i + 1;
        drafts.push(sub_draft(core, caller, sub, Vec::new()).map_err(|e| of_list(n, e))?);
        picks.push(images::chosen(&own.images, sub.images.as_deref()).map_err(|e| of_list(n, e))?);
    }
    let handed = images::hand_down(core, &own, &picks)
        .await
        .map_err(|(i, e)| of_list(i + 1, e))?;
    let chain = a.chain.unwrap_or(true);
    let mut previous = own.id.clone();
    let mut made: Vec<String> = Vec::with_capacity(count);
    // The images taken by default whose files are gone, said one by one.
    let mut skipped: Vec<String> = Vec::new();
    for (mut draft, pick) in drafts.into_iter().zip(&picks) {
        draft.after = vec![if chain {
            previous.clone()
        } else {
            own.id.clone()
        }];
        // Copies of its own: the core writes files in its folder.
        draft.images = Some(handed.copies(pick)).filter(|copies| !copies.is_empty());
        match core.ticket_create(&own.project_id, draft).await {
            Ok(t) => {
                skipped.extend(handed.left_out(&own, pick, &t.key));
                previous = t.id;
                made.push(t.key);
            }
            Err(e) => {
                let done = made.join(", ");
                return Err(ToolError::Failed(tr!(
                    "Tickets créés avant l’échec : {done}. Les suivants ne le sont pas : {e:#}",
                    "Tickets made before the failure: {done}. The next ones are not: {e:#}"
                )));
            }
        }
    }
    // The keys alone when every image went; what was left out is said beside them otherwise.
    if skipped.is_empty() {
        json(&made)
    } else {
        json(&json!({ "tickets": made, "skipped": skipped }))
    }
}
