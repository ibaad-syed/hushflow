//! A meeting's two audio sources, both delivered as 16 kHz mono: the
//! microphone ("Me", through handy-recorder like dictation) and what the
//! computer plays ("Them": a Core Audio process tap on macOS, WASAPI loopback
//! on Windows, both through cpal's input stream on the output device).

use crate::audio_toolkit::audio::Resampled;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use handy_recorder::{AudioChunk, Recorder, RecorderConfig, Sink};
use log::{info, warn};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

/// 16 kHz mono audio, in arrival order. The channel closes when capture stops.
pub type Audio = Receiver<Vec<f32>>;

struct ForwardSink(Sender<Vec<f32>>);

impl Sink for ForwardSink {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        let _ = self.0.send(chunk.samples[..chunk.valid_frames].to_vec());
    }
}

pub struct MicCapture {
    recorder: Recorder<ForwardSink>,
}

impl MicCapture {
    /// Opens the microphone dictation uses (by name), or the default one.
    pub fn start(selected: Option<&str>) -> Result<(Self, Audio)> {
        let device = selected.and_then(|name| {
            handy_recorder::list_input_devices()
                .ok()?
                .into_iter()
                .find(|d| d.name == name)
                .map(|d| d.id)
        });
        let mut config = RecorderConfig::speech();
        config.device = device;
        let recorder = Recorder::open(config)?;
        let (tx, rx) = mpsc::channel();
        recorder
            .start(ForwardSink(tx))
            .map_err(|e| anyhow!("Couldn't start the microphone: {}", e.error))?;
        Ok((Self { recorder }, rx))
    }

    pub fn stop(self) {
        if let Err(e) = self.recorder.stop() {
            warn!("Meeting microphone stopped with an error: {e}");
        }
    }
}

/// The system-audio stream lives on its own thread: cpal streams can't move
/// between threads on every platform.
pub struct SystemCapture {
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl SystemCapture {
    pub fn start() -> Result<(Self, Audio)> {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        let thread = std::thread::Builder::new()
            .name("meeting-system-audio".into())
            .spawn(move || {
                let stream = match open_loopback(tx) {
                    Ok(stream) => stream,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let _ = stop_rx.recv();
                drop(stream);
            })?;
        ready_rx
            .recv()
            .map_err(|_| anyhow!("system audio thread ended"))??;
        Ok((
            Self {
                stop: stop_tx,
                thread: Some(thread),
            },
            rx,
        ))
    }

    pub fn stop(mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn open_loopback(tx: Sender<Vec<f32>>) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("no output device"))?;
    let config = device.default_output_config()?;
    let channels = config.channels() as usize;
    let rate = config.sample_rate();
    info!(
        "Meeting system audio: {:?} at {rate} Hz, {channels} ch",
        device.description().map(|d| d.name().to_string())
    );
    let mut resampler = Resampled::new(rate)?;
    let stream = device.build_input_stream(
        config.config(),
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let mono = data
                .chunks(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32);
            if resampler.push(mono).is_ok() {
                let out = resampler.take_output();
                if !out.is_empty() {
                    let _ = tx.send(out);
                }
            }
        },
        |e| warn!("Meeting system audio error: {e}"),
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

const RATE: usize = 16_000;
const PREROLL: usize = RATE / 5;
const MIN_SPEECH: usize = RATE * 2 / 5;
/// A pause this long ends an utterance; a shorter one does once it runs long.
const END_PAUSE: usize = RATE * 4 / 5;
const LONG_PAUSE: usize = RATE * 3 / 10;
const LONG_UTTERANCE: usize = 15 * RATE;
const MAX_UTTERANCE: usize = 25 * RATE;

/// Cuts a channel into utterances at pauses, so each can be transcribed on
/// its own as the meeting goes: short enough for live captions, long enough
/// for the model to have context.
pub struct Segmenter {
    scorer: Box<dyn crate::audio_toolkit::SpeechScorer>,
    frame: usize,
    /// Audio not yet scored (less than a frame).
    pending: Vec<f32>,
    /// Samples seen so far: the meeting clock for this channel.
    clock: usize,
    /// The last PREROLL samples before speech, kept to lead into it.
    preroll: Vec<f32>,
    /// The utterance in progress and where it started.
    current: Vec<f32>,
    start: usize,
    speech: usize,
    silence: usize,
}

/// One utterance: its start on the meeting clock, in samples, and its audio.
pub type Utterance = (usize, Vec<f32>);

impl Segmenter {
    pub fn new(scorer: Box<dyn crate::audio_toolkit::SpeechScorer>) -> Self {
        let frame = scorer.frame_samples();
        Self {
            scorer,
            frame,
            pending: Vec::new(),
            clock: 0,
            preroll: Vec::new(),
            current: Vec::new(),
            start: 0,
            speech: 0,
            silence: 0,
        }
    }

    pub fn push(&mut self, audio: &[f32]) -> Vec<Utterance> {
        self.pending.extend_from_slice(audio);
        let whole = self.pending.len() / self.frame * self.frame;
        if whole == 0 {
            return Vec::new();
        }
        let frames: Vec<f32> = self.pending.drain(..whole).collect();
        let probs = match self.scorer.score(&frames) {
            Ok(probs) => probs,
            // Unscorable audio isn't speech we can find; keep the clock right.
            Err(e) => {
                warn!("Meeting VAD failed: {e}");
                vec![0.0; frames.len() / self.frame]
            }
        };
        let mut out = Vec::new();
        for (frame, prob) in frames.chunks(self.frame).zip(probs) {
            if let Some(utterance) = self.step(frame, prob) {
                out.push(utterance);
            }
            self.clock += frame.len();
        }
        out
    }

    fn step(&mut self, frame: &[f32], prob: f32) -> Option<Utterance> {
        let in_utterance = !self.current.is_empty();
        if !in_utterance {
            if prob >= 0.5 {
                self.start = self.clock - self.preroll.len();
                self.current = std::mem::take(&mut self.preroll);
                self.current.extend_from_slice(frame);
                self.speech = frame.len();
                self.silence = 0;
            } else {
                self.preroll.extend_from_slice(frame);
                let excess = self.preroll.len().saturating_sub(PREROLL);
                self.preroll.drain(..excess);
            }
            return None;
        }
        self.current.extend_from_slice(frame);
        if prob >= 0.35 {
            self.speech += frame.len();
            self.silence = 0;
        } else {
            self.silence += frame.len();
        }
        let len = self.current.len();
        let pause = if len > LONG_UTTERANCE {
            LONG_PAUSE
        } else {
            END_PAUSE
        };
        if self.silence >= pause || len >= MAX_UTTERANCE {
            return self.take();
        }
        None
    }

    fn take(&mut self) -> Option<Utterance> {
        let audio = std::mem::take(&mut self.current);
        let speech = std::mem::take(&mut self.speech);
        self.silence = 0;
        (speech >= MIN_SPEECH).then_some((self.start, audio))
    }

    /// The utterance in progress, at the end of the meeting.
    pub fn finish(&mut self) -> Option<Utterance> {
        if self.current.is_empty() {
            None
        } else {
            self.take()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_toolkit::vad::{VadFrame, VoiceActivityDetector};

    /// Treats loud samples as speech.
    struct Loud;
    impl VoiceActivityDetector for Loud {
        fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> anyhow::Result<VadFrame<'a>> {
            Ok(if frame[0] > 0.5 {
                VadFrame::Speech(frame)
            } else {
                VadFrame::Noise
            })
        }
        fn frame_samples(&self) -> usize {
            160
        }
    }
    impl crate::audio_toolkit::SpeechScorer for Loud {
        fn score(&mut self, frames: &[f32]) -> anyhow::Result<Vec<f32>> {
            Ok(frames
                .chunks(160)
                .map(|f| if f[0] > 0.5 { 1.0 } else { 0.0 })
                .collect())
        }
    }

    #[test]
    fn utterances_are_cut_at_pauses_with_their_start_times() {
        let mut seg = Segmenter::new(Box::new(Loud));
        let silence = vec![0.0; RATE];
        let speech = vec![1.0; RATE * 2];
        let mut got = Vec::new();
        // Fed in odd-sized pieces, as capture delivers it.
        let audio: Vec<f32> = [&silence[..], &speech, &silence, &speech, &silence].concat();
        for piece in audio.chunks(333) {
            got.extend(seg.push(piece));
        }
        got.extend(seg.finish());
        assert_eq!(got.len(), 2);
        // Each starts a preroll before its speech.
        assert_eq!(got[0].0, RATE - PREROLL);
        assert_eq!(got[1].0, RATE * 4 - PREROLL);
        // A blip shorter than MIN_SPEECH isn't an utterance.
        let mut seg = Segmenter::new(Box::new(Loud));
        let blip: Vec<f32> = [&silence[..], &vec![1.0; 1600], &silence].concat();
        assert!(seg.push(&blip).is_empty() && seg.finish().is_none());
    }
}
