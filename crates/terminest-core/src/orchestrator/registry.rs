use std::collections::HashMap;
use std::sync::Arc;

use crate::backend::HypervisorBackend;
use crate::domain::HypervisorType;
use crate::error::HypervisorError;

/// Registry managing active hypervisor backend providers.
#[derive(Default, Clone)]
pub struct BackendRegistry {
    backends: HashMap<HypervisorType, Arc<dyn HypervisorBackend>>,
}

impl BackendRegistry {
    /// Creates an empty BackendRegistry.
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
        }
    }

    /// Registers a hypervisor backend provider.
    pub fn register(&mut self, backend_type: HypervisorType, backend: Arc<dyn HypervisorBackend>) {
        self.backends.insert(backend_type, backend);
    }

    /// Retrieves the hypervisor backend corresponding to `backend_type`.
    pub fn get(&self, backend_type: HypervisorType) -> Result<Arc<dyn HypervisorBackend>, HypervisorError> {
        self.backends
            .get(&backend_type)
            .cloned()
            .ok_or_else(|| HypervisorError::Unavailable(format!("Hypervisor provider '{backend_type}' is not registered")))
    }
}
