use phistudio_sdk::*;

struct Guides {
    visible: bool,
}
fn themed(color: ThemeColor) -> DrawColor {
    DrawColor::Theme(color)
}
fn line(from: [f64; 2], to: [f64; 2], color: ThemeColor) -> DrawPrimitive {
    DrawPrimitive::Line {
        from,
        to,
        color: themed(color),
        width: 1.,
    }
}
impl Guides {
    fn panel(&self) -> Effect {
        Effect::Panel {
            id: "settings".into(),
            widgets: vec![
                Widget::Bool {
                    id: "visible".into(),
                    text: "Show plugin guides".into(),
                    value: self.visible,
                },
                Widget::Canvas {
                    height: 72.,
                    layers: vec![DrawLayer {
                        space: DrawSpace::Normalized,
                        primitives: (0..80)
                            .map(|i| {
                                let x = i as f64 / 79.;
                                DrawPrimitive::Circle {
                                    center: [x, 0.5 - 0.35 * (x * std::f64::consts::TAU).sin()],
                                    radius: 2.,
                                    color: themed(ThemeColor::Accent),
                                }
                            })
                            .collect(),
                    }],
                },
            ],
        }
    }
    fn preview(&self, context: &EditorContext) -> Vec<DrawLayer> {
        vec![DrawLayer {
            space: DrawSpace::Normalized,
            primitives: vec![
                line([0.5, 0.], [0.5, 1.], ThemeColor::Accent),
                line([0., 0.5], [1., 0.5], ThemeColor::Accent),
                DrawPrimitive::Circle {
                    center: [
                        0.5 + 0.2 * (context.beat * std::f64::consts::TAU).sin(),
                        0.5,
                    ],
                    radius: 4.,
                    color: themed(ThemeColor::Flick),
                },
            ],
        }]
    }
    fn track(&self, viewport: &Viewport) -> Vec<DrawLayer> {
        let Some(t) = &viewport.timeline else {
            return vec![];
        };
        let first = (t.beat
            + f64::from(t.baseline - viewport.size[1]) / f64::from(t.pixels_per_beat))
        .floor() as i64;
        let count = (viewport.size[1] / t.pixels_per_beat)
            .ceil()
            .clamp(0., 1000.) as i64
            + 2;
        let mut points = vec![];
        for beat in first..first.saturating_add(count) {
            if beat >= 0 {
                points.push(DrawPrimitive::Circle {
                    center: [250. * (beat as f64 * 0.7).sin(), beat as f64],
                    radius: 4.,
                    color: themed(ThemeColor::Tap),
                });
            }
        }
        vec![
            DrawLayer {
                space: DrawSpace::Timeline,
                primitives: points,
            },
            DrawLayer {
                space: DrawSpace::Points,
                primitives: vec![DrawPrimitive::Text {
                    position: [8., 8.],
                    text: "Plugin beat guide".into(),
                    size: 12.,
                    color: themed(ThemeColor::Text),
                }],
            },
        ]
    }
}
impl Plugin for Guides {
    fn call(&mut self, request: &Request) -> Result<Response, String> {
        Ok(match request {
            Request::Activate { .. } => Response {
                contributions: vec![
                    Contribution::Preview {
                        id: "center".into(),
                    },
                    Contribution::Track {
                        id: "beats".into(),
                        title: "Plugin beat guide".into(),
                    },
                    Contribution::Panel {
                        id: "settings".into(),
                        title: "Guide settings".into(),
                        location: PanelLocation::Window,
                        snapshot: false,
                    },
                ],
                effects: vec![self.panel()],
                ..Default::default()
            },
            Request::Ui { event, .. } if event.widget == "visible" => {
                if let Some(value) = event.value.as_bool() {
                    self.visible = value;
                }
                Response {
                    effects: vec![self.panel()],
                    ..Default::default()
                }
            }
            Request::Render { frame, context } => Response {
                effects: vec![Effect::Draw {
                    frame: frame.token,
                    layers: if !self.visible {
                        vec![]
                    } else {
                        match frame.target.kind {
                            RenderKind::Preview => self.preview(context),
                            RenderKind::Track => self.track(&frame.viewport),
                            RenderKind::PreviewEffect => vec![],
                        }
                    },
                }],
                ..Default::default()
            },
            _ => Response::default(),
        })
    }
}
phistudio_sdk::export_plugin!(|| Guides { visible: true });
phistudio_sdk::export_native_plugin!(|| Guides { visible: true });
