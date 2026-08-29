use rusqlite::Connection;
use serde_json::Value;

pub enum ContextEvent {
    Capture {
        item_id: String,
        workspace_id: Option<String>,
        source: String,
        data_type: String,
        metadata: Value,
    },
    UserAction {
        item_id: String,
        action: String,
        source_workspace: Option<String>,
        destination_workspace: Option<String>,
    },
    WorkspaceSelected {
        workspace_id: String,
    },
}

pub struct ContextSignalParams {
    pub workspace_id: Option<String>,
    pub item_id: Option<String>,
    pub signal_type: String,
    pub payload: Value,
}

pub trait ContextProvider: Send + Sync {
    fn name(&self) -> &'static str;
    
    // Lifecycle
    fn initialize(&mut self) -> Result<(), String> { Ok(()) }
    fn cleanup(&mut self) -> Result<(), String> { Ok(()) }
    
    // For push-based (event-driven) and pull-based contextual signals
    fn collect(&self, event: Option<&ContextEvent>) -> Vec<ContextSignalParams>;
}

pub struct ContextEngine {
    providers: Vec<Box<dyn ContextProvider>>,
}

impl ContextEngine {
    pub fn new() -> Self {
        Self { providers: vec![] }
    }
    
    pub fn register(&mut self, provider: Box<dyn ContextProvider>) {
        self.providers.push(provider);
    }
    
    pub fn initialize_all(&mut self) -> Result<(), String> {
        for p in &mut self.providers {
            p.initialize()?;
        }
        Ok(())
    }
    
    pub fn process_event(&self, conn: &Connection, event: ContextEvent) {
        for provider in &self.providers {
            let signal_params = provider.collect(Some(&event));
            for param in signal_params {
                let _ = super::operations::create_context_signal(
                    conn,
                    param.workspace_id.as_deref(),
                    param.item_id.as_deref(),
                    &param.signal_type,
                    None,
                    &param.payload.to_string()
                );
            }
        }
    }
}
