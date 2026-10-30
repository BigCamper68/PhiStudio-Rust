//! Project-scoped line texture chooser; one worker and bounded visible thumbnails.
mod picker;
#[cfg(test)]
mod tests;
use egui::{Context, TextureHandle};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};

pub(crate) struct State {
    pub count: usize,
    pub show: bool,
    pub importing: bool,
    target: (u64, usize, u64),
    current: String,
    query: String,
    key: Option<(u64, u64, Option<PathBuf>)>,
    generation: u64,
    frame: u64,
    names: Option<Vec<String>>,
    error: Option<String>,
    entries: HashMap<String, Entry>,
    pending: usize,
    worker: Option<Worker>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            count: 1,
            show: false,
            importing: false,
            target: (0, 0, 0),
            current: "line.png".into(),
            query: String::new(),
            key: None,
            generation: 0,
            frame: 0,
            names: None,
            error: None,
            entries: HashMap::new(),
            pending: 0,
            worker: None,
        }
    }
}
struct Entry {
    image: Option<TextureHandle>,
    loading: bool,
    used: u64,
}
enum Kind {
    List,
    Image(String),
}
struct Request {
    generation: u64,
    root: PathBuf,
    kind: Kind,
    ctx: Context,
}
enum Data {
    Names(Result<Vec<String>, String>),
    Image(String, Option<phistudio_prpr::CoverImage>),
}
struct Response {
    generation: u64,
    data: Data,
}
struct Worker {
    send: Sender<Request>,
    receive: Receiver<Response>,
    active: Arc<AtomicU64>,
}
impl Worker {
    fn new() -> Self {
        let (send, requests) = mpsc::channel::<Request>();
        let (results, receive) = mpsc::channel();
        let active = Arc::new(AtomicU64::new(0));
        let token = Arc::clone(&active);
        std::thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                if request.generation != token.load(Ordering::Relaxed) {
                    continue;
                }
                let data = match request.kind {
                    Kind::List => Data::Names(
                        phistudio_core::resources::texture_names(&request.root)
                            .map_err(|e| e.to_string()),
                    ),
                    Kind::Image(name) => {
                        let image = phistudio_core::resources::texture_path(&request.root, &name)
                            .ok()
                            .and_then(|path| phistudio_prpr::load_texture_thumbnail(&path).ok());
                        Data::Image(name, image)
                    }
                };
                if results
                    .send(Response {
                        generation: request.generation,
                        data,
                    })
                    .is_err()
                {
                    break;
                }
                request.ctx.request_repaint();
            }
        });
        Self {
            send,
            receive,
            active,
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.active.store(u64::MAX, Ordering::Relaxed);
    }
}
impl State {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub(crate) fn probe_report(&self) -> serde_json::Value {
        serde_json::json!({"catalog_loaded": self.names.is_some(),"textures":self.names.as_ref().map_or(0,Vec::len),"ready":self.entries.values().filter(|e|e.image.is_some()).count(),"failed":self.entries.values().filter(|e|!e.loading && e.image.is_none()).count(),"pending":self.pending})
    }
    pub fn open(&mut self, session: u64, line: usize, revision: u64, current: String) {
        self.target = (session, line, revision);
        self.current = current.replace('\\', "/");
        self.query.clear();
        self.show = true;
    }
    fn sync(&mut self, ctx: &Context, session: u64, revision: u64, root: Option<PathBuf>) {
        self.frame = self.frame.wrapping_add(1);
        let key = (session, revision, root.clone());
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key);
            self.generation = self.generation.wrapping_add(1);
            self.entries.clear();
            self.pending = 0;
            self.error = None;
            if let Some(root) = root {
                let worker = self.worker.get_or_insert_with(Worker::new);
                worker.active.store(self.generation, Ordering::Relaxed);
                let _ = worker.send.send(Request {
                    generation: self.generation,
                    root,
                    kind: Kind::List,
                    ctx: ctx.clone(),
                });
                self.names = None;
            } else {
                self.names = Some(vec![]);
            }
        }
        if let Some(worker) = &self.worker {
            for response in worker.receive.try_iter() {
                if response.generation != self.generation {
                    continue;
                }
                match response.data {
                    Data::Names(result) => match result {
                        Ok(names) => self.names = Some(names),
                        Err(error) => {
                            self.error = Some(error);
                            self.names = Some(vec![]);
                        }
                    },
                    Data::Image(name, image) => {
                        self.pending = self.pending.saturating_sub(1);
                        if let Some(entry) = self.entries.get_mut(&name) {
                            entry.loading = false;
                            entry.image = image.map(|image| {
                                ctx.load_texture(
                                    format!("line-texture-{}-{name}", self.generation),
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        image.size,
                                        &image.rgba,
                                    ),
                                    egui::TextureOptions::LINEAR,
                                )
                            });
                        }
                    }
                }
            }
        }
        while self.entries.len() > 128 {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, e)| !e.loading)
                .min_by_key(|(_, e)| e.used)
                .map(|(name, _)| name.clone());
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
    }
    fn image(&mut self, ctx: &Context, name: &str) -> Option<TextureHandle> {
        if let Some(entry) = self.entries.get_mut(name) {
            entry.used = self.frame;
            return entry.image.clone();
        }
        if self.pending < 24
            && let Some((_, _, Some(root))) = &self.key
            && let Some(worker) = &self.worker
            && worker
                .send
                .send(Request {
                    generation: self.generation,
                    root: root.clone(),
                    kind: Kind::Image(name.into()),
                    ctx: ctx.clone(),
                })
                .is_ok()
        {
            self.entries.insert(
                name.into(),
                Entry {
                    image: None,
                    loading: true,
                    used: self.frame,
                },
            );
            self.pending += 1;
        }
        None
    }
}
