use crate::batch::{self, Vertex};
use miniquad::{
    Bindings, BlendFactor, BlendState, BlendValue, Buffer, BufferLayout, BufferType, Context,
    Equation, GraphicsContext, Pipeline, PipelineParams, Shader, VertexAttribute, VertexFormat,
};

/// A callback function that can be used to compose an [`egui::PaintCallback`] for custom rendering
/// with [`egui-miniquad`].
///
/// The callback is passed, the [`egui::PaintCallbackInfo`] and the [`GraphicsContext`] which can be used to
/// access the OpenGL context.
///
/// # Example
///
/// See the [`custom3d_glow`](https://github.com/emilk/egui/blob/master/crates/egui_demo_app/src/apps/custom3d_wgpu.rs) demo source for a detailed usage example.
pub struct CallbackFn {
    #[allow(clippy::type_complexity)]
    f: Box<dyn Fn(egui::PaintCallbackInfo, &mut GraphicsContext) + Sync + Send>,
}

impl CallbackFn {
    pub fn new(
        callback: impl Fn(egui::PaintCallbackInfo, &mut GraphicsContext) + Sync + Send + 'static,
    ) -> Self {
        let f = Box::new(callback);
        CallbackFn { f }
    }
}

pub struct Painter {
    pipeline: Pipeline,
    bindings: Bindings,
    textures: std::collections::HashMap<egui::TextureId, miniquad::Texture>,
    pub stats: PaintStats,
    streams: [Vec<StreamBuffers>; 3],
    stream_frame: usize,
    stream_cursor: usize,
}

struct StreamBuffers {
    vertex: Buffer,
    index: Buffer,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PaintStats {
    pub draw_calls: usize,
    pub buffer_uploads: usize,
    pub vertices: usize,
}

impl Painter {
    pub fn new(ctx: &mut Context) -> Painter {
        let shader = Shader::new(ctx, shader::VERTEX, shader::FRAGMENT, shader::meta());

        let pipeline = Pipeline::with_params(
            ctx,
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("a_pos", VertexFormat::Float2),
                VertexAttribute::new("a_tc", VertexFormat::Float2),
                VertexAttribute::new("a_srgba", VertexFormat::Byte4),
                VertexAttribute::new("a_material", VertexFormat::Float2),
            ],
            shader.expect("couldn't make shader"),
            PipelineParams {
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::One,
                    BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
                )),
                cull_face: miniquad::CullFace::Nothing,
                ..Default::default()
            },
        );

        let vertex_buffer = Buffer::stream(ctx, BufferType::VertexBuffer, 1024);
        let index_buffer = Buffer::stream(ctx, BufferType::IndexBuffer, 1024);

        let bindings = Bindings {
            vertex_buffers: vec![vertex_buffer],
            index_buffer,
            images: vec![miniquad::Texture::empty(); batch::TEXTURE_SLOTS],
        };

        Painter {
            pipeline,
            bindings,
            textures: Default::default(),
            stats: PaintStats::default(),
            streams: [
                vec![StreamBuffers {
                    vertex: vertex_buffer,
                    index: index_buffer,
                }],
                vec![],
                vec![],
            ],
            stream_frame: 2,
            stream_cursor: 0,
        }
    }

    pub fn set_texture(
        &mut self,
        ctx: &mut Context,
        tex_id: egui::TextureId,
        delta: &egui::epaint::ImageDelta,
    ) {
        let [w, h] = delta.image.size();

        if let Some([x, y]) = delta.pos {
            // Partial update
            if let Some(texture) = self.textures.get(&tex_id) {
                match &delta.image {
                    egui::ImageData::Color(image) => {
                        assert_eq!(
                            image.width() * image.height(),
                            image.pixels.len(),
                            "Mismatch between texture size and texel count"
                        );
                        let data: &[u8] = bytemuck::cast_slice(image.pixels.as_ref());
                        texture.update_texture_part(ctx, x as _, y as _, w as _, h as _, data);
                    }
                    egui::ImageData::Font(image) => {
                        assert_eq!(
                            image.width() * image.height(),
                            image.pixels.len(),
                            "Mismatch between texture size and texel count"
                        );

                        let data: Vec<u8> = image
                            .srgba_pixels(None)
                            .flat_map(|a| a.to_array())
                            .collect();

                        texture.update_texture_part(ctx, x as _, y as _, w as _, h as _, &data);
                    }
                }
            } else {
                eprintln!("Failed to find egui texture {tex_id:?}");
            }
        } else {
            // New texture (or full update).
            let params = miniquad::TextureParams {
                format: miniquad::TextureFormat::RGBA8,
                wrap: miniquad::TextureWrap::Clamp,
                filter: miniquad::FilterMode::Linear,
                width: w as _,
                height: h as _,
            };

            let texture = match &delta.image {
                egui::ImageData::Color(image) => {
                    assert_eq!(
                        image.width() * image.height(),
                        image.pixels.len(),
                        "Mismatch between texture size and texel count"
                    );
                    let data: &[u8] = bytemuck::cast_slice(image.pixels.as_ref());
                    miniquad::Texture::from_data_and_format(ctx, data, params)
                }
                egui::ImageData::Font(image) => {
                    assert_eq!(
                        image.width() * image.height(),
                        image.pixels.len(),
                        "Mismatch between texture size and texel count"
                    );

                    let data: Vec<u8> = image
                        .srgba_pixels(None)
                        .flat_map(|a| a.to_array())
                        .collect();

                    miniquad::Texture::from_data_and_format(ctx, &data, params)
                }
            };

            let previous = self.textures.insert(tex_id, texture);
            if let Some(previous) = previous {
                previous.delete();
            }
        }
    }

    pub fn free_texture(&mut self, tex_id: egui::TextureId) {
        if let Some(old_tex) = self.textures.remove(&tex_id) {
            old_tex.delete();
        }
    }

    pub fn paint_and_update_textures(
        &mut self,
        ctx: &mut Context,
        primtives: Vec<egui::ClippedPrimitive>,
        textures_delta: &egui::TexturesDelta,
        egui_ctx: &egui::Context,
    ) {
        for (id, image_delta) in &textures_delta.set {
            self.set_texture(ctx, *id, image_delta);
        }

        self.paint(ctx, primtives, egui_ctx);

        for &id in &textures_delta.free {
            self.free_texture(id);
        }
    }

    pub fn paint(
        &mut self,
        ctx: &mut Context,
        primitives: Vec<egui::ClippedPrimitive>,
        egui_ctx: &egui::Context,
    ) {
        self.stats = PaintStats::default();
        self.stream_frame = (self.stream_frame + 1) % self.streams.len();
        self.stream_cursor = 0;
        // Keep the old missing-texture behavior local to its mesh.
        let primitives = primitives
            .into_iter()
            .filter(|p| {
                if let egui::epaint::Primitive::Mesh(mesh) = &p.primitive {
                    if matches!(mesh.texture_id, egui::TextureId::Managed(_))
                        && !self.textures.contains_key(&mesh.texture_id)
                    {
                        eprintln!("Texture {:?} not found", mesh.texture_id);
                        return false;
                    }
                }
                true
            })
            .collect();
        ctx.begin_default_pass(miniquad::PassAction::Nothing);
        ctx.apply_pipeline(&self.pipeline);
        let screen = ctx.screen_size();
        for item in batch::prepare(primitives) {
            match item {
                batch::Item::Mesh(batch) => self.paint_batch(ctx, batch, egui_ctx),
                batch::Item::Callback {
                    clip_rect,
                    callback,
                } => {
                    let info = egui::PaintCallbackInfo {
                        viewport: callback.rect,
                        clip_rect,
                        pixels_per_point: egui_ctx.pixels_per_point(),
                        screen_size_px: [screen.0.round() as _, screen.1.round() as _],
                    };
                    if let Some(callback) = callback.callback.downcast_ref::<CallbackFn>() {
                        (callback.f)(info, ctx);
                    } else {
                        eprintln!("Warning: Unsupported render callback. Expected egui_miniquad::CallbackFn");
                    }
                    ctx.apply_pipeline(&self.pipeline);
                    ctx.apply_viewport(0, 0, screen.0 as i32, screen.1 as i32);
                }
            }
        }
        ctx.end_render_pass();
    }

    fn paint_batch(&mut self, ctx: &mut Context, batch: batch::Batch, egui_ctx: &egui::Context) {
        let screen = ctx.screen_size();
        let scale = egui_ctx.pixels_per_point();
        let min_x = (scale * batch.clip_rect.min.x).clamp(0., screen.0).round() as i32;
        let min_y = (scale * batch.clip_rect.min.y).clamp(0., screen.1).round() as i32;
        let max_x = (scale * batch.clip_rect.max.x)
            .clamp(min_x as f32, screen.0)
            .round() as i32;
        let max_y = (scale * batch.clip_rect.max.y)
            .clamp(min_y as f32, screen.1)
            .round() as i32;
        if max_x <= min_x || max_y <= min_y || batch.indices.is_empty() {
            return;
        }
        let vertex_bytes = batch.vertices.len() * std::mem::size_of::<Vertex>();
        let index_bytes = batch.indices.len() * std::mem::size_of::<u16>();
        // Keep each draw separate and rotate three frames of streams, avoiding
        // same-frame overwrites and reducing stalls across consecutive frames.
        let streams = &mut self.streams[self.stream_frame];
        if self.stream_cursor == streams.len() {
            streams.push(StreamBuffers {
                vertex: Buffer::stream(
                    ctx,
                    BufferType::VertexBuffer,
                    vertex_bytes.next_power_of_two().max(1024),
                ),
                index: Buffer::stream(
                    ctx,
                    BufferType::IndexBuffer,
                    index_bytes.next_power_of_two().max(1024),
                ),
            });
        }
        let stream = &mut streams[self.stream_cursor];
        self.stream_cursor += 1;
        let retired_vertex = if stream.vertex.size() < vertex_bytes {
            Some(std::mem::replace(
                &mut stream.vertex,
                Buffer::stream(
                    ctx,
                    BufferType::VertexBuffer,
                    vertex_bytes.next_power_of_two(),
                ),
            ))
        } else {
            None
        };
        let retired_index = if stream.index.size() < index_bytes {
            Some(std::mem::replace(
                &mut stream.index,
                Buffer::stream(
                    ctx,
                    BufferType::IndexBuffer,
                    index_bytes.next_power_of_two(),
                ),
            ))
        } else {
            None
        };
        self.bindings.vertex_buffers[0] = stream.vertex;
        self.bindings.index_buffer = stream.index;
        stream.vertex.update(ctx, &batch.vertices);
        stream.index.update(ctx, &batch.indices);
        let texture = |id: egui::TextureId| match id {
            egui::TextureId::Managed(_) => self.textures[&id],
            egui::TextureId::User(id) => unsafe {
                miniquad::Texture::from_raw_id(id as u32, miniquad::TextureFormat::RGBA8)
            },
        };
        self.bindings.images.fill(texture(batch.textures[0]));
        for (slot, id) in batch.textures.iter().enumerate() {
            self.bindings.images[slot] = texture(*id);
        }
        ctx.apply_uniforms(&shader::Uniforms {
            u_screen_size: (screen.0 / scale, screen.1 / scale),
        });
        ctx.apply_scissor_rect(min_x, screen.1 as i32 - max_y, max_x - min_x, max_y - min_y);
        ctx.apply_bindings(&self.bindings);
        // Retire only after replacement attributes/bindings are installed.
        for buffer in [retired_vertex, retired_index].into_iter().flatten() {
            buffer.delete();
        }
        ctx.draw(0, batch.indices.len() as i32, 1);
        self.stats.draw_calls += 1;
        self.stats.buffer_uploads += 2;
        self.stats.vertices += batch.vertices.len();
    }
}

mod shader {
    use miniquad::{ShaderMeta, UniformBlockLayout, UniformDesc, UniformType};

    pub const VERTEX: &str = r#"
    #version 100
    uniform vec2 u_screen_size;

    attribute vec2 a_pos;
    attribute vec2 a_tc;
    attribute vec4 a_srgba;
    attribute vec2 a_material;

    varying vec2 v_tc;
    varying vec2 v_material;
    varying vec4 v_rgba_in_gamma;

    void main() {
        gl_Position = vec4(
            2.0 * a_pos.x / u_screen_size.x - 1.0,
            1.0 - 2.0 * a_pos.y / u_screen_size.y,
            0.0,
            1.0);
            v_rgba_in_gamma = a_srgba / 255.0;
            v_tc = a_tc;
            v_material = a_material;
    }
    "#;

    pub const FRAGMENT: &str = r#"
    #version 100
    uniform sampler2D u_sampler0;
    uniform sampler2D u_sampler1;
    uniform sampler2D u_sampler2;
    uniform sampler2D u_sampler3;
    precision highp float;

    varying vec2 v_tc;
    varying vec2 v_material;
    varying vec4 v_rgba_in_gamma;

    void main() {
        vec4 texture_in_gamma;
        if (v_material.x < 0.5) texture_in_gamma = texture2D(u_sampler0, v_tc);
        else if (v_material.x < 1.5) texture_in_gamma = texture2D(u_sampler1, v_tc);
        else if (v_material.x < 2.5) texture_in_gamma = texture2D(u_sampler2, v_tc);
        else texture_in_gamma = texture2D(u_sampler3, v_tc);
        texture_in_gamma.rgb *= mix(1.0, texture_in_gamma.a, step(0.5, v_material.y));
        gl_FragColor = v_rgba_in_gamma * texture_in_gamma;
    }
    "#;

    pub fn meta() -> ShaderMeta {
        ShaderMeta {
            images: (0..4).map(|i| format!("u_sampler{i}")).collect(),
            uniforms: UniformBlockLayout {
                uniforms: vec![UniformDesc::new("u_screen_size", UniformType::Float2)],
            },
        }
    }

    #[repr(C)]
    #[derive(Debug)]
    pub struct Uniforms {
        pub u_screen_size: (f32, f32),
    }
}
