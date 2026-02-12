use anyhow;

use crate::{Submission, parse_stupid_date};
use chrono::NaiveDateTime;

#[derive(Clone, Debug)]
pub struct Score {
    pub base: f32,
    pub total: f32,
    pub submitted: NaiveDateTime,
    pub items: Vec<ScoreItem>,
}

#[derive(Clone, Debug)]
pub enum ScoreItem {
    Deduction(f32, String),
    Alert(String),
    Comment(String),
}

impl ScoreItem {
    pub fn alert(s: impl std::fmt::Display) -> ScoreItem {
        ScoreItem::Alert(s.to_string())
    }
}

pub fn score(submission: &Submission, late_deadline: &str, final_deadline: &str) -> anyhow::Result<Score> {
    let submitted = parse_stupid_date(&submission.report.date)?;
    let base_score = submission.report.tests_score();

    let mut items = Vec::new();
    let mut final_score = base_score;

    for (penalty, reason) in &submission.report.manual_deductions {
        items.push(ScoreItem::Deduction(*penalty, reason.clone()));
        final_score -= penalty;
    }

    match submission.report.readme_approved {
        None => items.push(ScoreItem::alert("README not checked")),
        Some(false) => {
            let penalty = submission.report.readme_penalty.unwrap_or(1.);
            final_score -= penalty;
            items.push(ScoreItem::Deduction(penalty, "README missing or filled out incorrectly".to_string()));
        },
        Some(true) => {
            if submission.report.readme_penalty.is_some() {
                items.push(ScoreItem::alert("README is approved, but a penalty was given."));
            }
        },
    }

    match submission.report.code_approved {
        None => items.push(ScoreItem::alert("Style requirements not checked")),
        Some(false) => {
            final_score -= 1.;
            items.push(ScoreItem::Deduction(1., "Not meeting style requirements".to_string()));
        },
        Some(true) => (),
    }

    match deadline_status(submission, late_deadline, final_deadline) {
        Ok(DeadlineStatus::Early(hours, minutes)) => {
            items.push(ScoreItem::Comment(format!("Early by {hours}h {minutes}m")));
        },
        Ok(DeadlineStatus::Late(hours, minutes)) => {
            final_score -= 2.;
            items.push(ScoreItem::Deduction(2., format!("Submitted {hours}h {minutes}m after deadline")));
        },
        Ok(DeadlineStatus::Missed(hours, minutes)) => {
            final_score -= 10.;
            items.push(ScoreItem::Deduction(10., format!("Submitted {hours}h {minutes}m after final deadline")));
        },
        Err(e) => {
            items.push(ScoreItem::Comment(format!("Error parsing date: {e:?}")));
        },
    }

    if !submission.compile_log.trim().is_empty() {
        final_score -= 2.;
        items.push(ScoreItem::Deduction(2., "One or more compiler warnings".to_string()));
    }

    Ok(Score {
        base: base_score,
        total: final_score,
        items,
        submitted
    })
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum DeadlineStatus {
    Early(i64, i64), // hours, minutes
    Late(i64, i64), // hours, minutes
    Missed(i64, i64), // hours, minutes
}

fn deadline_status(submission: &Submission, late_deadline: &str, final_deadline: &str) -> anyhow::Result<DeadlineStatus> {
    let late_deadline = NaiveDateTime::parse_from_str(late_deadline, "%Y-%m-%dT%H:%M")?;
    let final_deadline = NaiveDateTime::parse_from_str(final_deadline, "%Y-%m-%dT%H:%M")?;
    let submitted = parse_stupid_date(&submission.report.date)?;

    let past_late_deadline = submitted.signed_duration_since(late_deadline);
    let past_final_deadline = submitted.signed_duration_since(final_deadline);

    if past_final_deadline.num_seconds() > 0 {
        Ok(DeadlineStatus::Missed(past_final_deadline.num_hours(), past_final_deadline.num_minutes() % 60))
    } else if past_late_deadline.num_seconds() > 0 {
        Ok(DeadlineStatus::Late(past_late_deadline.num_hours(), past_late_deadline.num_minutes() % 60))
    } else {
        Ok(DeadlineStatus::Early(-past_late_deadline.num_hours(), -past_late_deadline.num_minutes() % 60))
    }
}
