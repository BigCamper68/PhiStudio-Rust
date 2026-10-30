use super::*;

impl WorkspaceContext {
    pub(super) fn assets(&self) -> Result<&Path, ChartError> {
        let assets = self
            .location
            .as_ref()
            .and_then(|p| p.assets.as_deref())
            .ok_or_else(|| invalid("Open a project from Projects first"))?;
        if !assets
            .canonicalize()?
            .starts_with(self.root.join("projects").canonicalize()?)
        {
            return Err(invalid("Resources must belong to a private project"));
        }
        Ok(assets)
    }

    pub(super) fn document(&self) -> Result<&ChartDocument, ChartError> {
        self.document
            .as_ref()
            .ok_or_else(|| invalid("Project snapshot is unavailable"))
    }

    pub(crate) fn snapshot(&self, workspace: &Path) -> Result<ProjectSnapshot<'_>, ChartError> {
        if let Some(location) = &self.location
            && location.chart.starts_with(workspace)
        {
            return Ok((
                Cow::Borrowed(self.document()?),
                location.clone(),
                self.options.clone(),
                true,
            ));
        }
        let project = core::open_project(&self.root, workspace)?;
        let chart = project
            .charts
            .get(project.suggested.unwrap_or(0))
            .ok_or_else(|| invalid("Project has no chart"))?;
        let location = ProjectLocation {
            chart: chart.path.clone(),
            assets: Some(project.assets),
        };
        let doc = core::load_chart(&location.chart)?;
        let options =
            core::load_project_options_with_assets(&location.chart, location.assets.as_deref())?;
        Ok((Cow::Owned(doc), location, options, false))
    }

    pub fn run(self, job: WorkspaceJob) -> Result<(u64, WorkspaceResult), String> {
        let session = self.session;
        self.perform(job)
            .map(|result| (session, result))
            .map_err(|e| e.to_string())
    }

    pub(super) fn perform(&self, job: WorkspaceJob) -> Result<WorkspaceResult, ChartError> {
        let pattern_root = self.root.join("patterns");
        use WorkspaceJob::*;
        match job {
            Repository {
                workspace,
                revision,
                action,
            } => self.repository_job(workspace, revision, action),
            Patterns => Ok(WorkspaceResult::Patterns(patterns::list(&pattern_root)?)),
            SavePattern { name, clipboard } => {
                patterns::save(
                    &pattern_root,
                    &name,
                    (*clipboard).clone(),
                    self.location.as_ref().and_then(|p| p.assets.as_deref()),
                )?;
                Ok(WorkspaceResult::Patterns(patterns::list(&pattern_root)?))
            }
            Favorite { id, value } => {
                patterns::favorite(&pattern_root, &id, value)?;
                Ok(WorkspaceResult::Patterns(patterns::list(&pattern_root)?))
            }
            DeletePattern(id) => {
                patterns::delete(&pattern_root, &id)?;
                Ok(WorkspaceResult::Patterns(patterns::list(&pattern_root)?))
            }
            InsertPattern { id, settings } => {
                let clipboard = patterns::prepare(
                    &pattern_root,
                    &id,
                    self.location.as_ref().and_then(|p| p.assets.as_deref()),
                )?
                .configured(settings.scale, settings.offset)?;
                Ok(WorkspaceResult::Insert(clipboard, settings))
            }
            InspectResource(name) => {
                let assets = self.assets()?;
                let path = assets.join(core_path(&name)?);
                if !path.canonicalize()?.starts_with(assets.canonicalize()?) {
                    return Err(invalid("Resource is outside its project"));
                }
                Ok(WorkspaceResult::Inspected(
                    phistudio_prpr::inspect_media(&path, &name)
                        .map_err(|e| invalid(&format!("{e:#}")))?,
                ))
            }
            Resources | ImportResource { .. } | DeleteResource(_) | UndoResource(_) => {
                let assets = self.assets()?;
                let mut undo = None;
                let mut changed = false;
                match job {
                    ImportResource { file, replace } => {
                        undo = Some(if let Some(name) = replace {
                            resources::replace(&self.root, assets, &name, file.path(), file.name())?
                        } else {
                            resources::add(assets, file.path(), file.name())?.1
                        });
                        changed = true;
                    }
                    DeleteResource(name) => {
                        undo = Some(resources::delete(
                            &self.root,
                            assets,
                            &name,
                            self.document()?,
                            &self.options,
                        )?);
                        changed = true;
                    }
                    UndoResource(change) => {
                        resources::undo(change)?;
                        changed = true;
                    }
                    _ => {}
                }
                Ok(WorkspaceResult::Resources {
                    entries: if let Some(refs) = &self.resource_references {
                        resources::list_with_references(
                            assets,
                            refs.clone(),
                            self.location.as_ref().map(|l| l.chart.as_path()),
                        )?
                    } else {
                        resources::list(assets, self.document()?, &self.options)?
                    },
                    undo,
                    changed,
                })
            }
            Rename { workspace, name } => {
                core::rename_project(&self.root, &workspace, &name)?;
                Ok(WorkspaceResult::Library(core::list_projects(&self.root)?))
            }
            Duplicate { workspace, name } => {
                let (doc, loc, options, _) = self.snapshot(&workspace)?;
                versions::duplicate(&self.root, &workspace, &name, &doc, &loc, &options)?;
                Ok(WorkspaceResult::Library(core::list_projects(&self.root)?))
            }
        }
    }
}
