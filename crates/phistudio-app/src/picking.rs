//! Repeated completed taps walk overlapping objects in their paint order.
//! Pressing a selected object still starts a drag of that object.
use crate::{
    advanced_ui::StoryId,
    grid_input::{Gesture, Grid, Intent},
    workbench::{Tool, Workbench},
};
use egui::Pos2;
use phistudio_core::{EventId, NoteId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Note(NoteId),
    Event(EventId),
    Story(StoryId),
    Shader(usize),
    Keyframe {
        effect: usize,
        parameter: String,
        index: usize,
    },
}
pub(crate) struct TapCycle {
    session: u64,
    revision: u64,
    point: Pos2,
    hits: Vec<Hit>,
    selected: Hit,
}
#[derive(Clone, Copy)]
enum Surface {
    Notes,
    Events,
    Story,
    Shaders,
}

fn tap(gesture: &Gesture) -> Option<(Grid, Pos2, Surface)> {
    match gesture {
        Gesture::Tap {
            grid,
            origin,
            intent,
            moved: false,
        } => {
            let surface = match intent {
                Intent::Select(_, false) => Surface::Notes,
                Intent::SelectEvent(_, false) => Surface::Events,
                Intent::SelectStory(_, false) => Surface::Story,
                Intent::SelectShader(_) => Surface::Shaders,
                _ => return None,
            };
            Some((*grid, *origin, surface))
        }
        Gesture::Move {
            grid,
            origin,
            moved: false,
            ..
        }
        | Gesture::Resize {
            grid,
            origin,
            moved: false,
            ..
        } => Some((*grid, *origin, Surface::Notes)),
        Gesture::Event(crate::event_grid::EventGesture::Edit {
            grid,
            origin,
            moved: false,
            ..
        }) => Some((*grid, *origin, Surface::Events)),
        Gesture::Story(d)
            if d.id.is_some() && !d.moved && d.group.len() == 1 && d.normal.is_empty() =>
        {
            Some((d.grid, d.origin, Surface::Story))
        }
        Gesture::Shader(d) => {
            crate::shader_ui::grid::tap(d).map(|(grid, pos)| (grid, pos, Surface::Shaders))
        }
        _ => None,
    }
}
fn selected(app: &Workbench, surface: Surface) -> Option<Hit> {
    match surface {
        Surface::Notes => app.selected.map(Hit::Note),
        Surface::Events => app.events.selected.map(Hit::Event),
        Surface::Story => app.advanced.story_selected.map(Hit::Story),
        Surface::Shaders => crate::shader_ui::grid::selected_hit(app),
    }
}
pub(crate) fn finish_tap(app: &mut Workbench, gesture: &Gesture) -> bool {
    if !matches!(app.timeline.tool, Tool::Select | Tool::Event)
        || app.tools.additive
        || app.selection().len() + app.story_selection().len() > 1
    {
        app.timeline.input.cycle = None;
        return false;
    }
    let Some((grid, point, surface)) = tap(gesture) else {
        app.timeline.input.cycle = None;
        return false;
    };
    let hits = match surface {
        Surface::Notes => crate::grid_input::note_hits(app, grid, point)
            .into_iter()
            .map(|(id, _)| Hit::Note(id))
            .collect(),
        Surface::Events => crate::event_grid::hits(app, grid.events(), point)
            .into_iter()
            .map(|(id, _)| Hit::Event(id))
            .collect(),
        Surface::Story => crate::story_grid::hits(app, grid.events(), point)
            .into_iter()
            .rev()
            .map(Hit::Story)
            .collect(),
        Surface::Shaders => crate::shader_ui::grid::cycle_hits(app, grid.events(), point),
    };
    let hits: Vec<Hit> = hits;
    if hits.len() < 2 {
        app.timeline.input.cycle = None;
        return false;
    }
    let current = selected(app, surface);
    let previous = app.timeline.input.cycle.take();
    let continuing = previous.as_ref().is_some_and(|last| {
        last.session == app.session
            && last.revision == app.editor.revision()
            && last.point.distance(point) <= 12.
            && last.hits == hits
            && current.as_ref() == Some(&last.selected)
    });
    let index = if continuing {
        (hits
            .iter()
            .position(|h| Some(h) == current.as_ref())
            .unwrap()
            + 1)
            % hits.len()
    } else {
        0
    };
    let target = hits[index].clone();
    app.transport.playing = false;
    if current.as_ref() != Some(&target) {
        match &target {
            Hit::Note(id) => app.toggle_note(Some(*id), false),
            Hit::Event(id) => app.toggle_event(Some(*id), false),
            Hit::Story(id) => app.toggle_story(Some(*id), false),
            Hit::Shader(index) | Hit::Keyframe { index, .. } => {
                crate::shader_ui::grid::select(app, Some(*index))
            }
        }
    }
    if selected(app, surface).as_ref() == Some(&target) {
        let point = if continuing {
            previous.unwrap().point
        } else {
            point
        };
        app.timeline.input.cycle = Some(TapCycle {
            session: app.session,
            revision: app.editor.revision(),
            point,
            hits,
            selected: target,
        });
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{advanced_ui::RightMode, grid_input};
    use egui::{Event, Modifiers, PointerButton, Rect, pos2, vec2};
    use phistudio_core::{
        Beat, ChartDocument, Command, LineEvent, Note, NoteKind, StoryEvent, StoryKind,
    };
    fn app() -> Workbench {
        let mut app = Workbench::new(ChartDocument::empty("Overlap taps", 120.).unwrap(), None);
        app.set_tool(Tool::Select);
        app
    }
    fn grid(app: &Workbench) -> Grid {
        grid_input::geometry(Rect::from_min_max(pos2(0., 80.), pos2(900., 700.)), app)
    }
    fn button(pos: Pos2, pressed: bool, shift: bool) -> Event {
        Event::PointerButton {
            pos,
            pressed,
            button: PointerButton::Primary,
            modifiers: Modifiers {
                shift,
                ..Default::default()
            },
        }
    }
    fn events(app: &mut Workbench, events: Vec<Event>) {
        let g = grid(app);
        grid_input::handle(app, &events, g, g.full_rect(), |_| true, false);
    }
    fn click(app: &mut Workbench, p: Pos2) {
        events(app, vec![button(p, true, false), button(p, false, false)]);
    }
    fn add_notes(app: &mut Workbench) {
        for kind in [NoteKind::Tap, NoteKind::Hold, NoteKind::Drag] {
            app.execute(Command::AddNote {
                line: 0,
                note: Note {
                    kind,
                    start_time: Beat::new(2, 0, 1).unwrap(),
                    end_time: Beat::new(if kind == NoteKind::Hold { 4 } else { 2 }, 0, 1).unwrap(),
                    ..Default::default()
                },
            });
        }
        app.editor.mark_saved();
        app.select(None);
    }
    #[test]
    fn note_taps_cycle_front_to_back_and_drag_the_selected_lower_hold() {
        let mut app = app();
        add_notes(&mut app);
        let original = app.editor.document().as_json().clone();
        let count = app.editor.history_rows().len();
        let g = grid(&app);
        let p = pos2(g.x(0.), g.y(2.));
        for index in [2, 1, 0, 2, 1] {
            click(&mut app, p);
            assert_eq!(app.selected, Some(NoteId { line: 0, index }));
        }
        assert_eq!(app.editor.document().as_json(), &original);
        assert_eq!(app.editor.history_rows().len(), count);
        assert!(!app.editor.is_dirty());
        events(
            &mut app,
            vec![
                button(p, true, false),
                Event::PointerMoved(p - vec2(0., 96.)),
                button(p - vec2(0., 96.), false, false),
            ],
        );
        assert_eq!(
            app.editor.document().note(0, 1).unwrap().start_time,
            Beat::new(3, 0, 1).unwrap()
        );
        assert_eq!(
            app.editor.document().note(0, 0).unwrap().start_time,
            Beat::new(2, 0, 1).unwrap()
        );
        assert_eq!(
            app.editor.document().note(0, 2).unwrap().start_time,
            Beat::new(2, 0, 1).unwrap()
        );
        assert_eq!(app.editor.history_rows().len(), count + 1);
        app.history_step(false);
        assert_eq!(app.editor.document().as_json(), &original);
        click(&mut app, p);
        assert_eq!(app.selected.unwrap().index, 2);
    }
    #[test]
    fn empty_taps_and_additive_selection_reset_the_cycle() {
        let mut app = app();
        add_notes(&mut app);
        let g = grid(&app);
        let p = pos2(g.x(0.), g.y(2.));
        click(&mut app, p);
        click(&mut app, p);
        assert_eq!(app.selected.unwrap().index, 1);
        click(&mut app, pos2(g.x(0.), g.y(5.)));
        assert!(app.selected.is_none());
        click(&mut app, p);
        assert_eq!(app.selected.unwrap().index, 2);
        events(
            &mut app,
            vec![button(p, true, true), button(p, false, true)],
        );
        assert!(
            !app.selection()
                .notes
                .contains(&NoteId { line: 0, index: 1 })
        );
        assert!(!app.editor.is_dirty());
        click(&mut app, p);
        assert_eq!(app.selected.unwrap().index, 2);
    }
    #[test]
    fn overlapping_event_and_storyboard_tracks_cycle_in_paint_order() {
        let mut raw = ChartDocument::empty("Imported overlap", 120.)
            .unwrap()
            .as_json()
            .clone();
        let rows = [1., 2., 3.]
            .into_iter()
            .map(|value| {
                serde_json::json!(LineEvent {
                    start_time: Beat::new(2, 0, 1).unwrap(),
                    end_time: Beat::new(4, 0, 1).unwrap(),
                    start: value,
                    end: value,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        raw["judgeLineList"][0]["eventLayers"][0]["moveXEvents"] = serde_json::json!(rows);
        raw["judgeLineList"][0]["extended"]["scaleXEvents"] = serde_json::json!([
            StoryEvent::new(
                StoryKind::ScaleX,
                Beat::new(2, 0, 1).unwrap(),
                Beat::new(4, 0, 1).unwrap()
            ),
            StoryEvent::new(
                StoryKind::ScaleX,
                Beat::new(2, 0, 1).unwrap(),
                Beat::new(4, 0, 1).unwrap()
            ),
            StoryEvent::new(
                StoryKind::ScaleX,
                Beat::new(2, 0, 1).unwrap(),
                Beat::new(4, 0, 1).unwrap()
            )
        ]);
        let mut app = Workbench::new(ChartDocument::from_value(raw).unwrap(), None);
        app.set_tool(Tool::Select);
        let first = 0;
        app.editor.mark_saved();
        let original = app.editor.document().as_json().clone();
        let g = grid(&app);
        let p = pos2(g.event_rect.left() + g.event_rect.width() / 10., g.y(3.));
        for index in [first + 2, first + 1, first, first + 2] {
            click(&mut app, p);
            assert_eq!(app.events.selected.unwrap().index, index);
        }
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(!app.editor.is_dirty());
        app.advanced.right_mode = RightMode::Storyboard;
        app.timeline.input.cancel();
        app.editor.mark_saved();
        let original = app.editor.document().as_json().clone();
        let g = grid(&app);
        let p = pos2(g.event_rect.left() + g.event_rect.width() / 12., g.y(3.));
        for index in [2, 1, 0, 2] {
            click(&mut app, p);
            assert_eq!(app.advanced.story_selected.unwrap().index, index);
        }
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(!app.editor.is_dirty());
    }
    #[test]
    fn overlapping_shaders_cycle_and_move_the_selected_lower_effect() {
        use phistudio_core::shaders::*;
        let mut app = app();
        app.advanced.right_mode = RightMode::Shaders;
        let baseline = default_shader_extra(app.editor.document());
        for index in 0..3 {
            app.execute(Command::EditShader {
                baseline: baseline.clone(),
                action: ShaderAction::Insert {
                    index,
                    effect: {
                        let mut effect = serde_json::json!(ShaderEffect::new(
                            Beat::new(2, 0, 1).unwrap(),
                            Beat::new(4, 0, 1).unwrap()
                        ));
                        effect["phiStudioEditorX"] = serde_json::json!(0.);
                        effect
                    },
                },
            });
        }
        app.refresh_shaders();
        app.editor.mark_saved();
        let original = app.editor.document().as_json().clone();
        let g = grid(&app);
        let p = pos2(g.rect.left() + 26., g.y(3.));
        for index in [2, 1, 0, 2, 1] {
            click(&mut app, p);
            assert_eq!(app.shaders.selected, Some(index));
        }
        assert_eq!(app.editor.document().as_json(), &original);
        assert!(!app.editor.is_dirty());
        let count = app.editor.history_rows().len();
        events(
            &mut app,
            vec![
                button(p, true, false),
                Event::PointerMoved(p - vec2(0., 96.)),
                button(p - vec2(0., 96.), false, false),
            ],
        );
        assert_eq!(
            app.editor.document().as_json()[EXTRA_KEY]["effects"][1]["start"],
            serde_json::json!([3, 0, 1])
        );
        assert_eq!(app.editor.history_rows().len(), count + 1);
        app.history_step(false);
        assert_eq!(app.editor.document().as_json(), &original);
    }
}
