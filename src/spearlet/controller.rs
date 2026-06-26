use std::sync::Arc;

pub trait Controller: Send + Sync {
    fn name(&self) -> &'static str;
    fn start(&self);
    fn shutdown(&self);
}

#[derive(Default)]
pub struct ControllerGroup {
    controllers: Vec<Arc<dyn Controller>>,
}

impl ControllerGroup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, controller: Arc<dyn Controller>) {
        self.controllers.push(controller);
    }

    pub fn start_all(&self) {
        for c in self.controllers.iter() {
            c.start();
        }
    }

    pub fn shutdown_all(&self) {
        for c in self.controllers.iter() {
            c.shutdown();
        }
    }
}

