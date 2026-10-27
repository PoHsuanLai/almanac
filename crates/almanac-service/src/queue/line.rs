//! The line of requests waiting for their Spaces.
//!
//! A request joins at the back and runs once nothing running, and nothing ahead of it in the
//! line, holds a Space it needs. So two requests for one Space run in the order they arrived,
//! requests for other Spaces pass by, and a request for every Space is not starved by a stream
//! of single-Space ones. Every Space it needs is taken at once, so two requests cannot deadlock.
//! No runtime is involved: a waiting request parks its waker and is woken when another leaves.

use super::claim::Claim;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
struct Waiter {
    ticket: u64,
    claim: Claim,
    waker: Option<Waker>,
}

#[derive(Debug, Default)]
struct Ledger {
    next: u64,
    running: Vec<(u64, Claim)>,
    waiting: Vec<Waiter>,
}

/// The line itself. Cloning shares it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Line {
    ledger: Arc<Mutex<Ledger>>,
}

/// A request's place in the line. Dropping it leaves the line, whether the request waited, ran or
/// was dropped halfway, and wakes the requests behind.
#[derive(Debug)]
pub(crate) struct Place {
    line: Line,
    ticket: u64,
}

fn lock(ledger: &Mutex<Ledger>) -> MutexGuard<'_, Ledger> {
    ledger.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Line {
    /// Waits for `claim`, then holds it until the returned place drops.
    pub(crate) async fn enter(&self, claim: Claim) -> Place {
        let place = self.join(claim);
        std::future::poll_fn(|cx| place.poll_turn(cx)).await;
        place
    }

    fn join(&self, claim: Claim) -> Place {
        let mut ledger = lock(&self.ledger);
        let ticket = ledger.next;
        ledger.next += 1;
        ledger.waiting.push(Waiter {
            ticket,
            claim,
            waker: None,
        });
        Place {
            line: self.clone(),
            ticket,
        }
    }
}

impl Place {
    /// Ready once nothing running or ahead conflicts; the claim then moves from waiting to running.
    fn poll_turn(&self, cx: &mut Context<'_>) -> Poll<()> {
        let mut ledger = lock(&self.line.ledger);
        let Some(me) = ledger.waiting.iter().position(|w| w.ticket == self.ticket) else {
            return Poll::Ready(());
        };
        let blocked = {
            let mine = &ledger.waiting[me].claim;
            ledger.running.iter().any(|(_, held)| held.conflicts(mine))
                || ledger.waiting[..me].iter().any(|w| w.claim.conflicts(mine))
        };
        if blocked {
            ledger.waiting[me].waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        let granted = ledger.waiting.remove(me);
        ledger.running.push((granted.ticket, granted.claim));
        Poll::Ready(())
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let wakers: Vec<Waker> = {
            let mut ledger = lock(&self.line.ledger);
            ledger.waiting.retain(|w| w.ticket != self.ticket);
            ledger.running.retain(|(ticket, _)| *ticket != self.ticket);
            ledger
                .waiting
                .iter_mut()
                .filter_map(|w| w.waker.take())
                .collect()
        };
        wakers.into_iter().for_each(Waker::wake);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almanac_core::SpaceId;
    use std::future::Future;
    use std::pin::Pin;

    type Entry<'a> = Pin<Box<dyn Future<Output = Place> + 'a>>;

    fn spaces(ids: &[&str]) -> Claim {
        Claim::Spaces(
            ids.iter()
                .map(|id| SpaceId::parse(id).expect("space"))
                .collect(),
        )
    }

    fn poll(entry: &mut Entry<'_>) -> Poll<Place> {
        entry.as_mut().poll(&mut Context::from_waker(Waker::noop()))
    }

    fn granted(entry: &mut Entry<'_>) -> Place {
        match poll(entry) {
            Poll::Ready(place) => place,
            Poll::Pending => panic!("expected a turn"),
        }
    }

    #[test]
    fn requests_for_other_spaces_pass_by() {
        let line = Line::default();
        let mut a: Entry = Box::pin(line.enter(spaces(&["a"])));
        let mut b: Entry = Box::pin(line.enter(spaces(&["b"])));
        let _first = granted(&mut a);
        let _second = granted(&mut b);
    }

    #[test]
    fn a_second_request_for_one_space_waits_for_the_first() {
        let line = Line::default();
        let mut first: Entry = Box::pin(line.enter(spaces(&["a", "b"])));
        let mut second: Entry = Box::pin(line.enter(spaces(&["b", "c"])));
        let held = granted(&mut first);
        assert!(poll(&mut second).is_pending());
        drop(held);
        let _turn = granted(&mut second);
    }

    #[test]
    fn a_request_for_every_space_is_not_overtaken() {
        let line = Line::default();
        let mut running: Entry = Box::pin(line.enter(spaces(&["a"])));
        let mut all: Entry = Box::pin(line.enter(Claim::Everything));
        let mut later: Entry = Box::pin(line.enter(spaces(&["b"])));
        let held = granted(&mut running);
        assert!(poll(&mut all).is_pending());
        assert!(poll(&mut later).is_pending());
        drop(held);
        let everything = granted(&mut all);
        assert!(poll(&mut later).is_pending());
        drop(everything);
        let _turn = granted(&mut later);
    }

    #[test]
    fn a_request_dropped_while_waiting_frees_those_behind_it() {
        let line = Line::default();
        let mut running: Entry = Box::pin(line.enter(spaces(&["a"])));
        let mut gone: Entry = Box::pin(line.enter(Claim::Everything));
        let mut behind: Entry = Box::pin(line.enter(spaces(&["b"])));
        let held = granted(&mut running);
        assert!(poll(&mut gone).is_pending());
        assert!(poll(&mut behind).is_pending());
        drop(gone);
        let _turn = granted(&mut behind);
        drop(held);
    }

    #[test]
    fn a_request_that_holds_nothing_never_waits() {
        let line = Line::default();
        let mut all: Entry = Box::pin(line.enter(Claim::Everything));
        let mut nothing: Entry = Box::pin(line.enter(Claim::Nothing));
        let _held = granted(&mut all);
        let _turn = granted(&mut nothing);
    }
}
