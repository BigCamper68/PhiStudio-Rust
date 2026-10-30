//! Editor controls configure the original Phira resources and animations.
use crate::Preview;
use anyhow::Result;
use phistudio_core::{EditorOptions, NoteKind};
use prpr::core::{NOTE_WIDTH_RATIO_BASE, ParticleEmitter};
impl Preview {
    pub fn set_editor_options(
        &mut self,
        options: &EditorOptions,
        active_line: usize,
    ) -> Result<()> {
        let options = options.clone().normalized();
        if self.editor_options.language != options.language {
            prpr_l10n::set_prefered_locale(Some(options.language.phira_locale().parse()?));
        }
        let ratio = options.preview_aspect.ratio();
        let force = ratio.is_some() || self.package_force_aspect;
        if self.resource.config.aspect_ratio != ratio
            || self.resource.info.force_aspect_ratio != force
        {
            self.resource.config.aspect_ratio = ratio;
            self.resource.info.force_aspect_ratio = force;
            self.resource.last_vp = (0, 0, 0, 0);
        }
        if self.resource.config.note_scale != options.preview_note_scale {
            let emitter = ParticleEmitter::new(
                &self.resource.res_pack,
                options.preview_note_scale,
                self.resource.res_pack.info.hide_particles,
            )?;
            self.resource.emitter = emitter;
            self.resource.config.note_scale = options.preview_note_scale;
            self.resource.note_width = options.preview_note_scale * NOTE_WIDTH_RATIO_BASE as f32;
        }
        self.resource.config.chart_debug = options.mark_active_line_id;
        self.resource.config.show_acc = options.preview_accuracy;
        self.resource.config.double_hint = options.preview_double_hint;
        self.resource.info.background_dim = options.preview_dim.unwrap_or(self.package_dim);
        if self.editor_options.preview_particles != options.preview_particles {
            self.clear_particles = true;
        }
        let rebuild = !self.sound_schedule.ready
            || self.editor_options.hitsound_delay_ms != options.hitsound_delay_ms
            || self.editor_options.drag_delay_ms != options.drag_delay_ms;
        self.editor_options = options;
        self.configure_effects();
        if rebuild {
            self.rebuild_sound_schedule();
        }
        self.active_line = active_line;
        Ok(())
    }
    pub fn hud_pause_bounds(&self) -> [f32; 4] {
        self.pause_bounds
    }
    pub fn audition(&mut self, reference: Option<&str>, kind: NoteKind) -> Result<()> {
        use prpr::judge::HitSound;
        let sound = match reference {
            Some("tap.mp3") => HitSound::Click,
            Some("flick.mp3") => HitSound::Flick,
            Some("drag.mp3") => HitSound::Drag,
            Some(path) => {
                anyhow::ensure!(
                    self.resource.extra_sfxs.contains_key(path),
                    "Apply this hitsound before testing it"
                );
                HitSound::Custom(path.into())
            }
            None => match kind {
                NoteKind::Tap | NoteKind::Hold => HitSound::Click,
                NoteKind::Flick => HitSound::Flick,
                NoteKind::Drag => HitSound::Drag,
            },
        };
        sound.play(&mut self.resource);
        Ok(())
    }
}
