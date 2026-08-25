//! Submission: partitioning a draft and posting it, retry-safely.
//!
//! The load-bearing invariant, inherited from leanreview: the outcome
//! records exactly which local drafts the host accepted, **even when a
//! later step fails**. The caller removes those from the draft and saves,
//! so a retry after a partial failure sends only what is still pending —
//! never a duplicate.

use prchum_core::diff::Side;
use prchum_core::review::{DraftReview, DraftState, ReviewEvent};

use crate::{Forge, PullRequestRef, ReviewComment};

/// What a submission would send, and what it deliberately skips.
pub struct SubmissionPlan {
    /// (local id, wire comment) — the atomic review's line comments.
    pub review: Vec<(String, ReviewComment)>,
    /// (local id, host root comment id, body) — posted individually.
    pub replies: Vec<(String, i64, String)>,
    /// (local id, body) — conversation comments, posted individually last.
    pub generals: Vec<(String, String)>,
    pub skipped_dismissed: usize,
    pub skipped_orphaned: usize,
}

impl SubmissionPlan {
    pub fn is_empty(&self) -> bool {
        self.review.is_empty() && self.replies.is_empty() && self.generals.is_empty()
    }
}

/// Partitions the draft: dismissed and orphaned are skipped (and kept),
/// replies to host threads post individually, the rest ride the review.
pub fn plan(draft: &DraftReview) -> SubmissionPlan {
    let mut plan = SubmissionPlan {
        review: Vec::new(),
        replies: Vec::new(),
        generals: Vec::new(),
        skipped_dismissed: 0,
        skipped_orphaned: 0,
    };
    for comment in &draft.comments {
        match comment.state {
            DraftState::Dismissed => {
                plan.skipped_dismissed += 1;
                continue;
            }
            DraftState::Orphaned => {
                plan.skipped_orphaned += 1;
                continue;
            }
            DraftState::Active | DraftState::Stale => {}
        }
        if let Some(parent) = comment.reply_to {
            plan.replies
                .push((comment.local_id.clone(), parent, comment.body.clone()));
            continue;
        }
        let location = &comment.location;
        let side = match location.side {
            Side::Left => "LEFT",
            Side::Right => "RIGHT",
        };
        let multi_line = location.end_line > location.start_line;
        plan.review.push((
            comment.local_id.clone(),
            ReviewComment {
                path: location.path.clone(),
                body: comment.body.clone(),
                // GitHub anchors a range on its end line.
                line: location.end_line,
                side: side.to_string(),
                start_line: multi_line.then_some(location.start_line),
                start_side: multi_line.then(|| side.to_string()),
            },
        ));
    }
    for general in &draft.general {
        plan.generals
            .push((general.local_id.clone(), general.body.clone()));
    }
    plan
}

/// The result of executing a plan. `accepted` lists local ids the host
/// took; `error`, when set, describes what failed *after* those.
pub struct SubmitOutcome {
    pub accepted: Vec<String>,
    pub error: Option<String>,
}

pub fn event_name(event: ReviewEvent) -> &'static str {
    match event {
        ReviewEvent::Comment => "COMMENT",
        ReviewEvent::Approve => "APPROVE",
        ReviewEvent::RequestChanges => "REQUEST_CHANGES",
    }
}

/// Executes the plan in order: the atomic review, then each reply, then
/// each general comment. Stops at the first failure, reporting everything
/// accepted so far.
pub fn execute(
    forge: &dyn Forge,
    pr: &PullRequestRef,
    draft: &DraftReview,
    plan: &SubmissionPlan,
) -> SubmitOutcome {
    let mut accepted = Vec::new();

    let needs_review =
        !plan.review.is_empty() || !draft.summary.is_empty() || draft.event != ReviewEvent::Comment;
    if needs_review {
        let comments: Vec<ReviewComment> = plan.review.iter().map(|(_, c)| c.clone()).collect();
        if let Err(error) = forge.create_review(
            pr,
            event_name(draft.event),
            &draft.summary,
            &comments,
        ) {
            return SubmitOutcome {
                accepted,
                error: Some(format!("could not submit the review: {error}")),
            };
        }
        accepted.extend(plan.review.iter().map(|(id, _)| id.clone()));
    }

    for (local_id, parent, body) in &plan.replies {
        if let Err(error) = forge.reply(pr, *parent, body) {
            return SubmitOutcome {
                accepted,
                error: Some(format!(
                    "posted the review but a reply failed (it is kept as a draft): {error}"
                )),
            };
        }
        accepted.push(local_id.clone());
    }

    for (local_id, body) in &plan.generals {
        if let Err(error) = forge.add_general_comment(pr, body) {
            return SubmitOutcome {
                accepted,
                error: Some(format!(
                    "a conversation comment failed (it is kept as a draft): {error}"
                )),
            };
        }
        accepted.push(local_id.clone());
    }

    SubmitOutcome {
        accepted,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prchum_core::diff::parse;
    use prchum_core::location::build_location;
    use prchum_core::review::DraftReview;
    use std::sync::Mutex;

    fn draft_with(states: &[DraftState]) -> DraftReview {
        let files = parse("--- a/x\n+++ b/x\n@@ -1,2 +1,3 @@\n c\n-a\n+b\n+d\n", 4).unwrap();
        let mut draft = DraftReview::default();
        for (index, state) in states.iter().enumerate() {
            let line = 2 + (index as u32 % 2);
            let location = build_location(&files[0], Side::Right, line, line).unwrap();
            let id = draft.add_comment(location, "code".into(), format!("note {index}"), "me");
            draft.comment_mut(&id).unwrap().state = *state;
        }
        draft
    }

    #[test]
    fn partition_rules() {
        let mut draft = draft_with(&[
            DraftState::Active,
            DraftState::Dismissed,
            DraftState::Orphaned,
            DraftState::Active,
        ]);
        // Turn the last one into a thread reply.
        let id = draft.comments[3].local_id.clone();
        draft.comment_mut(&id).unwrap().reply_to = Some(42);
        draft.add_general("overall".into());

        let plan = plan(&draft);
        assert_eq!(plan.review.len(), 1);
        assert_eq!(plan.replies.len(), 1);
        assert_eq!(plan.replies[0].1, 42);
        assert_eq!(plan.generals.len(), 1);
        assert_eq!(plan.skipped_dismissed, 1);
        assert_eq!(plan.skipped_orphaned, 1);
    }

    #[test]
    fn multi_line_anchors_on_the_end() {
        let files = parse("--- a/x\n+++ b/x\n@@ -1,2 +1,3 @@\n c\n-a\n+b\n+d\n", 4).unwrap();
        let mut draft = DraftReview::default();
        let location = build_location(&files[0], Side::Right, 2, 3).unwrap();
        draft.add_comment(location, "code".into(), "note".into(), "me");
        let plan = plan(&draft);
        assert_eq!(plan.review[0].1.line, 3);
        assert_eq!(plan.review[0].1.start_line, Some(2));
    }

    /// A forge that accepts `ok` calls, then fails.
    struct FlakyForge {
        remaining_ok: Mutex<usize>,
    }

    impl FlakyForge {
        fn take(&self) -> Result<(), String> {
            let mut remaining = self.remaining_ok.lock().unwrap();
            if *remaining == 0 {
                return Err("boom".to_string());
            }
            *remaining -= 1;
            Ok(())
        }
    }

    impl Forge for FlakyForge {
        fn pull_request(&self, _: &PullRequestRef) -> Result<crate::PullRequest, String> {
            unreachable!()
        }
        fn diff(&self, _: &PullRequestRef) -> Result<String, String> {
            unreachable!()
        }
        fn threads(&self, _: &PullRequestRef) -> Result<Vec<crate::ThreadInfo>, String> {
            unreachable!()
        }
        fn general_comments(&self, _: &PullRequestRef) -> Result<Vec<crate::Comment>, String> {
            unreachable!()
        }
        fn create_review(
            &self,
            _: &PullRequestRef,
            _: &str,
            _: &str,
            _: &[ReviewComment],
        ) -> Result<(), String> {
            self.take()
        }
        fn reply(&self, _: &PullRequestRef, _: i64, _: &str) -> Result<(), String> {
            self.take()
        }
        fn add_general_comment(&self, _: &PullRequestRef, _: &str) -> Result<(), String> {
            self.take()
        }
    }

    #[test]
    fn partial_failure_reports_what_was_accepted() {
        let mut draft = draft_with(&[DraftState::Active, DraftState::Active]);
        let id = draft.comments[1].local_id.clone();
        draft.comment_mut(&id).unwrap().reply_to = Some(9);
        draft.add_general("g".into());

        // Review succeeds, the reply fails: the review's comments are
        // accepted, the reply and the general are not.
        let forge = FlakyForge {
            remaining_ok: Mutex::new(1),
        };
        let submission = plan(&draft);
        let outcome = execute(&forge, &PullRequestRef::default(), &draft, &submission);
        assert_eq!(outcome.accepted.len(), 1);
        assert_eq!(outcome.accepted[0], draft.comments[0].local_id);
        assert!(outcome.error.unwrap().contains("reply failed"));

        // Retry with the pending remainder only: no duplicates possible.
        let mut retry_draft = draft.clone();
        retry_draft.comments.retain(|c| c.local_id != outcome.accepted[0]);
        let retry_plan = plan(&retry_draft);
        assert!(retry_plan.review.is_empty());
        assert_eq!(retry_plan.replies.len(), 1);
    }

    #[test]
    fn empty_plan_with_a_summary_still_reviews() {
        let mut draft = DraftReview::default();
        draft.summary = "just words".into();
        let forge = FlakyForge {
            remaining_ok: Mutex::new(1),
        };
        let submission = plan(&draft);
        assert!(submission.is_empty());
        let outcome = execute(&forge, &PullRequestRef::default(), &draft, &submission);
        assert!(outcome.error.is_none());
    }
}
