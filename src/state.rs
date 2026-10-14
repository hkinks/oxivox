//! Pure recording state machine. The daemon feeds it events and executes the returned actions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    /// Capture opened, no audio yet (a Bluetooth headset switching to call mode delivers zeros first).
    WarmingUp,
    Recording,
    Transcribing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicFailure {
    /// The stream never connected: there is no usable source.
    NoMicrophone,
    /// Connected, but only silence arrived within the warm-up window.
    NeverStarted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Text,
    /// Nothing audible, or only non-speech tokens.
    Empty,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Toggle,
    Cancel,
    MicLive,
    MicFailed(MicFailure),
    MaxLength,
    Transcribed(Outcome),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flash {
    NothingHeard,
    NoMicrophone,
    MicDidNotStart,
    TranscriptionFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Note the focused window, pause media, open the microphone, show the warm-up overlay.
    Begin,
    ShowRecording,
    StopCapture,
    ShowTranscribing,
    /// Hand the captured audio to the engine.
    Transcribe,
    CancelTranscription,
    /// Hide the overlay, paste the text, resume media.
    Deliver,
    ResumeMedia,
    Hide,
    Flash(Flash),
}

impl From<MicFailure> for Flash {
    fn from(failure: MicFailure) -> Self {
        match failure {
            MicFailure::NoMicrophone => Flash::NoMicrophone,
            MicFailure::NeverStarted => Flash::MicDidNotStart,
        }
    }
}

use Action::*;
use State::*;

/// Next state and the actions to run, in order. Events that make no sense in a state are ignored.
pub fn step(state: State, event: Event) -> (State, Vec<Action>) {
    match (state, event) {
        (Idle, Event::Toggle) => (WarmingUp, vec![Begin]),
        (Idle, Event::Cancel) => (Idle, vec![Hide]),

        (WarmingUp, Event::MicLive) => (Recording, vec![ShowRecording]),
        (WarmingUp, Event::Toggle | Event::Cancel) => (Idle, vec![StopCapture, ResumeMedia, Hide]),
        (WarmingUp | Recording, Event::MicFailed(failure)) => {
            (Idle, vec![StopCapture, ResumeMedia, Flash(failure.into())])
        }

        (Recording, Event::Toggle | Event::MaxLength) => {
            (Transcribing, vec![StopCapture, ShowTranscribing, Transcribe])
        }
        (Recording, Event::Cancel) => (Idle, vec![StopCapture, ResumeMedia, Hide]),

        (Transcribing, Event::Cancel) => (Idle, vec![CancelTranscription, ResumeMedia, Hide]),
        (Transcribing, Event::Transcribed(Outcome::Text)) => (Idle, vec![Deliver]),
        (Transcribing, Event::Transcribed(Outcome::Empty)) => {
            (Idle, vec![ResumeMedia, Flash(self::Flash::NothingHeard)])
        }
        (Transcribing, Event::Transcribed(Outcome::Failed)) => {
            (Idle, vec![ResumeMedia, Flash(self::Flash::TranscriptionFailed)])
        }

        (state, _) => (state, vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(events: &[Event]) -> (State, Vec<Action>) {
        let mut state = Idle;
        let mut actions = vec![];
        for &event in events {
            let (next, mut out) = step(state, event);
            state = next;
            actions.append(&mut out);
        }
        (state, actions)
    }

    #[test]
    fn happy_path() {
        let (state, actions) = run(&[Event::Toggle, Event::MicLive, Event::Toggle, Event::Transcribed(Outcome::Text)]);
        assert_eq!(state, Idle);
        assert_eq!(actions, [Begin, ShowRecording, StopCapture, ShowTranscribing, Transcribe, Deliver]);
    }

    #[test]
    fn toggle_during_transcription_is_ignored() {
        let (state, actions) = run(&[Event::Toggle, Event::MicLive, Event::Toggle, Event::Toggle]);
        assert_eq!(state, Transcribing);
        assert_eq!(actions.iter().filter(|a| **a == Transcribe).count(), 1);
    }

    #[test]
    fn cancel_from_every_active_state() {
        for prefix in
            [&[Event::Toggle][..], &[Event::Toggle, Event::MicLive], &[Event::Toggle, Event::MicLive, Event::Toggle]]
        {
            let mut events = prefix.to_vec();
            events.push(Event::Cancel);
            let (state, actions) = run(&events);
            assert_eq!(state, Idle);
            assert!(actions.ends_with(&[ResumeMedia, Hide]), "{actions:?}");
            assert!(!actions.contains(&Deliver));
        }
    }

    #[test]
    fn toggle_while_warming_up_cancels() {
        let (state, actions) = run(&[Event::Toggle, Event::Toggle]);
        assert_eq!(state, Idle);
        assert_eq!(actions, [Begin, StopCapture, ResumeMedia, Hide]);
    }

    #[test]
    fn empty_result_flashes_and_does_not_deliver() {
        let (state, actions) = run(&[Event::Toggle, Event::MicLive, Event::Toggle, Event::Transcribed(Outcome::Empty)]);
        assert_eq!(state, Idle);
        assert_eq!(actions.last(), Some(&Flash(self::Flash::NothingHeard)));
        assert!(!actions.contains(&Deliver));
    }

    #[test]
    fn mic_failure_flashes_error() {
        let (state, actions) = run(&[Event::Toggle, Event::MicFailed(MicFailure::NoMicrophone)]);
        assert_eq!(state, Idle);
        assert_eq!(actions, [Begin, StopCapture, ResumeMedia, Flash(self::Flash::NoMicrophone)]);
        let (_, actions) = run(&[Event::Toggle, Event::MicFailed(MicFailure::NeverStarted)]);
        assert_eq!(actions.last(), Some(&Flash(self::Flash::MicDidNotStart)));
    }

    #[test]
    fn max_length_stops_like_a_toggle() {
        let (state, actions) = run(&[Event::Toggle, Event::MicLive, Event::MaxLength]);
        assert_eq!(state, Transcribing);
        assert!(actions.contains(&Transcribe));
    }

    #[test]
    fn stale_events_are_ignored() {
        for event in [
            Event::MicLive,
            Event::MaxLength,
            Event::Transcribed(Outcome::Text),
            Event::MicFailed(MicFailure::NoMicrophone),
        ] {
            assert_eq!(step(Idle, event), (Idle, vec![]));
        }
        assert_eq!(step(Recording, Event::MicLive), (Recording, vec![]));
    }
}
