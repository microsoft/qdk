//! The shots an [`AnswersLater`] counts in flight: a call's shots from its
//! reply given until that reply ends or is dropped, and a refused call's
//! never.

use std::sync::Arc;
use std::task::Waker;

use executor_protocol::{
    Close, Executor, InstructionBatchRef, InstructionSet, Reply, ReplyStatus, ShotSet,
};

use super::{AnswersLater, InFlight};
use crate::coin::coin_device;

/// Coins over a set of no instruction, answering later, counting in
/// `in_flight`; and four shots to call them on.
macro_rules! coins_answering_later {
    ($in_flight:expr) => {
        (
            AnswersLater::new(
                coin_device(Arc::new(InstructionSet::new("none")), 0x5EED),
                Arc::clone($in_flight),
            ),
            ShotSet::new(vec![2, 5, 6, 11]).expect("distinct shots"),
        )
    };
}

/// The coins, closed, refuse a call on four shots: none of its shots is
/// counted in flight, since no reply carries them.
#[test]
fn a_refused_call_puts_no_shot_in_flight() {
    let in_flight = Arc::new(InFlight::default());
    let (mut coins, shots) = coins_answering_later!(&in_flight);
    let _ = coins.close();

    let call = coins.call(InstructionBatchRef {
        shots: &shots,
        instructions: &[],
    });
    assert!(call.is_err(), "the closed coins refuse the call");
    assert_eq!(in_flight.most(), 0);
}

/// A caller drops a reply on four shots unpolled, then calls on four shots
/// again: the dropped reply's shots left the flight, so at most four were
/// in flight at once.
#[test]
fn a_dropped_reply_takes_its_shots_out_of_flight() {
    let in_flight = Arc::new(InFlight::default());
    let (mut coins, shots) = coins_answering_later!(&in_flight);
    let batch = InstructionBatchRef {
        shots: &shots,
        instructions: &[],
    };

    let first = coins.call(batch).expect("the coins take the call");
    drop(first);
    let _second = coins.call(batch).expect("the coins take the call");
    assert_eq!(in_flight.most(), 4);
}

/// A caller polls a reply on four shots to its end and keeps it, then calls
/// on four shots again: the ended reply's shots left the flight at its
/// end, so at most four were in flight at once.
#[test]
fn an_ended_reply_takes_its_shots_out_of_flight() {
    let in_flight = Arc::new(InFlight::default());
    let (mut coins, shots) = coins_answering_later!(&in_flight);
    let batch = InstructionBatchRef {
        shots: &shots,
        instructions: &[],
    };

    let mut first = coins.call(batch).expect("the coins take the call");
    let mut events = Vec::new();
    let status = (0..3)
        .map(|_| first.poll_events(&mut events, Waker::noop()))
        .find(|status| !matches!(status, ReplyStatus::Pending));
    assert_eq!(status, Some(ReplyStatus::Done), "the reply ends");
    let _second = coins.call(batch).expect("the coins take the call");
    assert_eq!(in_flight.most(), 4);
    drop(first);
}
