use super::*;

impl Spec {
    pub fn compile(&self) -> Result<Compiled, ChartError> {
        if self.start < Beat::ZERO || self.end <= self.start || self.step <= Beat::ZERO {
            return Err(invalid(
                "Motion needs an ordered nonnegative range and positive step",
            ));
        }
        if self
            .center
            .iter()
            .chain(&self.radius)
            .chain(&self.scale)
            .chain(&self.trim)
            .chain(&self.parameter)
            .chain(&self.frequency)
            .chain(&self.tolerance)
            .chain([&self.rotation, &self.turns, &self.phase])
            .any(|x| !x.is_finite() || x.abs() > 1e7)
            || self.turns <= 0.
            || self.turns > 64.
            || self.frequency.iter().any(|v| *v <= 0. || *v > 64.)
            || self.trim[0] < 0.
            || self.trim[1] > 1.
            || self.trim[0] >= self.trim[1]
            || self.parameter[0] >= self.parameter[1]
            || self.easing.iter().any(|v| !(1..=29).contains(v))
            || self.tolerance.iter().any(|v| *v <= 0.)
        {
            return Err(invalid("Invalid trajectory parameters"));
        }
        let formulas = if matches!(self.shape, Shape::Formula | Shape::Polar) {
            let expressions = if self.shape == Shape::Polar {
                self.polar_expressions.as_ref().unwrap_or(&self.expressions)
            } else {
                &self.expressions
            };
            Some([
                Formula::parse(&expressions[0])?,
                Formula::parse(&expressions[1])?,
            ])
        } else {
            None
        };
        Ok(Compiled {
            spec: self.clone(),
            formulas,
        })
    }
}

impl Compiled {
    pub(super) fn local(&self, u: f64) -> Result<[f64; 2], ChartError> {
        let s = &self.spec;
        let a = TAU * s.turns * u;
        let (sn, cs) = a.sin_cos();
        let n = s.frequency[0];
        let m = s.frequency[1];
        let p = s.phase;
        let polar = |r: f64| [r * cs, r * sn];
        let polygon = |sides: f64| {
            let q = TAU * u;
            let sector = TAU / sides;
            let radial =
                (PI / sides).cos() / ((q + sector / 2.).rem_euclid(sector) - sector / 2.).cos();
            [radial * q.cos(), radial * q.sin()]
        };
        let mut v = match s.shape {
            Shape::Line => [2. * u - 1., 0.],
            Shape::Arc => [-(PI * u).cos(), (PI * u).sin()],
            Shape::Circle => [cs, sn],
            Shape::Ellipse => [cs, sn],
            Shape::Wave => [2. * u - 1., sn],
            Shape::Spiral => polar(u),
            Shape::LogSpiral => polar(((4. * u).exp() - 1.) / (4_f64.exp() - 1.)),
            Shape::Rose => polar((n * a).cos()),
            Shape::Lissajous => [(n * TAU * u).sin(), (m * TAU * u + p).sin()],
            Shape::Lemniscate => [cs / (1. + sn * sn), sn * cs / (1. + sn * sn)],
            Shape::Heart => [
                sn.powi(3),
                (13. * cs - 5. * (2. * a).cos() - 2. * (3. * a).cos() - (4. * a).cos()) / 16.,
            ],
            Shape::Cardioid => polar((1. - cs) / 2.),
            Shape::Astroid => [cs.powi(3), sn.powi(3)],
            Shape::Deltoid => [
                (2. * cs + (2. * a).cos()) / 3.,
                (2. * sn - (2. * a).sin()) / 3.,
            ],
            Shape::Nephroid => [
                (3. * cs - (3. * a).cos()) / 4.,
                (3. * sn - (3. * a).sin()) / 4.,
            ],
            Shape::Cycloid => [(a - sn - PI * s.turns) / (PI * s.turns), (1. - cs) / 2.],
            Shape::Epicycloid => [
                ((n + 1.) * cs - ((n + 1.) * a).cos()) / (n + 2.),
                ((n + 1.) * sn - ((n + 1.) * a).sin()) / (n + 2.),
            ],
            Shape::Hypocycloid => [
                ((n + 1.) * cs + ((n + 1.) * a).cos()) / (n + 2.),
                ((n + 1.) * sn - ((n + 1.) * a).sin()) / (n + 2.),
            ],
            Shape::Epitrochoid => [
                ((n + 1.) * cs - m * ((n + 1.) * a).cos()) / (n + 1. + m),
                ((n + 1.) * sn - m * ((n + 1.) * a).sin()) / (n + 1. + m),
            ],
            Shape::Hypotrochoid => [
                (n * cs + m * (n * a).cos()) / (n + m),
                (n * sn - m * (n * a).sin()) / (n + m),
            ],
            Shape::Superellipse => [
                cs.signum() * cs.abs().powf(2. / n),
                sn.signum() * sn.abs().powf(2. / n),
            ],
            Shape::Triangle => polygon(3.),
            Shape::Square => polygon(4.),
            Shape::Pentagon => polygon(5.),
            Shape::Hexagon => polygon(6.),
            Shape::Octagon => polygon(8.),
            Shape::Star => polar(0.65 + 0.35 * (n.round() * a).cos()),
            Shape::Rectangle => {
                let d = cs.abs().max(sn.abs());
                [cs / d, sn / d]
            }
            Shape::Diamond => {
                let d = cs.abs() + sn.abs();
                [cs / d, sn / d]
            }
            Shape::Zigzag => [2. * u - 1., 2. / PI * sn.asin()],
            Shape::RandomLoop => {
                let seed = f64::from(s.seed);
                polar(0.7 + 0.15 * (3. * a + seed).sin() + 0.15 * (5. * a + seed * 0.73).cos())
            }
            Shape::Butterfly => {
                let r = (a.sin().exp() - 2. * (4. * a).cos() - (a / 12.).sin().powi(5)) / 4.;
                [r * sn, r * cs]
            }
            Shape::Formula | Shape::Polar => {
                let t = s.parameter[0] + u * (s.parameter[1] - s.parameter[0]);
                let f = self.formulas.as_ref().unwrap();
                let x = f[0].sample(t)?;
                let y = f[1].sample(t)?;
                if s.shape == Shape::Polar {
                    [x * y.cos(), x * y.sin()]
                } else {
                    [x, y]
                }
            }
        };
        if !matches!(s.shape, Shape::Formula | Shape::Polar) {
            v[0] *= s.radius[0];
            v[1] *= if s.shape == Shape::Circle {
                s.radius[0]
            } else {
                s.radius[1]
            };
        }
        Ok(v)
    }

    pub fn point(&self, t: f64, ease: &impl Fn(i32, f64) -> f64) -> Result<[f64; 2], ChartError> {
        let s = &self.spec;
        let u = |id| s.trim[0] + ease(id, t) * (s.trim[1] - s.trim[0]);
        let a = self.local(u(s.easing[0]))?;
        let b = self.local(u(s.easing[1]))?;
        let (sn, cs) = (-s.rotation.to_radians()).sin_cos();
        let x = a[0] * s.scale[0];
        let y = b[1] * s.scale[1];
        let result = [s.center[0] + x * cs - y * sn, s.center[1] + x * sn + y * cs];
        if result.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
            return Err(invalid("Expression returned non-finite coordinates"));
        }
        Ok(result)
    }

    pub fn sample(&self, t: f64, ease: &impl Fn(i32, f64) -> f64) -> Result<[f64; 3], ChartError> {
        let p = self.point(t, ease)?;
        let a = self.point((t - 1e-5).max(0.), ease)?;
        let b = self.point((t + 1e-5).min(1.), ease)?;
        let angle = if (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-10 {
            0.
        } else {
            -(b[1] - a[1]).atan2(b[0] - a[0]).to_degrees()
        };
        Ok([p[0], p[1], angle])
    }
}
