//! Owner-thread registration changes applied after borrowed plugin or system parameters drop.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use super::{Ecs, Resources, system::SystemRunner};

pub(crate) enum Registration<C> {
    Resource(Box<dyn FnOnce(&Resources) -> anyhow::Result<()>>),
    System(String, SystemRunner<C>),
    Stage(String),
    ApplyDeferred(String),
}

pub(crate) struct Registrations<C> {
    pending: Rc<RefCell<VecDeque<Registration<C>>>>,
}

impl<C> Default for Registrations<C> {
    fn default() -> Self {
        Self {
            pending: Rc::default(),
        }
    }
}

impl<C> Clone for Registrations<C> {
    fn clone(&self) -> Self {
        Self {
            pending: self.pending.clone(),
        }
    }
}

impl<C: 'static> Registrations<C> {
    pub(crate) fn push(&self, registration: Registration<C>) {
        self.pending.borrow_mut().push_back(registration);
    }

    pub(crate) fn contains_stage(&self, stage: &str) -> bool {
        self.pending
            .borrow()
            .iter()
            .any(|entry| matches!(entry, Registration::Stage(name) if name == stage))
    }

    pub(crate) fn apply(&self, ecs: &mut Ecs<C>) -> anyhow::Result<()> {
        let mut batch = std::mem::take(&mut *self.pending.borrow_mut());
        while let Some(registration) = batch.pop_front() {
            let result = match registration {
                Registration::Resource(add) => add(ecs.resources()),
                Registration::System(stage, system) => ecs.add_registered_system(stage, system),
                Registration::Stage(stage) => {
                    ecs.add_stage(stage);
                    Ok(())
                }
                Registration::ApplyDeferred(stage) => ecs.add_apply_deferred(stage),
            };
            if let Err(error) = result {
                batch.append(&mut self.pending.borrow_mut());
                *self.pending.borrow_mut() = batch;
                return Err(error);
            }
        }
        Ok(())
    }

    pub(crate) fn clear(&self) {
        self.pending.borrow_mut().clear();
    }
}
