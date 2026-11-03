//! Bound live queues and completion deliveries, without limiting total streamed data.
use super::*;
use crate::{MAX_EFFECTS, limits::serialized_size};

const MAX_QUEUED: usize = 2048;

#[derive(Default)]
struct Limit {
    count: usize,
    bytes: usize,
}
impl Limit {
    fn admit(
        &mut self,
        bytes: Option<usize>,
        max_count: usize,
        label: &str,
    ) -> Result<usize, String> {
        if self.count >= max_count {
            return Err(format!("Plugin {label} exceeds its item limit"));
        }
        let bytes = bytes
            .filter(|n| *n <= MAX_MESSAGE_BYTES.saturating_sub(self.bytes))
            .ok_or_else(|| format!("Plugin {label} exceeds its byte limit"))?;
        self.count += 1;
        self.bytes += bytes;
        Ok(bytes)
    }
    fn release(&mut self, bytes: usize) {
        self.count -= 1;
        self.bytes -= bytes;
    }
}

fn scope_size(origin: &Origin, required: &[Origin]) -> Option<usize> {
    std::iter::once(origin)
        .chain(required)
        .try_fold(0usize, |size, p| {
            size.checked_add(2 * std::mem::size_of::<Origin>())?
                .checked_add(p.plugin.len())
        })
}
fn scoped_size(
    value: &(impl serde::Serialize + ?Sized),
    origin: &Origin,
    required: &[Origin],
) -> Option<usize> {
    let scope = scope_size(origin, required)?;
    serialized_size(value, MAX_MESSAGE_BYTES.checked_sub(scope)?).map(|bytes| bytes + scope)
}

impl Dispatch {
    fn bytes(&self) -> Option<usize> {
        match self {
            Self::Effect(e) => scoped_size(&e.effect, &e.origin, &e.required),
            Self::Reply {
                recipient,
                required,
                reply,
            } => scoped_size(reply, recipient, required),
        }
    }
}

#[derive(Default)]
pub(super) struct DispatchQueue {
    items: VecDeque<(Dispatch, usize)>,
    limit: Limit,
}
impl DispatchQueue {
    pub(super) fn push_front(&mut self, item: Dispatch) -> Result<(), String> {
        let bytes = self
            .limit
            .admit(item.bytes(), MAX_QUEUED, "cascade queue")?;
        self.items.push_front((item, bytes));
        Ok(())
    }
    pub(super) fn push_back(&mut self, item: Dispatch) -> Result<(), String> {
        let bytes = self
            .limit
            .admit(item.bytes(), MAX_QUEUED, "cascade queue")?;
        self.items.push_back((item, bytes));
        Ok(())
    }
    pub(super) fn pop_front(&mut self) -> Option<Dispatch> {
        let (item, bytes) = self.items.pop_front()?;
        self.limit.release(bytes);
        Some(item)
    }
}

#[derive(Default)]
pub(super) struct Deliveries {
    pub effects: Vec<OwnedEffect>,
    pub calls: Vec<HostCall>,
    limit: Limit,
}
impl Deliveries {
    pub(super) fn effect(&mut self, effect: OwnedEffect) -> Result<(), String> {
        self.limit.admit(
            scoped_size(&effect.effect, &effect.origin, &effect.required),
            MAX_EFFECTS,
            "completion",
        )?;
        self.effects.push(effect);
        Ok(())
    }
    pub(super) fn call(&mut self, call: HostCall) -> Result<(), String> {
        self.limit.admit(
            scoped_size(
                &(&call.request, &call.context, &call.reply_service),
                &call.origin,
                &call.required,
            ),
            MAX_EFFECTS,
            "completion",
        )?;
        self.calls.push(call);
        Ok(())
    }
}
