//! Optional color variants preserve the extracted red materials. Color ownership
//! follows surviving union/XOR regions, then extends into displaced edges.
use macroquad::prelude::*;
use phistudio_core::noise_fields::{DEFAULT_COLOR, Sample};
#[derive(Default)]
pub(super) struct Colors {
    texture: Option<Texture2D>,
    size: (u16, u16),
    pub pixels: Vec<u8>,
}
impl Colors {
    pub fn update(
        &mut self,
        areas: &[Sample],
        aspect: f32,
        flip: bool,
        vp: (i32, i32, i32, i32),
        size: (u32, u32),
        single: bool,
    ) -> Option<Texture2D> {
        if !areas.iter().any(|a| a.color != DEFAULT_COLOR) {
            return None;
        }
        let (w, h) = ((size.0 / 8).max(1) as usize, (size.1 / 8).max(1) as usize);
        self.pixels.resize(w * h * 8, 0);
        for layer in 0..2 {
            let mut seeds = vec![usize::MAX; w * h];
            let mut owners = vec![usize::MAX; w * h];
            let mut normals = vec![usize::MAX; w * h];
            let mut subtracts = vec![usize::MAX; w * h];
            let mut counts = vec![0_u32; w * h];
            let mirror = if flip { -1. } else { 1. };
            let pixel_world = |x: f32, y: f32| {
                vec2(
                    (((x + 0.5) / w as f32 * size.0 as f32 - vp.0 as f32) / vp.2 as f32 * 2. - 1.)
                        * 5.
                        * aspect
                        * mirror,
                    (((y + 0.5) / h as f32 * size.1 as f32 - vp.1 as f32) / vp.3 as f32 * 2. - 1.)
                        * 5.,
                )
            };
            for (id, a) in areas.iter().enumerate().filter(|(_, a)| {
                (a.phase == 2) == (layer == 0)
                    && a.opacity > 0.
                    && a.pose.size.x > 0.0001
                    && a.pose.size.y > 0.0001
            }) {
                let corners = a.corners().map(|p| {
                    super::render::project_point(p, aspect, flip, vp, size)
                        * vec2(w as f32, h as f32)
                });
                let low = corners
                    .iter()
                    .copied()
                    .fold(vec2(w as f32, h as f32), Vec2::min)
                    .floor()
                    .max(Vec2::ZERO);
                let high = corners
                    .iter()
                    .copied()
                    .fold(Vec2::ZERO, Vec2::max)
                    .ceil()
                    .min(vec2(w as f32, h as f32));
                let (sin, cos) = a.pose.angle.sin_cos();
                for y in low.y as usize..high.y as usize {
                    for x in low.x as usize..high.x as usize {
                        let d =
                            (pixel_world(x as f32, y as f32) - a.pose.center) / vec2(1., a.y_scale);
                        let p = vec2(cos * d.x + sin * d.y, -sin * d.x + cos * d.y).abs();
                        if p.x <= a.pose.size.x * 0.5 && p.y <= a.pose.size.y * 0.5 {
                            let i = y * w + x;
                            if a.subtract {
                                counts[i] += 1;
                                subtracts[i] = id;
                            } else {
                                normals[i] = id;
                            }
                        }
                    }
                }
            }
            for i in 0..w * h {
                let inverted = if single {
                    counts[i] == 1
                } else {
                    counts[i] % 2 == 1
                };
                if (normals[i] != usize::MAX) != inverted {
                    seeds[i] = i;
                    owners[i] = if normals[i] != usize::MAX {
                        normals[i]
                    } else {
                        subtracts[i]
                    };
                }
            }
            for reverse in [false, true] {
                for n in 0..w * h {
                    let i = if reverse { w * h - 1 - n } else { n };
                    if seeds[i] == i {
                        continue;
                    }
                    let (x, y) = (i % w, i / w);
                    let step = if reverse { 1_isize } else { -1 };
                    let distance = |seed: usize| {
                        if seed == usize::MAX {
                            usize::MAX
                        } else {
                            x.abs_diff(seed % w).pow(2) + y.abs_diff(seed / w).pow(2)
                        }
                    };
                    for (dx, dy) in [(step, 0), (-1, step), (0, step), (1, step)] {
                        let (nx, ny) = (x as isize + dx, y as isize + dy);
                        if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                            continue;
                        }
                        let candidate = seeds[ny as usize * w + nx as usize];
                        if distance(candidate) < distance(seeds[i]) {
                            seeds[i] = candidate;
                        }
                    }
                }
            }
            for (i, seed) in seeds.iter().enumerate() {
                let rgb = if *seed == usize::MAX {
                    DEFAULT_COLOR
                } else {
                    areas[owners[*seed]].color
                };
                let offset = (layer * w * h + i) * 4;
                self.pixels[offset..offset + 3].copy_from_slice(&rgb);
                self.pixels[offset + 3] = 255;
            }
        }
        let dimensions = (w as u16, (h * 2) as u16);
        let image = Image {
            width: dimensions.0,
            height: dimensions.1,
            bytes: self.pixels.clone(),
        };
        if self.size != dimensions {
            if let Some(t) = self.texture.take() {
                t.delete();
            }
            let t = Texture2D::from_image(&image);
            t.set_filter(FilterMode::Nearest);
            self.texture = Some(t);
            self.size = dimensions;
        } else if let Some(t) = self.texture {
            t.update(&image);
        }
        self.texture
    }
}
impl Drop for Colors {
    fn drop(&mut self) {
        if let Some(t) = self.texture {
            t.delete();
        }
    }
}
/// Color-space mapping follows PhiStudio Java's NoiseFieldColorShaders at
/// bb2e9b3 (MIT); source assets and native coverage arithmetic stay unchanged.
pub(super) fn material(source: &str, active: bool) -> String {
    let helper = r#"
uniform sampler2D _Palette;
vec3 noiseTint(vec3 material,vec3 rgb){
    if(all(lessThan(abs(rgb-vec3(1.0,84.0/255.0,84.0/255.0)),vec3(0.001))))return material;
    float value=max(max(rgb.r,rgb.g),rgb.b), low=min(min(rgb.r,rgb.g),rgb.b), chroma=value-low;
    if(chroma<0.00001)return vec3(material.r*value);
    vec3 hue=(rgb-vec3(low))/chroma;
    float saturation=chroma/max(value,0.00001)/(1.0-84.0/255.0);
    float s=clamp((1.0-material.g/max(material.r,0.00001))*saturation,0.0,1.0);
    return material.r*value*mix(vec3(1.0),hue,s);
}
vec3 noiseRgb;
"#;
    let mut result = source.to_owned();
    for uniform in [
        "_EdgeColor",
        "_FillColor",
        "_SparkTint",
        "_NoiseTint",
        "_TouchGlowColor",
    ] {
        result = result.replace(
            &format!("{uniform}.xyz"),
            &format!("noiseTint({uniform}.xyz,noiseRgb)"),
        );
    }
    let setup = if active {
        // Colored masks precompose the ready multiplier and pack hover in B.
        result = result.replace("uniform mediump sampler2D _TouchHoverRT;", "");
        result = result
            .replace("texture2D(_MaskRT, vs_TEXCOORD0.xy).b;", "0.0;")
            .replace("texture2D(_MaskRT, vs_TEXCOORD0.xy).a;", "1.0;");
        for uv in ["vs_TEXCOORD0.xy", "u_xlat0.xy"] {
            result = result.replace(
                &format!("texture2D(_TouchHoverRT, {uv}).x"),
                &format!("texture2D(_MaskRT, {uv}).b"),
            );
        }
        "float layer=texture2D(_MaskRT,vs_TEXCOORD0.xy).r+texture2D(_EffectRT,vs_TEXCOORD0.xy).r>0.0001?0.0:0.5; noiseRgb=texture2D(_Palette,vec2(vs_TEXCOORD0.x,vs_TEXCOORD0.y*0.5+layer)).rgb;"
    } else {
        "noiseRgb=texture2D(_Palette,vec2(vs_TEXCOORD0.x,vs_TEXCOORD0.y*0.5+0.5)).rgb;"
    };
    result
        .replace("void main()", &format!("{helper}\nvoid main()"))
        .replace("void main()\n{", &format!("void main()\n{{\n{setup}\n"))
}
pub(super) const PACK: &str = "#version 100\nprecision highp float; varying vec2 uv; uniform sampler2D Active; uniform sampler2D Ready; uniform sampler2D Normal; uniform sampler2D Subtract; uniform sampler2D Hover; void main(){gl_FragColor=vec4(texture2D(Active,uv).r,texture2D(Ready,uv).r*abs(texture2D(Normal,uv).r-texture2D(Subtract,uv).g),texture2D(Hover,uv).r,0.0);}";
