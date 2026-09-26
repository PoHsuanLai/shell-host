use super::*;

use LockAction::{CreateSurface, Destroy, DestroySurface, Report, Tell, UnlockAndDestroy};

/// A case: its name, the inputs after the start, the outputs left, the surface actions.
type OutputCase<'a> = (&'a str, &'a [LockInput], &'a [OutputId], &'a [LockAction]);

const A: OutputId = OutputId(10);
const B: OutputId = OutputId(11);
const C: OutputId = OutputId(12);

/// Run `inputs` from a fresh lock over `outputs`; the final machine and every action, in order.
fn run(outputs: &[OutputId], inputs: &[LockInput]) -> (LockMachine, Vec<LockAction>) {
    let (mut machine, mut actions) = start(outputs.iter().copied());
    for &input in inputs {
        let (next, more) = step(&machine, input);
        machine = next;
        actions.extend(more);
    }
    (machine, actions)
}

fn outputs(of: &[OutputId]) -> BTreeSet<OutputId> {
    of.iter().copied().collect()
}

#[test]
fn a_lock_starts_with_a_surface_per_output() {
    let (machine, actions) = start([B, A]);
    assert_eq!(machine.phase, LockPhase::Requested);
    assert_eq!(machine.outputs, outputs(&[A, B]));
    assert_eq!(actions, vec![CreateSurface(A), CreateSurface(B)]);
}

#[test]
fn requested_locked_unlocking_unlocked() {
    let cases: &[(&str, &[LockInput], LockPhase, &[LockAction])] = &[
        (
            "locked",
            &[LockInput::Locked],
            LockPhase::Locked,
            &[Tell(LockPhase::Locked), Report(LockState::Locked)],
        ),
        (
            "unlock asked: the documents are told first, the compositor not yet",
            &[LockInput::Locked, LockInput::UnlockAsked],
            LockPhase::Unlocking,
            &[
                Tell(LockPhase::Locked),
                Report(LockState::Locked),
                Tell(LockPhase::Unlocking),
            ],
        ),
        (
            "told: unlock_and_destroy, then every surface, then the reports",
            &[LockInput::Locked, LockInput::UnlockAsked, LockInput::Told],
            LockPhase::Unlocked,
            &[
                Tell(LockPhase::Locked),
                Report(LockState::Locked),
                Tell(LockPhase::Unlocking),
                UnlockAndDestroy,
                DestroySurface(A),
                DestroySurface(B),
                Tell(LockPhase::Unlocked),
                Report(LockState::Unlocked),
            ],
        ),
        (
            "a second unlock while unlocking changes nothing",
            &[
                LockInput::Locked,
                LockInput::UnlockAsked,
                LockInput::UnlockAsked,
            ],
            LockPhase::Unlocking,
            &[
                Tell(LockPhase::Locked),
                Report(LockState::Locked),
                Tell(LockPhase::Unlocking),
            ],
        ),
        (
            "told before any unlock is ignored",
            &[LockInput::Locked, LockInput::Told],
            LockPhase::Locked,
            &[Tell(LockPhase::Locked), Report(LockState::Locked)],
        ),
    ];
    for (name, inputs, phase, want) in cases {
        let (machine, got) = run(&[A, B], inputs);
        let created = [CreateSurface(A), CreateSurface(B)];
        assert_eq!(machine.phase, *phase, "{name}");
        assert_eq!(&got[..2], &created, "{name}");
        assert_eq!(&got[2..], *want, "{name}");
    }
}

#[test]
fn finished_ends_the_lock_with_the_request_the_protocol_allows() {
    let cases: &[(&str, &[LockInput], LockAction)] = &[
        (
            "refused before locked: destroy, never unlock_and_destroy (invalid_unlock)",
            &[LockInput::Finished],
            Destroy,
        ),
        (
            "ended after locked: unlock_and_destroy, never destroy (invalid_destroy)",
            &[LockInput::Locked, LockInput::Finished],
            UnlockAndDestroy,
        ),
        (
            "ended while unlocking",
            &[
                LockInput::Locked,
                LockInput::UnlockAsked,
                LockInput::Finished,
            ],
            UnlockAndDestroy,
        ),
    ];
    for (name, inputs, request) in cases {
        let (machine, got) = run(&[A], inputs);
        assert_eq!(machine.phase, LockPhase::Finished, "{name}");
        assert!(machine.outputs.is_empty(), "{name}");
        let tail = &got[got.len() - 4..];
        assert_eq!(
            tail,
            &[
                *request,
                DestroySurface(A),
                Tell(LockPhase::Finished),
                Report(LockState::Finished)
            ],
            "{name}"
        );
        let requests = got
            .iter()
            .filter(|action| matches!(action, Destroy | UnlockAndDestroy))
            .count();
        assert_eq!(requests, 1, "{name}: exactly one destructor");
    }
}

#[test]
fn an_unlock_before_locked_cancels_with_destroy() {
    let (machine, got) = run(&[A], &[LockInput::UnlockAsked]);
    assert_eq!(machine.phase, LockPhase::Unlocked);
    assert_eq!(
        got,
        vec![
            CreateSurface(A),
            Destroy,
            DestroySurface(A),
            Tell(LockPhase::Unlocked),
            Report(LockState::Unlocked)
        ]
    );
}

#[test]
fn outputs_added_and_removed_while_locked() {
    let cases: &[OutputCase] = &[
        (
            "added while locked: a surface",
            &[LockInput::Locked, LockInput::OutputAdded(C)],
            &[A, B, C],
            &[CreateSurface(C)],
        ),
        (
            "added before locked: a surface too",
            &[LockInput::OutputAdded(C)],
            &[A, B, C],
            &[CreateSurface(C)],
        ),
        (
            "added twice: one surface",
            &[
                LockInput::Locked,
                LockInput::OutputAdded(C),
                LockInput::OutputAdded(C),
            ],
            &[A, B, C],
            &[CreateSurface(C)],
        ),
        (
            "removed while locked: its surface goes",
            &[LockInput::Locked, LockInput::OutputRemoved(A)],
            &[B],
            &[DestroySurface(A)],
        ),
        (
            "removing an unknown output does nothing",
            &[LockInput::Locked, LockInput::OutputRemoved(C)],
            &[A, B],
            &[],
        ),
        (
            "added while unlocking: still locked, so a surface",
            &[
                LockInput::Locked,
                LockInput::UnlockAsked,
                LockInput::OutputAdded(C),
            ],
            &[A, B, C],
            &[CreateSurface(C)],
        ),
    ];
    for (name, inputs, left, want) in cases {
        let (machine, got) = run(&[A, B], inputs);
        let own: Vec<LockAction> = got
            .into_iter()
            .skip(2)
            .filter(|action| matches!(action, CreateSurface(_) | DestroySurface(_)))
            .collect();
        assert_eq!(machine.outputs, outputs(left), "{name}");
        assert_eq!(own, *want, "{name}");
    }
}

#[test]
fn every_surface_made_is_destroyed_by_the_end() {
    let (_, got) = run(
        &[A],
        &[
            LockInput::OutputAdded(B),
            LockInput::Locked,
            LockInput::OutputAdded(C),
            LockInput::OutputRemoved(B),
            LockInput::UnlockAsked,
            LockInput::Told,
        ],
    );
    let made: BTreeSet<OutputId> = got
        .iter()
        .filter_map(|action| match action {
            CreateSurface(output) => Some(*output),
            _ => None,
        })
        .collect();
    let gone: Vec<OutputId> = got
        .iter()
        .filter_map(|action| match action {
            DestroySurface(output) => Some(*output),
            _ => None,
        })
        .collect();
    assert_eq!(made, outputs(&[A, B, C]));
    assert_eq!(gone, vec![B, A, C]);
}

#[test]
fn nothing_moves_an_ended_lock() {
    let inputs = [
        LockInput::Locked,
        LockInput::Finished,
        LockInput::OutputAdded(C),
        LockInput::OutputRemoved(A),
        LockInput::UnlockAsked,
        LockInput::Told,
    ];
    for end in [&[LockInput::Finished][..], &[LockInput::UnlockAsked][..]] {
        let (ended, _) = run(&[A], end);
        for input in inputs {
            let (next, actions) = step(&ended, input);
            assert_eq!(next, ended, "{input:?} after {end:?}");
            assert!(actions.is_empty(), "{input:?} after {end:?}");
        }
        assert_eq!(ended.phase.ended(), Ended::Yes);
    }
}
