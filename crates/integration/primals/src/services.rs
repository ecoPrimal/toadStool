// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::error::PrimalResult;

/// Service manager for primal services
pub struct ServiceManager {
    services: std::collections::HashMap<String, ServiceInfo>,
}

// Canonical ServiceInfo and ServiceStatus from cellmembrane-types.
// Local ServiceInfo removed — was {service_id, service_type, status, endpoint}.
// Canonical uses: {id, name, version, endpoints, capabilities, status, metadata}.
// Local ServiceStatus removed — was {Running, Stopped, Error(String), Unknown}.
// Canonical ServiceStatus: {Running, Starting, Stopped, Degraded, Error, Unknown}.
pub use cellmembrane_types::ServiceInfo;
pub use cellmembrane_types::EcosystemServiceStatus as ServiceStatus;

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            services: std::collections::HashMap::new(),
        }
    }

    pub fn start_service(&mut self, service_id: String, service_type: String) -> PrimalResult<()> {
        let config = toadstool_config::env_config::EnvironmentConfig::from_env();
        let host = &config.network.bind_address;
        let port = config.network.toadstool_port;

        let mut service_info = ServiceInfo::new(&service_id, &service_id)
            .with_endpoint(format!("http://{host}:{port}/{service_id}"));
        service_info.metadata.insert("service_type".into(), service_type);
        self.services.insert(service_id, service_info);
        Ok(())
    }

    pub fn stop_service(&mut self, service_id: &str) -> PrimalResult<()> {
        if let Some(service) = self.services.get_mut(service_id) {
            service.status = ServiceStatus::Stopped;
        }
        Ok(())
    }

    pub fn get_service(&self, service_id: &str) -> Option<&ServiceInfo> {
        self.services.get(service_id)
    }
}
