use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Tracks the provenance (origin and history) of catalog items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogProvenance {
    pub item_id: Uuid,
    pub item_type: String,
    pub source: ProvenanceSource,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modification_history: Vec<ProvenanceRecord>,
    pub lineage: Vec<Uuid>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceSource {
    pub source_type: SourceType,
    pub source_id: Option<String>,
    pub source_url: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Manual,
    Import,
    Generated,
    Cloned,
    Derived,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub record_id: Uuid,
    pub action: ProvenanceAction,
    pub actor: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub changes: HashMap<String, serde_json::Value>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceAction {
    Created,
    Modified,
    Copied,
    Merged,
    Deleted,
    Restored,
}

impl CatalogProvenance {
    pub fn new(item_id: Uuid, item_type: String, created_by: Uuid, source: ProvenanceSource) -> Self {
        Self {
            item_id,
            item_type,
            source,
            created_by,
            created_at: chrono::Utc::now(),
            modification_history: Vec::new(),
            lineage: Vec::new(),
            metadata: HashMap::new(),
        }
    }
    
    pub fn record_modification(
        &mut self,
        action: ProvenanceAction,
        actor: Uuid,
        changes: HashMap<String, serde_json::Value>,
        reason: Option<String>,
    ) {
        let record = ProvenanceRecord {
            record_id: Uuid::new_v4(),
            action,
            actor,
            timestamp: chrono::Utc::now(),
            changes,
            reason,
        };
        
        self.modification_history.push(record);
    }
    
    pub fn add_lineage(&mut self, ancestor_id: Uuid) {
        if !self.lineage.contains(&ancestor_id) {
            self.lineage.push(ancestor_id);
        }
    }
    
    pub fn set_metadata(&mut self, key: String, value: serde_json::Value) {
        self.metadata.insert(key, value);
    }
    
    pub fn get_latest_modification(&self) -> Option<&ProvenanceRecord> {
        self.modification_history.last()
    }
    
    pub fn has_been_modified(&self) -> bool {
        !self.modification_history.is_empty()
    }
    
    pub fn is_derived(&self) -> bool {
        matches!(self.source.source_type, SourceType::Derived | SourceType::Cloned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_provenance_creation() {
        let item_id = Uuid::new_v4();
        let creator = Uuid::new_v4();
        
        let source = ProvenanceSource {
            source_type: SourceType::Manual,
            source_id: None,
            source_url: None,
            version: None,
        };
        
        let provenance = CatalogProvenance::new(
            item_id,
            "agent".to_string(),
            creator,
            source,
        );
        
        assert_eq!(provenance.item_id, item_id);
        assert!(!provenance.has_been_modified());
    }
    
    #[test]
    fn test_modification_recording() {
        let mut provenance = CatalogProvenance::new(
            Uuid::new_v4(),
            "routine".to_string(),
            Uuid::new_v4(),
            ProvenanceSource {
                source_type: SourceType::Manual,
                source_id: None,
                source_url: None,
                version: None,
            },
        );
        
        let mut changes = HashMap::new();
        changes.insert("name".to_string(), serde_json::json!("New Name"));
        
        provenance.record_modification(
            ProvenanceAction::Modified,
            Uuid::new_v4(),
            changes,
            Some("Updated name".to_string()),
        );
        
        assert!(provenance.has_been_modified());
        assert_eq!(provenance.modification_history.len(), 1);
    }
}
