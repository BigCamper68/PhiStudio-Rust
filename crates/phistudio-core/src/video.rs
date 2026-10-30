//! A movie uses an integer frame clock, independently of UI frame rate.
use crate::ChartError;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_mbps: u32,
    pub audio_bitrate_kbps: u32,
    pub whole_song: bool,
    pub start: f64,
    pub end: f64,
    pub tail: f64,
    pub audio: bool,
    pub music: f32,
    pub hitsounds: f32,
    pub hud: bool,
    pub particles: bool,
    pub shaders: bool,
}
impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_mbps: 12,
            audio_bitrate_kbps: 192,
            whole_song: true,
            start: 0.,
            end: 30.,
            tail: 1.,
            audio: true,
            music: 1.,
            hitsounds: 0.8,
            hud: true,
            particles: true,
            shaders: true,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct VideoPlan {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u64,
    pub start: f64,
}
impl VideoSettings {
    pub fn plan(&self, song_end: f64) -> Result<VideoPlan, ChartError> {
        let invalid = |s: &str| ChartError::Invalid(s.into());
        if self.width < 64
            || self.height < 64
            || self.width > 4096
            || self.height > 4096
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || u64::from(self.width) * u64::from(self.height) > 8_847_360
        {
            return Err(invalid(
                "Video dimensions must be even, between 64 and 4096, and at most 8.8 megapixels",
            ));
        }
        if !(1..=120).contains(&self.fps)
            || !(1..=100).contains(&self.bitrate_mbps)
            || !(64..=320).contains(&self.audio_bitrate_kbps)
        {
            return Err(invalid("Invalid video frame rate or bitrate"));
        }
        if !self.tail.is_finite()
            || !(0. ..=10.).contains(&self.tail)
            || !self.music.is_finite()
            || !(0. ..=1.).contains(&self.music)
            || !self.hitsounds.is_finite()
            || !(0. ..=1.).contains(&self.hitsounds)
        {
            return Err(invalid("Invalid export audio level or tail duration"));
        }
        let (start, end) = if self.whole_song {
            (0., song_end)
        } else {
            (self.start, self.end)
        };
        if !start.is_finite()
            || !end.is_finite()
            || start < 0.
            || end <= start
            || end + self.tail > 7200.
        {
            return Err(invalid(
                "Choose a video range between 0 and 7200 seconds, with end after start",
            ));
        }
        Ok(VideoPlan {
            width: self.width,
            height: self.height,
            fps: self.fps,
            frames: ((end + self.tail - start) * f64::from(self.fps)).ceil() as u64,
            start,
        })
    }
}
impl VideoPlan {
    pub fn time(&self, frame: u64) -> f64 {
        self.start + frame as f64 / f64::from(self.fps)
    }
    pub fn duration(&self) -> f64 {
        self.frames as f64 / f64::from(self.fps)
    }
    pub fn audio_frames(&self) -> u64 {
        (self.frames * 48_000).div_ceil(u64::from(self.fps))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn song_boundary_and_custom_range_use_exact_frame_and_sample_counts() {
        let mut s = VideoSettings {
            fps: 30,
            tail: 0.,
            ..Default::default()
        };
        let p = s.plan(100.125).unwrap();
        assert_eq!(p.frames, 3004);
        assert_eq!(p.audio_frames(), 4_806_400);
        s.whole_song = false;
        s.start = 4.25;
        s.end = 6.25;
        let p = s.plan(1000.).unwrap();
        assert_eq!(p.frames, 60);
        assert_eq!(p.time(59), 4.25 + 59. / 30.);
        assert_eq!(p.audio_frames(), 96000);
        for (w, h) in [(0, 720), (1919, 1080), (5000, 2160), (4096, 4096)] {
            s.width = w;
            s.height = h;
            assert!(s.plan(10.).is_err());
        }
        s.width = 3840;
        s.height = 2160;
        assert!(s.plan(10.).is_ok());
        s.end = f64::NAN;
        assert!(s.plan(10.).is_err());
    }
}
