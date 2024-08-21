use std::sync::Mutex;
use eframe::egui;
use atomig::Atomic;
use egui::{FontFamily, RichText, Ui};
use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;
use rivulet::circular_buffer::{Sink, Source};
use rivulet::{splittable, SplittableView, View, ViewMut};
use rust_music_theory::note::{Note, PitchClass};

use crate::ids::NodeId;
use crate::node::{PortStorage, SimpleNode};

#[derive(dsp_stuff_derive::DspNode)]
#[dsp(
    input = "in",
    title = "Pitch Detector",
    cfg_name = "pitch",
    description = "Display the peak pitch of a signal",
    custom_render = "Pitch::render"
)]
pub struct Pitch {
    #[dsp(id)]
    id: NodeId,

    #[dsp(inputs)]
    inputs: PortStorage,

    #[dsp(outputs)]
    outputs: PortStorage,

    #[dsp(default = "Mutex::new(McLeodDetector::new(1024, 512))")]
    state: Mutex<McLeodDetector<f32>>,

    #[dsp(default = "make_buffer()")]
    buffer: Mutex<(splittable::View<Source<f32>>, Sink<f32>)>,

    #[dsp(default = "0.0")]
    pitch: Atomic<f32>,

    #[dsp(default = "0.0")]
    clarity: Atomic<f32>,

    #[dsp(slider(range = "0.0..=1.0"), save, default = "0.5")]
    power_thresh: Atomic<f32>,

    #[dsp(slider(range = "0.0..=1.0"), save, default = "0.5")]
    clarity_thresh: Atomic<f32>,

    #[dsp(slider(range = "0.0..=1.0"), save, default = "0.5")]
    pick_thresh: Atomic<f32>,
}

fn make_buffer() -> Mutex<(splittable::View<Source<f32>>, Sink<f32>)> {
    let (sink, source) = rivulet::circular_buffer::<f32>(128);
    let source = source.into_view();

    Mutex::new((source, sink))
}

fn note_nr(note: Note) -> u8 {
    note.pitch_class.into_u8() + 12 * note.octave
}

fn from_note_nr(nr: u8) -> Note {
    let pitch_class = PitchClass::from_u8(nr % 12);
    let octave = nr / 12;
    Note::new(pitch_class, octave)
}

fn freq_to_note(freq: f32) -> Note {
    let a440 = note_nr(Note::new(PitchClass::A, 4));
    from_note_nr(((12.0 * (freq / 440.0).log2()) as i16 + a440 as i16) as u8)
}

impl Pitch {
    fn render(&self, ui: &mut Ui) {
        let pitch = self.pitch.load(atomig::Ordering::Relaxed);
        let note = freq_to_note(pitch);

        ui.separator();

        ui.label(
            RichText::new(format!("{note} {}", note.octave))
                .family(FontFamily::Monospace)
                .size(20.0)
                .strong(),
        );

        // ui.horizontal(|ui| {
        //     let width = ui.fonts().glyph_width(&egui::TextStyle::Body.resolve(ui.style()), ' ');
        //     ui.spacing_mut().item_spacing.x = width;

        //     ui.label(RichText::new(note.to_string())
        //              .family(FontFamily::Monospace)
        //              .size(20.0)
        //              .strong()
        //     );
        //     ui.label(RichText::new(note.octave.to_string())
        //              .family(FontFamily::Monospace)
        //              .size(15.0)
        //              .strong()
        //     );
        // });

        ui.label(format!("Frequency: {:>5.2} Hz", pitch));

        ui.label(format!(
            "Confidence: {:>1.2}",
            self.clarity.load(atomig::Ordering::Relaxed)
        ));
    }
}

impl SimpleNode for Pitch {
    fn process(&self, inputs: crate::node::ProcessInput, _outputs: crate::node::ProcessOutput) {
        let input = inputs.get("in").unwrap();

        let mut guard = self.buffer.lock().unwrap();

        if guard.0.try_grant(1024).unwrap_or(false) {
            let view = &guard.0.view()[..1024];

            let mut detector = self.state.lock().unwrap();

            let power_thresh = self.power_thresh.load(atomig::Ordering::Relaxed);
            let clarity_thresh = self.clarity_thresh.load(atomig::Ordering::Relaxed);
            let pick_thresh = self.pick_thresh.load(atomig::Ordering::Relaxed);

            if let Some(pitch_detection::Pitch { frequency, clarity }) =
                detector.get_pitch(view, 48_000, power_thresh, clarity_thresh, pick_thresh)
            {
                self.pitch.store(frequency, atomig::Ordering::Relaxed);
                self.clarity.store(clarity, atomig::Ordering::Relaxed);
            }

            guard.0.release(1024);
        }

        if guard.1.try_grant(input.len()).unwrap_or(false) {
            let view = &mut guard.1.view_mut()[..input.len()];

            view.copy_from_slice(input);
            guard.1.release(input.len());
        }
    }
}
