//! Prepared snapshots shared by authoring overlays, export and Preview.
use super::*;
use crate::LineEvent;
#[derive(Clone, Debug)]
struct ColorKey {
    start: f64,
    end: f64,
    from: [u8; 3],
    to: [u8; 3],
    curve: LineEvent,
    instant: bool,
}
#[derive(Clone, Debug)]
pub struct Field {
    pub native: Option<native::BlockArea>,
    pub next: Option<NextArea>,
    color: [u8; 3],
    colors: Vec<ColorKey>,
}
impl Field {
    pub fn new(raw: &Value, tempo: &TempoMap) -> Result<Self, ChartError> {
        validate(raw)?;
        let (native, next) = match format(raw)? {
            Format::Native => {
                let mut a: native::BlockArea = serde_json::from_value(raw.clone())?;
                a.normalize()?;
                (Some(a), None)
            }
            Format::Next => (None, Some(NextArea::new(raw, tempo)?)),
        };
        let mut colors = keys(raw, Track::Color)?
            .iter()
            .map(|value| {
                let mut v = value.clone();
                v["start"] = json!(0.);
                v["end"] = json!(1.);
                v["bezier"] = json!(u8::from(flag(&v["bezier"])));
                let curve: LineEvent = serde_json::from_value(v)?;
                Ok(ColorKey {
                    start: tempo.seconds_at(curve.start_time.as_f64()),
                    end: tempo.seconds_at(curve.end_time.as_f64()),
                    from: rgb(&value["start"]).ok_or_else(|| invalid("Invalid color key"))?,
                    to: rgb(&value["end"]).ok_or_else(|| invalid("Invalid color key"))?,
                    curve,
                    instant: flag(&value["inst"]),
                })
            })
            .collect::<Result<Vec<_>, ChartError>>()?;
        colors.sort_by(|a, b| a.start.total_cmp(&b.start));
        Ok(Self {
            native,
            next,
            color: color(raw),
            colors,
        })
    }
    pub fn sample(&self, time: f64, aspect: f32) -> Option<Sample> {
        let mut s = if let Some(a) = &self.native {
            let t = time as f32;
            if !a.visible(t) {
                return None;
            }
            Sample {
                pose: a.pose(t, aspect),
                y_scale: 1.,
                phase: if a.active(t) {
                    2
                } else if a.ready(t) {
                    1
                } else {
                    0
                },
                opacity: a.fade(t),
                subtract: a.is_subtract,
                color: self.color,
            }
        } else {
            self.next.as_ref()?.sample(time, aspect)?
        };
        s.color = self.color_at(time);
        Some(s)
    }
    pub fn color_at(&self, time: f64) -> [u8; 3] {
        self.color_values(time)
            .map(|v| v.round().clamp(0., 255.) as u8)
    }
    pub(super) fn color_values(&self, time: f64) -> [f64; 3] {
        let mut color = self.color.map(f64::from);
        let i = self.colors.partition_point(|k| k.start <= time);
        if i > 0 {
            let k = &self.colors[i - 1];
            let p = if k.end <= k.start || k.instant {
                1.
            } else {
                ((time - k.start) / (k.end - k.start)).clamp(0., 1.)
            };
            let f = curve(&k.curve, p);
            color = std::array::from_fn(|i| {
                (k.from[i] as f64 + (k.to[i] as f64 - k.from[i] as f64) * f).clamp(0., 255.)
            });
        }
        color
    }
}
/// rpe-next interval curves. Its Elastic InOut differs from legacy RPE/prpr;
/// keep this sampler shared by Noise Field authoring, Preview and export.
pub fn curve(event: &LineEvent, f: f64) -> f64 {
    let t = f.clamp(0., 1.);
    if event.bezier != 0 {
        let [a, b, c, d] = event.bezier_points;
        let component = |t: f64, p: f64, q: f64| {
            3. * t * (1. - t).powi(2) * p + 3. * t * t * (1. - t) * q + t * t * t
        };
        let (mut lo, mut hi) = (0., 1.);
        for _ in 0..48 {
            let m = (lo + hi) * 0.5;
            if component(m, a, c) < t {
                lo = m;
            } else {
                hi = m;
            }
        }
        component((lo + hi) * 0.5, b, d)
    } else {
        let (a, b) = (event.easing_left, event.easing_right);
        let base = scalar_curve(event.easing_type, a);
        let span = scalar_curve(event.easing_type, b) - base;
        if span.abs() < 1e-12 {
            f
        } else {
            (scalar_curve(event.easing_type, a + (b - a) * t) - base) / span
        }
    }
}

fn scalar_curve(kind: i32, t: f64) -> f64 {
    if kind != 29 {
        return crate::tools::java_easing(kind, t);
    }
    if t == 0. || t == 1. {
        return t;
    }
    let wave = ((20. * t - 11.125) * 2. * std::f64::consts::PI / 4.5).sin() / 2.;
    if t < 0.5 {
        -2_f64.powf(20. * t - 10.) * wave
    } else {
        2_f64.powf(10. - 20. * t) * wave + 1.
    }
}
