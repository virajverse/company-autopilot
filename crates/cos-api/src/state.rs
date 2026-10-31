use cos_executive::{AutonomousEngine, CompanyExecutive};
use cos_goals::GoalForest;
use cos_llm::{HttpInferenceGateway, LlmGatewayConfig, LlmPort, UnconfiguredLlmGateway};
use cos_persistence::{FileStore, GatewayConfigRepository, PersistenceEngine, SqliteStore};
use std::sync::{Arc, RwLock};

/// Shared application state across all REST API handlers.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<PersistenceEngine>,
    pub llm_port: Arc<RwLock<Arc<dyn LlmPort>>>,
    pub event_bus: tokio::sync::broadcast::Sender<String>,
    pub executive: Option<Arc<CompanyExecutive>>,
    pub engine: Option<Arc<AutonomousEngine>>,
    pub forest: Option<Arc<RwLock<GoalForest>>>,
}

impl AppState {
    pub fn new(store: Arc<PersistenceEngine>) -> Self {
        let (event_bus, _) = tokio::sync::broadcast::channel(1000);
        let config = store.load_config().unwrap_or_default();
        let port: Arc<dyn LlmPort> = match HttpInferenceGateway::new(config) {
            Ok(gw) => Arc::new(gw),
            Err(e) => Arc::new(UnconfiguredLlmGateway::new(e.to_string())),
        };

        Self {
            store,
            llm_port: Arc::new(RwLock::new(port)),
            event_bus,
            executive: None,
            engine: None,
            forest: None,
        }
    }

    pub fn with_executive(mut self, executive: Arc<CompanyExecutive>) -> Self {
        self.executive = Some(executive);
        self
    }

    pub fn with_engine(mut self, engine: Arc<AutonomousEngine>) -> Self {
        self.engine = Some(engine);
        self
    }

    pub fn with_forest(mut self, forest: Arc<RwLock<GoalForest>>) -> Self {
        self.forest = Some(forest);
        self
    }

    pub fn from_file_store(store: Arc<FileStore>) -> Self {
        Self::new(Arc::new(PersistenceEngine::File(store)))
    }

    pub fn from_sqlite_store(store: Arc<SqliteStore>) -> Self {
        Self::new(Arc::new(PersistenceEngine::Sqlite(store)))
    }

    pub fn with_llm_port(store: Arc<PersistenceEngine>, port: Arc<dyn LlmPort>) -> Self {
        let (event_bus, _) = tokio::sync::broadcast::channel(1000);
        Self {
            store,
            llm_port: Arc::new(RwLock::new(port)),
            event_bus,
            executive: None,
            engine: None,
            forest: None,
        }
    }

    /// Broadcasts an event to all connected SSE clients.
    pub fn broadcast_event(&self, event_type: &str, payload: serde_json::Value) {
        let msg = serde_json::json!({
            "type": event_type,
            "data": payload,
            "timestamp": chrono::Utc::now().to_rfc3339()
        })
        .to_string();
        let _ = self.event_bus.send(msg);
    }

    pub fn update_llm_gateway(&self, config: LlmGatewayConfig) -> Result<(), String> {
        self.store.save_config(&config).map_err(|e| e.to_string())?;

        let new_port: Arc<dyn LlmPort> = HttpInferenceGateway::new(config)
            .map(|gw| Arc::new(gw) as Arc<dyn LlmPort>)
            .map_err(|e| e.to_string())?;

        *self.llm_port.write().unwrap() = new_port;
        Ok(())
    }
}
