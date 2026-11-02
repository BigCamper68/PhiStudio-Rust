//! rpe-next 220 intervals and cumulative anchor geometry use standard easing
//! equations, including the rpe-next Elastic InOut period.
use super::*;
use crate::LineEvent;
use glam::{DMat3, DVec2, DVec3, Vec2};
use native::Pose;
#[derive(Clone, Debug)]
struct Key {
    start: f64,
    end: f64,
    curve: LineEvent,
    anchor: DVec2,
    reference: [f64; 2],
    prefix: DMat3,
    instant: bool,
}
#[derive(Clone, Debug)]
struct TrackData {
    keys: Vec<Key>,
    kind: Track,
    fallback: f64,
}
fn relative(kind: Track, from: f64, to: f64, anchor: DVec2) -> DMat3 {
    if !kind.anchored() {
        return DMat3::IDENTITY;
    }
    if kind == Track::Rotate {
        let (s, c) = (-(to - from)).to_radians().sin_cos();
        let origin = anchor - DVec2::new(c * anchor.x - s * anchor.y, s * anchor.x + c * anchor.y);
        DMat3::from_cols(
            DVec3::new(c, s, 0.),
            DVec3::new(-s, c, 0.),
            origin.extend(1.),
        )
    } else {
        let ratio = to / from;
        let ratio = if from == 0. || !ratio.is_finite() {
            1.
        } else {
            ratio
        };
        let scale = if kind == Track::ScaleX {
            DVec2::new(ratio, 1.)
        } else {
            DVec2::new(1., ratio)
        };
        DMat3::from_scale_angle_translation(scale, 0., anchor * (DVec2::ONE - scale))
    }
}
impl TrackData {
    fn new(
        field: &Value,
        kind: Track,
        tempo: &TempoMap,
        center: DVec2,
    ) -> Result<Self, ChartError> {
        let fallback = match kind {
            Track::MoveX => center.x,
            Track::MoveY => center.y,
            Track::ScaleX | Track::ScaleY => 1.,
            _ => 0.,
        };
        let mut keys = super::keys(field, kind)?
            .iter()
            .map(|value| {
                let mut curve_value = value.clone();
                curve_value["bezier"] = json!(u8::from(flag(&value["bezier"])));
                let curve: LineEvent = serde_json::from_value(curve_value)?;
                let anchor = value
                    .get("anchor")
                    .map(point)
                    .transpose()?
                    .map(DVec2::from_array)
                    .unwrap_or(center);
                Ok(Key {
                    start: tempo.seconds_at(curve.start_time.as_f64()),
                    end: tempo.seconds_at(curve.end_time.as_f64()),
                    anchor,
                    reference: [
                        value["transformStart"].as_f64().unwrap_or(curve.start),
                        value["transformEnd"].as_f64().unwrap_or(curve.end),
                    ],
                    curve,
                    prefix: DMat3::IDENTITY,
                    instant: flag(&value["inst"]),
                })
            })
            .collect::<Result<Vec<_>, ChartError>>()?;
        keys.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut prefix = DMat3::IDENTITY;
        for i in 0..keys.len() {
            if i > 0 {
                let p = &keys[i - 1];
                prefix = relative(kind, p.reference[0], p.reference[1], p.anchor) * prefix;
                prefix = relative(kind, p.reference[1], keys[i].reference[0], p.anchor) * prefix;
            }
            keys[i].prefix = prefix;
        }
        Ok(Self {
            keys,
            kind,
            fallback,
        })
    }
    fn sample(&self, t: f64) -> (f64, Option<&Key>) {
        let i = self.keys.partition_point(|key| key.start <= t + 1e-9);
        if i == 0 {
            return (self.fallback, None);
        }
        let key = &self.keys[i - 1];
        let p = if key.end <= key.start || key.instant {
            1.
        } else {
            ((t - key.start) / (key.end - key.start)).clamp(0., 1.)
        };
        (
            key.curve.start + (key.curve.end - key.curve.start) * curve(&key.curve, p),
            Some(key),
        )
    }
    fn transform(&self, p: DVec2, t: f64) -> DVec2 {
        let (value, key) = self.sample(t);
        key.map_or(p, |k| {
            (relative(self.kind, k.reference[0], value, k.anchor) * k.prefix).transform_point2(p)
        })
    }
}
pub(super) fn authoring_value(
    field: &Value,
    kind: Track,
    time: f64,
    tempo: &TempoMap,
) -> Result<(f64, [f64; 2]), ChartError> {
    let center = DVec2::from_array(center(field)?);
    let track = TrackData::new(field, kind, tempo, center)?;
    let (value, key) = track.sample(time);
    Ok((value, key.map_or(center, |k| k.anchor).to_array()))
}
#[derive(Clone, Debug)]
pub struct NextArea {
    center: DVec2,
    size: DVec2,
    lifetime: [f64; 2],
    intervals: Vec<[f64; 3]>,
    tracks: [TrackData; 5],
    subtract: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub pose: Pose,
    pub y_scale: f32,
    pub phase: u8,
    pub opacity: f32,
    pub subtract: bool,
    pub color: [u8; 3],
}
impl Sample {
    pub fn corners(self) -> [Vec2; 4] {
        self.pose
            .corners()
            .map(|p| self.pose.center + (p - self.pose.center) * Vec2::new(1., self.y_scale))
    }
}
impl NextArea {
    pub fn new(value: &Value, tempo: &TempoMap) -> Result<Self, ChartError> {
        validate(value)?;
        let [bl, tr] = bounds(value)?;
        let center = (DVec2::from_array(bl) + DVec2::from_array(tr)) * 0.5;
        let lifetime = [
            time(value, "appearTime", Format::Next, tempo)?,
            time(value, "disappearTime", Format::Next, tempo)?,
        ];
        let mut intervals = keys(value, Track::Active)?
            .iter()
            .map(|v| {
                let ready = match v.get("readyFlash") {
                    Some(Value::Bool(false)) => 0.,
                    Some(Value::Number(n)) => {
                        (n.as_f64().unwrap_or(0.).max(0.) * 1000.).round() / 1000.
                    }
                    _ => 0.5,
                };
                Ok([
                    time(v, "startTime", Format::Next, tempo)?.max(lifetime[0]),
                    time(v, "endTime", Format::Next, tempo)?.min(lifetime[1]),
                    ready,
                ])
            })
            .collect::<Result<Vec<_>, ChartError>>()?;
        intervals.retain(|v| v[1] >= v[0]);
        intervals.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let tracks = [
            Track::MoveX,
            Track::MoveY,
            Track::ScaleX,
            Track::ScaleY,
            Track::Rotate,
        ]
        .map(|kind| TrackData::new(value, kind, tempo, center));
        let [a, b, c, d, e] = tracks;
        Ok(Self {
            center,
            size: (DVec2::from_array(tr) - DVec2::from_array(bl)).abs(),
            lifetime,
            intervals,
            tracks: [a?, b?, c?, d?, e?],
            subtract: value["isInvert"].as_bool().unwrap_or(false),
        })
    }
    pub fn sample(&self, t: f64, aspect: f32) -> Option<Sample> {
        if t + 1e-9 < self.lifetime[0] || t + 1e-9 >= self.lifetime[1] {
            return None;
        }
        Some(self.sample_unbounded(t, aspect))
    }
    fn sample_unbounded(&self, t: f64, aspect: f32) -> Sample {
        let active = self
            .intervals
            .iter()
            .rev()
            .find(|i| i[0] <= t + 1e-9 && t + 1e-9 < i[1]);
        let ready = self
            .intervals
            .iter()
            .find(|i| i[0] > t + 1e-9)
            .filter(|i| t + 1e-9 >= i[0] - i[2] && i[2] > 0.);
        let values = self.tracks.each_ref().map(|tr| tr.sample(t).0);
        let center = self.tracks[4].transform(
            self.tracks[3].transform(self.tracks[2].transform(self.center, t), t),
            t,
        ) + DVec2::new(values[0], values[1])
            - self.center;
        let factor = 10. * aspect / 1350.;
        Sample {
            pose: Pose {
                center: Vec2::new(center.x as f32 * factor, center.y as f32 * 10. / 900.),
                size: Vec2::new(
                    (self.size.x * values[2]).abs() as f32 * factor,
                    (self.size.y * values[3]).abs() as f32 * factor,
                ),
                angle: -(values[4] as f32).to_radians(),
            },
            y_scale: 1.5 / aspect,
            phase: if active.is_some() {
                2
            } else if ready.is_some() {
                1
            } else {
                0
            },
            opacity: 1.,
            subtract: self.subtract,
            color: DEFAULT_COLOR,
        }
    }
}
