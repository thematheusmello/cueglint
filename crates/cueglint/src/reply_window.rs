//! Bounded long-conversation navigation; variable-height replies keep their native scroll area.
use cueglint_core::{RequestStatus, Session};
use std::ops::Range;

// shortcut: page at 16 to bound variable-height layout; replace with measured virtualization for continuous long-history parity.
pub const LIMIT: usize = 16;

#[derive(Clone, Copy)]
pub enum Navigation {
    Earlier,
    Later,
    Latest,
}

pub struct ReplyIndex {
    pub request: usize,
    pub answer: Option<usize>,
}

#[derive(Default)]
pub struct ReplyWindow {
    session: Option<String>,
    anchor: Option<String>,
    range: Range<usize>,
    replies: Vec<ReplyIndex>,
}

impl ReplyWindow {
    pub fn synchronize(&mut self, session: Option<&Session>) {
        let Some(session) = session else {
            *self = Self::default();
            return;
        };
        if self.session.as_deref() != Some(session.id.as_str()) {
            self.session = Some(session.id.clone());
            self.anchor = None;
        }
        if session.requests.len() <= LIMIT {
            self.anchor = None;
        }
        let end = self
            .anchor
            .as_ref()
            .and_then(|anchor| {
                session
                    .requests
                    .iter()
                    .position(|request| &request.id == anchor)
            })
            .map(|index| index + 1);
        if end.is_none() {
            self.anchor = None;
        }
        let end = end.unwrap_or(session.requests.len());
        self.range = end.saturating_sub(LIMIT)..end;
        self.replies.clear();
        for index in self.range.clone() {
            let request = &session.requests[index];
            let answer = session.suggestions.iter().position(|answer| {
                request.status == RequestStatus::Completed
                    && answer.request_id == request.id
                    && answer.context_revision == request.context.revision
            });
            self.replies.push(ReplyIndex {
                request: index,
                answer,
            });
        }
    }

    pub fn navigate(&mut self, session: &Session, navigation: Navigation) {
        self.synchronize(Some(session));
        let end = match navigation {
            Navigation::Earlier if self.range.start > 0 => self.range.start,
            Navigation::Later => (self.range.end + LIMIT).min(session.requests.len()),
            Navigation::Latest => session.requests.len(),
            Navigation::Earlier => return,
        };
        self.anchor = if end == session.requests.len() {
            None
        } else {
            Some(session.requests[end - 1].id.clone())
        };
        self.synchronize(Some(session));
    }

    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    pub fn replies(&self) -> &[ReplyIndex] {
        &self.replies
    }
    pub fn latest(&self) -> bool {
        self.anchor.is_none()
    }
    pub fn anchor(&self) -> Option<&str> {
        self.anchor.as_deref()
    }
}

#[cfg(test)]
pub fn history(count: usize) -> Session {
    let mut store = cueglint_core::Store::open_in_memory().unwrap();
    let session = store
        .start_session("[Test] Original history", None)
        .unwrap();
    let request = store
        .begin_request(&session.id, "Original question")
        .unwrap();
    store
        .complete_request(&session.id, &request.id, "Original answer")
        .unwrap();
    let mut session = store.session(&session.id).unwrap();
    let request = session.requests[0].clone();
    let answer = session.suggestions[0].clone();
    session.requests = (0..count)
        .map(|index| {
            let mut request = request.clone();
            request.id = format!("original-question-{index}");
            request.prompt = format!("Original history question {index:03}");
            request.context.revision = index as u64 + 1;
            request
        })
        .collect();
    session.suggestions = (0..count)
        .map(|index| {
            let mut answer = answer.clone();
            answer.id = format!("original-answer-{index}");
            answer.request_id = session.requests[index].id.clone();
            answer.context_revision = session.requests[index].context.revision;
            answer.text =
                format!("Original history response {index:03}: **preserve this exact source**.");
            answer
        })
        .collect();
    session.context_revision = count as u64;
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_answer_in_a_two_hundred_reply_history_is_reachable_with_bounded_windows() {
        let session = history(200);
        let mut window = ReplyWindow::default();
        window.synchronize(Some(&session));
        assert_eq!(window.range(), 184..200);
        let mut seen = std::collections::HashSet::new();
        loop {
            assert!(window.replies().len() <= LIMIT);
            for reply in window.replies() {
                assert!(seen.insert(reply.request));
                assert_eq!(reply.answer, Some(reply.request));
            }
            if window.range().start == 0 {
                break;
            }
            window.navigate(&session, Navigation::Earlier);
        }
        assert_eq!(seen.len(), 200);
        assert_eq!(window.range(), 0..8);
        let mut shortened = session.clone();
        shortened.requests.truncate(10);
        shortened.suggestions.truncate(10);
        let mut same_id = ReplyWindow::default();
        same_id.synchronize(Some(&session));
        while same_id.range().start > 0 {
            same_id.navigate(&session, Navigation::Earlier);
        }
        same_id.synchronize(Some(&shortened));
        assert_eq!(same_id.range(), 0..10);
        assert!(same_id.latest());
        while !window.latest() {
            window.navigate(&session, Navigation::Later);
        }
        assert_eq!(window.range(), 184..200);
        for count in [0, 1, LIMIT] {
            let session = history(count);
            window.synchronize(Some(&session));
            assert_eq!(window.range(), 0..count);
        }
    }

    #[test]
    fn request_anchors_survive_arrivals_and_missing_anchors_context_or_session_changes_are_safe() {
        let mut session = history(200);
        let mut window = ReplyWindow::default();
        window.synchronize(Some(&session));
        window.navigate(&session, Navigation::Earlier);
        let anchor = window.anchor().unwrap().to_owned();
        let before = window.range();
        let mut newest = session.requests.last().unwrap().clone();
        newest.id = "new-original-stream".into();
        newest.status = RequestStatus::Pending;
        session.active_request_id = Some(newest.id.clone());
        session.requests.push(newest);
        window.synchronize(Some(&session));
        assert_eq!(window.anchor(), Some(anchor.as_str()));
        assert_eq!(window.range(), before);
        let first = window.range().start;
        session.requests[first].context.revision += 1000;
        window.synchronize(Some(&session));
        assert!(window.replies()[0].answer.is_none());
        window.navigate(&session, Navigation::Latest);
        assert_eq!(window.range(), 185..201);
        assert!(window.replies().last().unwrap().answer.is_none());
        window.navigate(&session, Navigation::Earlier);
        let anchor = window.anchor().unwrap().to_owned();
        session.requests.retain(|request| request.id != anchor);
        window.synchronize(Some(&session));
        assert!(window.latest());
        let other = history(3);
        window.synchronize(Some(&other));
        assert_eq!(window.range(), 0..3);
        assert!(window.latest());
        window.synchronize(None);
        assert!(window.replies().is_empty());
    }
}
