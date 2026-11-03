use phistudio_sdk::*;
use std::collections::BTreeMap;

const SOURCE: &str = "#version 100\nprecision mediump float;\nvarying vec2 uv;\nuniform sampler2D screenTexture;\nuniform float time;\nuniform float amount; // %0.5%\nuniform vec2 center; // %0.5,0.5%\nuniform vec4 tint; // %0.2,0.8,1.0,1.0%\nvoid main(){vec4 src=texture2D(screenTexture,uv);float wave=0.5+0.5*sin(time+length(uv-center)*12.0);gl_FragColor=vec4(mix(src.rgb,tint.rgb,amount*wave*0.35),src.a);}";
struct Fx {
    enabled: bool,
    custom: bool,
    broken: bool,
    strength: f64,
}
impl Fx {
    fn panel(&self) -> Effect {
        Effect::Panel {
            id: "settings".into(),
            widgets: vec![
                Widget::Bool {
                    id: "enabled".into(),
                    text: "Enable plugin preview effect".into(),
                    value: self.enabled,
                },
                Widget::Bool {
                    id: "custom".into(),
                    text: "Use custom GLSL".into(),
                    value: self.custom,
                },
                Widget::Number {
                    id: "strength".into(),
                    text: "Effect strength".into(),
                    value: self.strength,
                    min: 0.,
                    max: 1.,
                },
                Widget::Bool {
                    id: "broken".into(),
                    text: "Test a shader compile error".into(),
                    value: self.broken,
                },
                Widget::Label {
                    text: "Preview only; this effect is not saved into the chart.".into(),
                },
            ],
        }
    }
    fn passes(&self) -> Vec<PreviewPass> {
        if !self.enabled {
            return vec![];
        }
        let (shader, uniforms) = if self.broken {
            (
                PreviewShader::Fragment {
                    source: "intentionally invalid GLSL".into(),
                },
                BTreeMap::new(),
            )
        } else if self.custom {
            (
                PreviewShader::Fragment {
                    source: SOURCE.into(),
                },
                BTreeMap::from([
                    ("amount".into(), ShaderUniform::Float(self.strength as f32)),
                    ("center".into(), ShaderUniform::Vec2([0.5, 0.5])),
                    ("tint".into(), ShaderUniform::Vec4([0.2, 0.8, 1., 1.])),
                ]),
            )
        } else {
            (
                PreviewShader::Preset {
                    name: "grayscale".into(),
                },
                BTreeMap::from([("factor".into(), ShaderUniform::Float(self.strength as f32))]),
            )
        };
        vec![PreviewPass {
            id: "grade".into(),
            shader,
            uniforms,
        }]
    }
}
impl Plugin for Fx {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::PreviewEffect {
                        id: "grade".into(),
                        priority: 10,
                    },
                    Contribution::Panel {
                        id: "settings".into(),
                        title: "Preview FX".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    },
                ],
                effects: vec![self.panel()],
                ..Default::default()
            },
            Request::Ui { event, .. } => {
                match event.widget.as_str() {
                    "enabled" => {
                        if let Some(v) = event.value.as_bool() {
                            self.enabled = v;
                        }
                    }
                    "custom" => {
                        if let Some(v) = event.value.as_bool() {
                            self.custom = v;
                        }
                    }
                    "broken" => {
                        if let Some(v) = event.value.as_bool() {
                            self.broken = v;
                        }
                    }
                    "strength" => {
                        if let Some(v) = event.value.as_f64().filter(|v| v.is_finite()) {
                            self.strength = v.clamp(0., 1.);
                        }
                    }
                    _ => {}
                }
                Response {
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::Render { frame, .. } if frame.target.kind == RenderKind::PreviewEffect => {
                Response {
                    effects: vec![Effect::PreviewEffects {
                        frame: frame.token,
                        passes: self.passes(),
                    }],
                    ..Default::default()
                }
            }
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(|| Fx {
    enabled: true,
    custom: false,
    broken: false,
    strength: 0.5
});
