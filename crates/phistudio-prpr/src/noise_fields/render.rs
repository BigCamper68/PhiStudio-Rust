//! BlockRender's mask, subtraction, dilation and composition passes, adapted
//! to the shared Macroquad context. Shader arithmetic and material values come
//! from the supplied Phigros build; see SOURCE.json.
use super::input::Input;
use anyhow::{Context, Result};
use macroquad::{
    miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams, UniformType},
    prelude::*,
};
use prpr::core::Resource;
use std::collections::BTreeMap;

const VERT: &str = "#version 100\nattribute vec3 position; attribute vec2 texcoord; attribute vec4 color0; varying vec2 uv; varying vec4 col; void main(){gl_Position=vec4(position,1.);uv=texcoord;col=color0/255.;}";
const SPRITE: &str = "#version 100\nprecision highp float; varying vec2 uv; varying vec4 col; uniform sampler2D Texture; void main(){gl_FragColor=texture2D(Texture,uv)*col;}";
const PACK: &str = "#version 100\nprecision highp float; varying vec2 uv; uniform sampler2D Active; uniform sampler2D Ready; uniform sampler2D Normal; uniform sampler2D Subtract; void main(){gl_FragColor=vec4(texture2D(Active,uv).r,texture2D(Ready,uv).r,texture2D(Normal,uv).r,texture2D(Subtract,uv).g);}";

struct Program {
    material: Material,
    uniforms: BTreeMap<String, UniformType>,
}
impl Program {
    fn new(vertex: &str, fragment: &str, blend: Option<BlendState>) -> Result<Self> {
        let mut uniforms = BTreeMap::new();
        let mut textures = Vec::new();
        // All extracted declarations use one uniform per statement. Skip Unity's
        // unused matrix arrays; the vertex adapter accepts clip-space geometry.
        for line in vertex.lines().chain(fragment.lines()) {
            for statement in line.split(';') {
                let Some(rest) = statement
                    .trim()
                    .strip_prefix("uniform ")
                    .or_else(|| statement.trim().strip_prefix("uniform\t"))
                else {
                    continue;
                };
                let words: Vec<_> = rest
                    .split_whitespace()
                    .filter(|w| !["highp", "mediump", "lowp"].contains(w))
                    .collect();
                let [kind, name] = words.as_slice() else {
                    continue;
                };
                if name.contains('[') {
                    continue;
                }
                if *kind == "sampler2D" {
                    if *name != "Texture" && !textures.iter().any(|s| s == name) {
                        textures.push((*name).to_owned());
                    }
                } else {
                    let ty = match *kind {
                        "float" => UniformType::Float1,
                        "int" => UniformType::Int1,
                        "vec2" => UniformType::Float2,
                        "vec3" => UniformType::Float3,
                        "vec4" => UniformType::Float4,
                        _ => continue,
                    };
                    uniforms.insert((*name).to_owned(), ty);
                }
            }
        }
        let material = load_material(
            vertex,
            fragment,
            MaterialParams {
                uniforms: uniforms.iter().map(|(n, t)| (n.clone(), *t)).collect(),
                textures,
                pipeline_params: PipelineParams {
                    color_blend: blend,
                    ..Default::default()
                },
            },
        )?;
        Ok(Self { material, uniforms })
    }
    fn float(&self, name: &str, value: f32) {
        if self.uniforms.contains_key(name) {
            self.material.set_uniform(name, value);
        }
    }
    fn vec4(&self, name: &str, value: Vec4) {
        if self.uniforms.contains_key(name) {
            self.material.set_uniform(name, value);
        }
    }
    fn properties(&self, value: &serde_json::Value) {
        for (name, ty) in &self.uniforms {
            if let Some(v) = value[name].as_f64() {
                self.float(name, v as f32);
            } else if let Some(v) = value[name].as_array().filter(|v| v.len() == 4) {
                let a = std::array::from_fn::<_, 4, _>(|i| v[i].as_f64().unwrap_or(0.) as f32);
                match ty {
                    UniformType::Float3 => self.material.set_uniform(name, vec3(a[0], a[1], a[2])),
                    UniformType::Float4 => self.vec4(name, Vec4::from_array(a)),
                    _ => {}
                }
            }
        }
    }
    fn pass(&self, target: RenderTarget, textures: &[(&str, Texture2D)], clear: bool) {
        camera(target);
        if clear {
            clear_background(BLANK);
        }
        let mut primary = None;
        for (name, texture) in textures {
            if *name == "Texture" {
                primary = Some(*texture);
            } else {
                self.material.set_texture(name, *texture);
            }
        }
        gl_use_material(self.material);
        mesh(
            [Vec2::ZERO, vec2(1., 0.), Vec2::ONE, vec2(0., 1.)],
            WHITE,
            primary,
        );
        gl_use_default_material();
        unsafe { get_internal_gl() }.flush();
    }
}
impl Drop for Program {
    fn drop(&mut self) {
        self.material.delete();
    }
}

struct Targets {
    size: (u32, u32),
    buffers: Vec<RenderTarget>,
}
impl Targets {
    fn new(w: u32, h: u32) -> Self {
        let (bw, bh) = ((w / 8).max(1), (h / 8).max(1));
        // Six camera masks; three resolved subtract masks; active/ready/disabled
        // composition; edge; packed masks; scene color.
        let buffers: Vec<_> = (0..16)
            .map(|i| {
                let (x, y) = match i {
                    12 => (bw * 2, bh * 2),
                    14 => ((w / 6).max(1), (h / 6).max(1)),
                    _ => (bw, bh),
                };
                let rt = render_target(x, y);
                rt.texture.set_filter(if i == 12 {
                    FilterMode::Linear
                } else {
                    FilterMode::Nearest
                });
                rt
            })
            .collect();
        Self {
            size: (w, h),
            buffers,
        }
    }
}
impl Drop for Targets {
    fn drop(&mut self) {
        unsafe { get_internal_gl() }.flush();
        for rt in &self.buffers {
            rt.delete();
        }
    }
}

pub(super) struct Renderer {
    programs: Vec<Program>,
    sprite: Program,
    xor_sprite: Program,
    resolve_xor: Program,
    pack: Program,
    color_pack: Program,
    colors: super::colors::Colors,
    targets: Option<Targets>,
    textures: [Texture2D; 4],
    hover: Program,
    hover_vertices: Vec<[f32; 4]>,
    hover_indices: Vec<u16>,
    touch_shine: [f32; 3],
}
impl Renderer {
    pub(super) fn new() -> Result<Self> {
        let over = Some(BlendState::new(
            Equation::Add,
            BlendFactor::One,
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        ));
        let additive = Some(BlendState::new(
            Equation::Add,
            BlendFactor::One,
            BlendFactor::One,
        ));
        macro_rules! shader {
            ($name:literal,$blend:expr) => {
                Program::new(
                    include_str!(concat!("shaders/", $name, ".vert")),
                    include_str!(concat!("shaders/", $name, ".frag")),
                    $blend,
                )
                .context($name)?
            };
        }
        let programs = vec![
            shader!("SubtractBlockBlender_0", None),
            shader!("SubtractBlockBlender_1", None),
            shader!("BlockCompose_0", None),
            shader!("BlockCompose_1", None),
            shader!("EdgeMask_1", None),
            shader!("DisabledBlock_0", additive),
            shader!("ActiveBlock_0", over),
            Program::new(
                include_str!("shaders/DisabledBlock_0.vert"),
                &super::colors::material(include_str!("shaders/DisabledBlock_0.frag"), false),
                additive,
            )
            .context("Colored DisabledBlock")?,
            Program::new(
                include_str!("shaders/ActiveBlock_0.vert"),
                &super::colors::material(include_str!("shaders/ActiveBlock_0.frag"), true),
                over,
            )
            .context("Colored ActiveBlock")?,
        ];
        let props: serde_json::Value = serde_json::from_str(include_str!("assets/materials.json"))?;
        programs[2].properties(&props["6"]);
        programs[5].properties(&props["8"]);
        programs[7].properties(&props["8"]);
        // BlockRender.CopyReadyTouchParamsToActive copies these at Start.
        for active in [&programs[6], &programs[8]] {
            active.properties(&props["5"]);
            for name in ["_ShineSpeed", "_ShineBrightness", "_ShineColor"] {
                active.properties(&serde_json::json!({name: props["10"][name]}));
            }
            for (source, target) in [
                ("_DisplaceMap_ST", "_TouchDisplaceMap_ST"),
                ("_DisplaceSpeed", "_TouchDisplaceSpeed"),
                ("_DisplaceStrength", "_TouchDisplaceStrength"),
                ("_DisplaceDirection", "_TouchDisplaceDirection"),
                ("_GlowColor", "_TouchGlowColor"),
                ("_BackgroundPixelScale", "_TouchBackgroundPixelScale"),
            ] {
                active.properties(&serde_json::json!({target: props["12"][source]}));
            }
            active.properties(&props["12"]);
            // The unprefixed displacement properties above belong to ActiveBlock.
            for name in [
                "_DisplaceMap_ST",
                "_DisplaceSpeed",
                "_DisplaceStrength",
                "_DisplaceDirection",
                "_BackgroundPixelScale",
            ] {
                active.properties(&serde_json::json!({name: props["5"][name]}));
            }
        }
        let touch_shine = [
            "_TouchPosShineSpeed",
            "_TouchPosLowThreshold",
            "_TouchPosBrightness",
        ]
        .map(|name| props["5"][name].as_f64().unwrap_or(0.) as f32);
        #[derive(serde::Deserialize)]
        struct Sprite {
            vertices: Vec<[f32; 4]>,
            indices: Vec<u16>,
        }
        let hover_sprite: Sprite = serde_json::from_str(include_str!("assets/hover-sprite.json"))?;
        for p in &programs[..2] {
            p.float("_ClampThresholdLow", 0.09);
            p.float("_ClampThresholdHigh", 0.12);
        }
        let textures = [
            Texture2D::from_file_with_format(
                include_bytes!("assets/BlockNoise1.png"),
                Some(ImageFormat::Png),
            ),
            Texture2D::from_file_with_format(
                include_bytes!("assets/PointNoise.png"),
                Some(ImageFormat::Png),
            ),
            Texture2D::from_file_with_format(
                include_bytes!("assets/FD_Noise_00000.png"),
                Some(ImageFormat::Png),
            ),
            Texture2D::from_file_with_format(
                include_bytes!("assets/Round10_Blur4.png"),
                Some(ImageFormat::Png),
            ),
        ];
        use macroquad::miniquad::TextureWrap;
        // Original Texture2D import settings, recorded in SOURCE.json.
        for (tex, filter, wrap) in [
            (textures[0], FilterMode::Nearest, TextureWrap::Mirror),
            (textures[1], FilterMode::Nearest, TextureWrap::Repeat),
            (textures[2], FilterMode::Nearest, TextureWrap::Mirror),
            (textures[3], FilterMode::Linear, TextureWrap::Clamp),
        ] {
            tex.set_filter(filter);
            tex.raw_miniquad_texture_handle()
                .set_wrap(unsafe { get_internal_gl() }.quad_context, wrap);
        }
        Ok(Self {
            programs,
            sprite: Program::new(
                VERT,
                SPRITE,
                Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::One,
                )),
            )?,
            xor_sprite: Program::new(
                VERT,
                SPRITE,
                Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::OneMinusValue(BlendValue::DestinationColor),
                    BlendFactor::OneMinusValue(BlendValue::SourceColor),
                )),
            )?,
            resolve_xor: Program::new(
                VERT,
                "#version 100\nprecision highp float; varying vec2 uv; uniform sampler2D _MainTex; void main(){float v=texture2D(_MainTex,uv).r; gl_FragColor=vec4(v,v,0.0,1.0);}",
                None,
            )?,
            hover: Program::new(
                VERT,
                "#version 100\nprecision highp float; varying vec2 uv; uniform sampler2D Texture; void main(){vec4 c=texture2D(Texture,uv); c.rgb*=c.a; gl_FragColor=c;}",
                over,
            )?,
            hover_vertices: hover_sprite.vertices,
            hover_indices: hover_sprite.indices,
            touch_shine,
            pack: Program::new(VERT, PACK, None)?,
            color_pack: Program::new(VERT, super::colors::PACK, None)?,
            colors: Default::default(),
            targets: None,
            textures,
        })
    }

    pub(super) fn render(
        &mut self,
        res: &mut Resource,
        areas: &[phistudio_core::noise_fields::Sample],
        input: &Input,
        full: (i32, i32, i32, i32),
        xor: bool,
        single: bool,
    ) {
        let t = res.time as f32;
        if areas.is_empty() && input.hovers.iter().all(|h| h.scale == 0.) {
            return;
        }
        let Some(target) = res.chart_target.as_mut() else {
            return;
        };
        unsafe { get_internal_gl() }.flush();
        target.swap();
        let source = target.old().texture;
        let output = target.output();
        let (w, h) = (source.width() as u32, source.height() as u32);
        if self.targets.as_ref().is_none_or(|r| r.size != (w, h)) {
            self.targets = Some(Targets::new(w, h));
        }
        let Some(targets) = &self.targets else { return };
        let rt = &targets.buffers;
        let cv = res.camera.viewport.unwrap_or(full);
        let vp = (cv.0 - full.0, cv.1 - full.1, cv.2, cv.3);
        push_camera_state();
        unsafe { get_internal_gl() }.quad_gl.scissor(None);
        let time = vec4(t / 20., t, t * 2., t * 3.);
        for p in &self.programs {
            p.vec4("_BlockTime", time);
        }
        for (i, buffer) in rt.iter().enumerate().take(6) {
            camera(*buffer);
            clear_background(BLANK);
            gl_use_material(if xor && i % 2 == 1 {
                self.xor_sprite.material
            } else {
                self.sprite.material
            });
            for area in areas.iter() {
                let phase = if area.phase == 2 {
                    0
                } else if area.phase == 1 {
                    2
                } else {
                    4
                };
                // Disabled cameras include the Ready layer (original culling
                // masks 40960 / 81920). Ready adds shine over the red base.
                if phase + usize::from(area.subtract) != i
                    && !(phase == 2 && i == 4 + usize::from(area.subtract))
                {
                    continue;
                }
                let fade = area.opacity;
                let points = area
                    .corners()
                    .map(|p| project_point(p, res.aspect_ratio, res.config.flip_x(), vp, (w, h)));
                if points.iter().all(|p| p.is_finite()) {
                    mesh(
                        points,
                        Color::new(
                            1.,
                            if area.subtract { fade } else { 1. },
                            1.,
                            if area.subtract {
                                if xor { 1. } else { 0.1 }
                            } else {
                                fade
                            },
                        ),
                        None,
                    );
                }
            }
            gl_use_default_material();
            unsafe { get_internal_gl() }.flush();
        }
        for (program, out, input) in [(0, 6, 1), (1, 7, 3), (1, 8, 5)] {
            let p = if xor {
                &self.resolve_xor
            } else {
                &self.programs[program]
            };
            p.pass(rt[out], &[("_MainTex", rt[input].texture)], true);
        }
        self.programs[2].pass(
            rt[9],
            &[
                ("_NormalBlockRT", rt[0].texture),
                ("_SubtractBlockRT", rt[6].texture),
                ("_DisplaceMap", self.textures[0]),
            ],
            true,
        );
        for (out, n, s) in [(10, 2, 7), (11, 4, 8)] {
            self.programs[3].pass(
                rt[out],
                &[
                    ("_DisabledNormalBlockRT", rt[n].texture),
                    ("_DisabledSubtractBlockRT", rt[s].texture),
                ],
                true,
            );
        }
        let (ew, eh) = (rt[12].texture.width(), rt[12].texture.height());
        let texel = vec4(1. / ew, 1. / eh, ew, eh);
        self.programs[4].vec4("_DilateTexelSize", texel);
        // The build 160 gameplay prefab sets edgeSize=1. RenderEffects therefore
        // goes directly to EdgeMask pass 1, without intermediate dilation.
        let composed = rt[9].texture;
        self.programs[4].pass(
            rt[12],
            &[("_MainTex", composed), ("_ComposeRT", rt[9].texture)],
            true,
        );
        camera(rt[15]);
        clear_background(BLANK);
        gl_use_material(self.hover.material);
        for hover in input.hovers.iter().filter(|h| h.scale > 0.) {
            let vertices: Vec<_> = self
                .hover_vertices
                .iter()
                .map(|v| {
                    let p = hover.position + vec2(v[0], v[1]) * hover.scale;
                    let p = project_point(p, res.aspect_ratio, res.config.flip_x(), vp, (w, h));
                    macroquad::models::Vertex {
                        position: vec3(p.x * 2. - 1., p.y * 2. - 1., 0.),
                        uv: vec2(v[2], v[3]),
                        color: WHITE,
                    }
                })
                .collect();
            let gl = unsafe { get_internal_gl() }.quad_gl;
            gl.texture(Some(self.textures[3]));
            gl.draw_mode(DrawMode::Triangles);
            gl.geometry(&vertices, &self.hover_indices);
        }
        gl_use_default_material();
        unsafe { get_internal_gl() }.flush();
        let palette = self.colors.update(
            areas,
            res.aspect_ratio,
            res.config.flip_x(),
            vp,
            (w, h),
            single || !xor,
        );
        let mut masks = vec![
            ("Active", rt[9].texture),
            ("Ready", rt[10].texture),
            ("Normal", rt[4].texture),
            ("Subtract", rt[8].texture),
        ];
        let pack = if palette.is_some() {
            masks.push(("Hover", rt[15].texture));
            &self.color_pack
        } else {
            &self.pack
        };
        pack.pass(rt[13], &masks, true);
        camera(rt[14]);
        clear_background(BLANK);
        gl_use_default_material();
        mesh(
            [Vec2::ZERO, vec2(1., 0.), Vec2::ONE, vec2(0., 1.)],
            WHITE,
            Some(source),
        );
        unsafe { get_internal_gl() }.flush();
        camera(output);
        clear_background(BLANK);
        mesh(
            [Vec2::ZERO, vec2(1., 0.), Vec2::ONE, vec2(0., 1.)],
            WHITE,
            Some(source),
        );
        unsafe { get_internal_gl() }.flush();
        if let Some(texture) = palette {
            self.programs[7].material.set_texture("_Palette", texture);
            self.programs[8].material.set_texture("_Palette", texture);
        }
        self.programs[if palette.is_some() { 7 } else { 5 }].pass(
            output,
            &[
                ("_ComposeRT", rt[11].texture),
                ("_DisplaceMap", self.textures[0]),
                ("_SparkMap", self.textures[1]),
            ],
            false,
        );
        let p = &self.programs[if palette.is_some() { 8 } else { 6 }];
        p.material
            .set_uniform("_TouchPosCount", input.positions.len() as i32);
        for (i, position) in input.positions.iter().enumerate() {
            let uv = project_point(*position, res.aspect_ratio, res.config.flip_x(), vp, (w, h));
            p.vec4(
                &format!("_TouchPos{i}"),
                vec4(uv.x * w as f32 / h as f32, uv.y, 0., 0.),
            );
        }
        let [speed, low, brightness] = self.touch_shine;
        p.float(
            "_TouchPosShine",
            (low + (1. - low) * ((t * speed).sin() * 0.5 + 0.5)) * brightness,
        );
        p.vec4(
            "_ScreenParams",
            vec4(w as f32, h as f32, 1. + 1. / w as f32, 1. + 1. / h as f32),
        );
        p.vec4("_ProjectionParams", vec4(1., 0.1, 100., 0.01));
        p.vec4("_EffectRT_TexelSize", vec4(1. / ew, 1. / eh, ew, eh));
        let mut textures = vec![
            ("_MaskRT", rt[13].texture),
            ("_EffectRT", rt[12].texture),
            ("Texture", self.textures[0]),
            ("_SparkMap", self.textures[1]),
            ("_NoiseMap", self.textures[2]),
            ("_SceneColor", rt[14].texture),
        ];
        if palette.is_none() {
            textures.push(("_TouchHoverRT", rt[15].texture));
        }
        p.pass(output, &textures, false);
        pop_camera_state();
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe { get_internal_gl() }.flush();
        for tex in self.textures {
            tex.delete();
        }
    }
}
fn camera(target: RenderTarget) {
    set_camera(&Camera2D {
        zoom: vec2(1., 1.),
        render_target: Some(target),
        ..Default::default()
    });
}
fn mesh(points: [Vec2; 4], color: Color, texture: Option<Texture2D>) {
    let uv = [Vec2::ZERO, vec2(1., 0.), Vec2::ONE, vec2(0., 1.)];
    let vertices = std::array::from_fn::<_, 4, _>(|i| macroquad::models::Vertex {
        position: vec3(points[i].x * 2. - 1., points[i].y * 2. - 1., 0.),
        uv: uv[i],
        color,
    });
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.texture(texture);
    gl.draw_mode(DrawMode::Triangles);
    gl.geometry(&vertices, &[0, 1, 2, 0, 2, 3]);
}
pub(super) fn project_point(
    p: Vec2,
    aspect: f32,
    flip: bool,
    vp: (i32, i32, i32, i32),
    size: (u32, u32),
) -> Vec2 {
    let x = p.x / (5. * aspect) * if flip { -1. } else { 1. };
    let y = p.y / 5.;
    vec2(
        (vp.0 as f32 + (x + 1.) * 0.5 * vp.2 as f32) / size.0 as f32,
        (vp.1 as f32 + (y + 1.) * 0.5 * vp.3 as f32) / size.1 as f32,
    )
}
