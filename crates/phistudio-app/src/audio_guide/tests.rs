use super::*;
use phistudio_audio::{Candidate, StereoSource};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
fn fixture() -> Arc<Analysis> {
    struct Silence;
    impl StereoSource for Silence {
        fn sample_rate(&self) -> u32 {
            8000
        }
        fn frame_count(&self) -> usize {
            800
        }
        fn frame(&self, _: usize) -> [f32; 2] {
            [0.; 2]
        }
    }
    let mut data = phistudio_audio::analyze(&Silence, &Control::default(), |_| {}).unwrap();
    data.candidates.push(Candidate {
        seconds: 0.05,
        strength: [0.9; 4],
        band: 1,
    });
    Arc::new(data)
}
#[test]
fn stale_project_results_and_stale_selection_results_cannot_replace_current_guides() {
    let (sender, _commands) = mpsc::channel();
    let (events, receiver) = mpsc::channel();
    let data = fixture();
    let mut guide = Guide::default();
    guide.worker = Some(Worker { sender, receiver });
    guide.epoch = 8;
    guide.tuning = 2;
    guide.manual_cancel = true;
    events
        .send(Message::Done {
            epoch: 7,
            data: Arc::clone(&data),
            markers: vec![],
            selection: Default::default(),
            cached: false,
            full_data: true,
        })
        .unwrap();
    events
        .send(Message::Selected {
            epoch: 8,
            tuning: 1,
            markers: vec![Onset {
                seconds: 0.1,
                strength: 1.,
                band: 1,
            }],
        })
        .unwrap();
    guide.update(0, 0, None, None, &Default::default());
    assert!(guide.result.is_none() && guide.waveform.is_none() && guide.markers.is_empty());
}
#[test]
fn changing_display_selection_uses_existing_analysis_without_a_decoder_or_chart() {
    let data = fixture();
    let mut guide = Guide::default();
    guide.result = Some(data);
    guide.status = Status::Ready;
    let options = phistudio_core::AudioGuideOptions {
        attacks: true,
        band: 2,
        ..Default::default()
    };
    guide.update(0, 0, None, None, &options);
    let deadline = Instant::now() + Duration::from_secs(3);
    while guide.working && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
        guide.update(0, 0, None, None, &options);
    }
    assert!(!guide.working);
    assert_eq!(guide.markers.len(), 1);
    assert_eq!(guide.markers[0].seconds, 0.05);
    guide.cancel();
    guide.update(0, 0, None, None, &options);
    assert!(!guide.working, "Explicit cancellation cannot auto-restart");
    guide.manual_cancel = false;
    guide.update(0, 0, None, None, &options);
    assert!(guide.working);
}

fn clip() -> ResourceAudioClip {
    let bytes = 8000_u32 * 4;
    let mut wav = vec![];
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&8000_u32.to_le_bytes());
    wav.extend_from_slice(&32000_u32.to_le_bytes());
    wav.extend_from_slice(&4_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    wav.resize(wav.len() + bytes as usize, 0);
    ResourceAudioClip::new(wav).unwrap()
}
#[test]
fn waveform_only_skips_fft_then_enabling_attacks_analyzes_the_same_clip_once() {
    let clip = clip();
    let mut guide = Guide::default();
    guide.update(
        1,
        0,
        Some("test.wav"),
        Some(clip.clone()),
        &Default::default(),
    );
    assert!(guide.worker.is_none() && !guide.working && guide.waveform.is_none());
    let mut options = phistudio_core::AudioGuideOptions {
        waveform: true,
        attacks: false,
        ..Default::default()
    };
    let wait = |guide: &mut Guide, options: &phistudio_core::AudioGuideOptions| {
        guide.update(1, 0, Some("test.wav"), Some(clip.clone()), options);
        let deadline = Instant::now() + Duration::from_secs(5);
        while guide.working && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            guide.update(1, 0, Some("test.wav"), Some(clip.clone()), options);
        }
        assert!(!guide.working);
    };
    wait(&mut guide, &options);
    assert!(guide.waveform.is_some() && !guide.full_data);
    options.attacks = true;
    wait(&mut guide, &options);
    assert!(guide.full_data);
    let epoch = guide.epoch;
    wait(&mut guide, &options);
    assert_eq!(guide.epoch, epoch);
    guide.update(2, 0, Some("test.wav"), Some(clip.clone()), &options);
    assert!(
        guide.waveform.is_none(),
        "Switching project clears the previous guide immediately"
    );
}

#[test]
fn markers_follow_piecewise_bpm_and_signed_audio_offset_without_touching_the_chart() {
    use egui::{Rect, pos2};
    use phistudio_core::ChartDocument;
    let mut raw = ChartDocument::empty("Guide timing", 120.)
        .unwrap()
        .as_json()
        .clone();
    raw["BPMList"] =
        serde_json::json!([{"bpm":120,"startTime":[0,0,1]},{"bpm":240,"startTime":[0,1,10]}]);
    let doc = ChartDocument::from_value(raw.clone()).unwrap();
    let mut guide = Guide::default();
    guide.markers = vec![Onset {
        seconds: 0.08,
        strength: 1.,
        band: 2,
    }];
    let grid = crate::grid_input::Grid {
        rect: Rect::from_min_max(pos2(20., 0.), pos2(220., 300.)),
        event_rect: Rect::NOTHING,
        beat: 0.,
        zoom: 100.,
        division: 4,
        lanes: 11,
        snap_x_enabled: true,
        scroll_speed: 1.,
        note_width: 46.,
        camera_x: 0.,
    };
    let options = phistudio_core::AudioGuideOptions {
        waveform: false,
        attacks: true,
        ..Default::default()
    };
    for (offset, beat) in [(0.01, 0.18), (-0.01, 0.26), (0.03, 0.1)] {
        let ctx = egui::Context::default();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                guide.paint_markers(ui.painter(), grid, doc.tempo(), offset, &options, 2.75)
            });
        });
        let marker = out
            .shapes
            .iter()
            .find_map(|s| match &s.1 {
                egui::Shape::Path(p) if p.points.len() == 4 => Some((s.0, p)),
                _ => None,
            })
            .unwrap();
        let y = marker.1.points.iter().map(|p| p.y).sum::<f32>() / 4.;
        assert!(
            (y - (276. - beat * 100.)).abs() < 0.001,
            "offset {offset}: y {y}"
        );
        assert_eq!(marker.0, grid.rect);
    }
    assert_eq!(doc.as_json(), &raw);
}

#[test]
fn replacing_a_document_preserves_the_worker_cache_and_clears_old_project_guides() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::workbench::Workbench::new(
        phistudio_core::ChartDocument::empty("First", 120.).unwrap(),
        None,
    );
    app.audio_guide.set_cache_root(Some(root.path().to_owned()));
    app.audio_guide.result = Some(fixture());
    app.replace_document(
        phistudio_core::ChartDocument::empty("Second", 140.).unwrap(),
        None,
        None,
    );
    assert_eq!(app.audio_guide.cache_root.as_deref(), Some(root.path()));
    assert!(app.audio_guide.result.is_none() && app.audio_guide.markers.is_empty());
}

mod ui;

#[test]
fn explicit_generator_can_request_attacks_with_both_visual_guides_disabled() {
    let clip = clip();
    let mut guide = Guide::default();
    let options = phistudio_core::AudioGuideOptions::default();
    guide.generation_requested = true;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        guide.update(1, 0, Some("test.wav"), Some(clip.clone()), &options);
        if guide.generation_markers(&options).is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Generator did not receive full attack analysis"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!options.waveform && !options.attacks && guide.full_data);
    let epoch = guide.epoch;
    guide.generation_requested = false;
    guide.update(1, 0, Some("test.wav"), Some(clip.clone()), &options);
    guide.generation_requested = true;
    guide.update(1, 0, Some("test.wav"), Some(clip), &options);
    assert_eq!(
        guide.epoch, epoch,
        "Opening the generator again must reuse full analysis"
    );
}
