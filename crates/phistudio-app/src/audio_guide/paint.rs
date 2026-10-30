use super::Guide;
use crate::grid_input::Grid;
use egui::{Color32, Painter, Shape, Stroke, pos2};
use phistudio_core::{AudioGuideOptions, TempoMap};
impl Guide {
    pub fn paint_waveform(
        &self,
        painter: &Painter,
        grid: Grid,
        tempo: &TempoMap,
        offset: f64,
        options: &AudioGuideOptions,
        pixels_per_point: f32,
    ) {
        let p = painter.with_clip_rect(grid.rect.intersect(painter.clip_rect()));
        if options.waveform
            && let Some(waveform) = &self.waveform
        {
            let physical = (grid.rect.height() * pixels_per_point)
                .ceil()
                .clamp(1., 4096.) as usize;
            let step = grid.rect.height() / physical as f32;
            let center = grid.rect.center().x;
            let scale = grid.rect.width() * 0.46;
            let mut peaks = egui::Mesh::default();
            let mut rms = egui::Mesh::default();
            let peak_color = crate::theme::accent().gamma_multiply(options.opacity);
            let rms_color = crate::theme::accent().gamma_multiply(options.opacity * 0.65);
            for i in 0..physical {
                let top = grid.rect.top() + i as f32 * step;
                let bottom = top + step;
                let start = tempo.seconds_at(grid.raw_beat(bottom)) + offset;
                let end = tempo.seconds_at(grid.raw_beat(top)) + offset;
                if end <= 0. || start >= waveform.duration() {
                    continue;
                }
                let sample = waveform.range(start, end);
                let low = (sample.min * options.gain).clamp(-1., 1.);
                let high = (sample.max * options.gain).clamp(-1., 1.);
                if high - low > 0.00001 {
                    peaks.add_colored_rect(
                        egui::Rect::from_min_max(
                            pos2(center + low * scale, top),
                            pos2(center + high * scale, bottom),
                        ),
                        peak_color,
                    );
                    let width = (sample.rms * options.gain).min(1.) * scale;
                    rms.add_colored_rect(
                        egui::Rect::from_min_max(
                            pos2(center - width, top),
                            pos2(center + width, bottom),
                        ),
                        rms_color,
                    );
                }
            }
            p.add(Shape::mesh(peaks));
            p.add(Shape::mesh(rms));
        }
    }

    pub fn paint_markers(
        &self,
        painter: &Painter,
        grid: Grid,
        tempo: &TempoMap,
        offset: f64,
        options: &AudioGuideOptions,
        pixels_per_point: f32,
    ) {
        let p = painter.with_clip_rect(grid.rect.intersect(painter.clip_rect()));
        if options.attacks {
            let start = tempo.seconds_at(grid.raw_beat(grid.rect.bottom() + 4.)) + offset;
            let end = tempo.seconds_at(grid.raw_beat(grid.rect.top() - 4.)) + offset;
            let first = self.markers.partition_point(|m| m.seconds < start);
            // At small zoom, retain the strongest marker per physical row.
            let mut rows = std::collections::BTreeMap::<i32, (&phistudio_audio::Onset, f32)>::new();
            for marker in self.markers[first..]
                .iter()
                .take_while(|m| m.seconds <= end)
            {
                let y = grid.y(tempo.beat_at(marker.seconds - offset));
                let row = (y * pixels_per_point).round() as i32;
                if rows
                    .get(&row)
                    .is_none_or(|(old, _)| old.strength < marker.strength)
                {
                    rows.insert(row, (marker, y));
                }
            }
            for (_, (marker, y)) in rows {
                let color = match marker.band {
                    1 => Color32::from_rgb(245, 190, 100),
                    2 => Color32::from_rgb(100, 207, 230),
                    _ => Color32::from_rgb(216, 145, 235),
                };
                p.line_segment(
                    [pos2(grid.rect.left(), y), pos2(grid.rect.right(), y)],
                    Stroke::new(1_f32, color.gamma_multiply(0.10 + 0.16 * marker.strength)),
                );
                let x = grid.rect.right() - 4.;
                let radius = 2. + marker.strength * 1.5;
                p.add(Shape::convex_polygon(
                    vec![
                        pos2(x - radius, y),
                        pos2(x, y - radius),
                        pos2(x + radius, y),
                        pos2(x, y + radius),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }
}
