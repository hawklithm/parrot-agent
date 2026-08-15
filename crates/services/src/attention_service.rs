/// Attention Service
/// 
/// 注意力优先级管理、待办事项聚合和提醒通知

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AttentionError {
    #[error("item not found: {0}")]
    NotFound(Uuid),
    
    #[error("invalid priority: {0}")]
    InvalidPriority(String),
}

pub type AttentionResult<T> = Result<T, AttentionError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Critical = 5,
    High = 4,
    Medium = 3,
    Low = 2,
    Info = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AttentionType {
    Task,
    Issue,
    Alert,
    Reminder,
    Notification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionItem {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub item_type: AttentionType,
    pub priority: Priority,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub due_date: Option<chrono::DateTime<chrono::Utc>>,
    pub agent_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub is_read: bool,
    pub is_dismissed: bool,
}

impl AttentionItem {
    pub fn new(
        title: String,
        description: String,
        item_type: AttentionType,
        priority: Priority,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            description,
            item_type,
            priority,
            created_at: chrono::Utc::now(),
            due_date: None,
            agent_id: None,
            workspace_id: None,
            metadata: HashMap::new(),
            is_read: false,
            is_dismissed: false,
        }
    }
    
    pub fn is_overdue(&self) -> bool {
        if let Some(due) = self.due_date {
            chrono::Utc::now() > due
        } else {
            false
        }
    }
    pub fn urgency_score(&self) -> u32 {
        let mut score = self.priority.clone() as u32 * 100;
        
        // 过期增加紧急度
        if self.is_overdue() {
            score += 500;
        }
        
        // 即将到期增加紧急度
        if let Some(due) = self.due_date {
            let hours_until_due = (due - chrono::Utc::now()).num_hours();
            if hours_until_due > 0 && hours_until_due < 24 {
                score += 200;
            }
        }
        
        score
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionFilter {
    pub item_type: Option<AttentionType>,
    pub priority: Option<Priority>,
    pub agent_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub include_read: bool,
    pub include_dismissed: bool,
}

impl Default for AttentionFilter {
    fn default() -> Self {
        Self {
            item_type: None,
            priority: None,
            agent_id: None,
            workspace_id: None,
            include_read: true,
            include_dismissed: false,
        }
    }
}

pub struct AttentionService {
    items: HashMap<Uuid, AttentionItem>,
}

impl AttentionService {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
        }
    }
    
    /// 添加注意力事项
    pub fn add_item(&mut self, item: AttentionItem) -> Uuid {
        let id = item.id;
        self.items.insert(id, item);
        id
    }
    
    /// 获取事项
    pub fn get_item(&self, id: Uuid) -> AttentionResult<&AttentionItem> {
        self.items.get(&id)
            .ok_or(AttentionError::NotFound(id))
    }
    
    /// 更新事项
    pub fn update_item(&mut self, id: Uuid, item: AttentionItem) -> AttentionResult<()> {
        if !self.items.contains_key(&id) {
            return Err(AttentionError::NotFound(id));
        }
        self.items.insert(id, item);
        Ok(())
    }
    
    /// 删除事项
    pub fn remove_item(&mut self, id: Uuid) -> AttentionResult<()> {
        self.items.remove(&id)
            .ok_or(AttentionError::NotFound(id))?;
        Ok(())
    }
    
    /// 标记为已读
    pub fn mark_as_read(&mut self, id: Uuid) -> AttentionResult<()> {
        let item = self.items.get_mut(&id)
            .ok_or(AttentionError::NotFound(id))?;
        item.is_read = true;
        Ok(())
    }
    
    /// 标记为已忽略
    pub fn dismiss(&mut self, id: Uuid) -> AttentionResult<()> {
        let item = self.items.get_mut(&id)
            .ok_or(AttentionError::NotFound(id))?;
        item.is_dismissed = true;
        Ok(())
    }
    
    /// 查询事项
    pub fn query_items(&self, filter: AttentionFilter) -> Vec<&AttentionItem> {
        let mut items: Vec<&AttentionItem> = self.items.values()
            .filter(|item| {
                // 过滤类型
                if let Some(ref filter_type) = filter.item_type {
                    if &item.item_type != filter_type {
                        return false;
                    }
                }
                
                // 过滤优先级
                if let Some(ref p) = filter.priority {
                    if item.priority != *p {
                        return false;
                    }
                }
                
                // 过滤agent
                if let Some(agent_id) = filter.agent_id {
                    if item.agent_id != Some(agent_id) {
                        return false;
                    }
                }
                
                // 过滤workspace
                if let Some(ws_id) = filter.workspace_id {
                    if item.workspace_id != Some(ws_id) {
                        return false;
                    }
                }
                
                // 过滤已读
                if !filter.include_read && item.is_read {
                    return false;
                }
                
                // 过滤已忽略
                if !filter.include_dismissed && item.is_dismissed {
                    return false;
                }
                
                true
            })
            .collect();
        
        // 按紧急度排序
        items.sort_by_key(|item| std::cmp::Reverse(item.urgency_score()));
        
        items
    }
    
    /// 获取未读数量
    pub fn get_unread_count(&self) -> usize {
        self.items.values()
            .filter(|item| !item.is_read && !item.is_dismissed)
            .count()
    }
    
    /// 获取过期事项
    pub fn get_overdue_items(&self) -> Vec<&AttentionItem> {
        let mut items: Vec<&AttentionItem> = self.items.values()
            .filter(|item| !item.is_dismissed && item.is_overdue())
            .collect();
        
        items.sort_by_key(|item| std::cmp::Reverse(item.urgency_score()));
        items
    }
    
    /// 获取即将到期事项
    pub fn get_upcoming_items(&self, hours: i64) -> Vec<&AttentionItem> {
        let now = chrono::Utc::now();
        let deadline = now + chrono::Duration::hours(hours);
        
        let mut items: Vec<&AttentionItem> = self.items.values()
            .filter(|item| {
                if item.is_dismissed {
                    return false;
                }
                if let Some(due) = item.due_date {
                    due > now && due <= deadline
                } else {
                    false
                }
            })
            .collect();
        
        items.sort_by_key(|item| item.due_date);
        items
    }
    
    /// 聚合统计
    pub fn get_statistics(&self) -> AttentionStatistics {
        let total = self.items.len();
        let unread = self.get_unread_count();
        let overdue = self.get_overdue_items().len();
        
        let mut by_priority = HashMap::new();
        let mut by_type = HashMap::new();
        
        for item in self.items.values() {
            if !item.is_dismissed {
                *by_priority.entry(item.priority.clone()).or_insert(0) += 1;
                *by_type.entry(format!("{:?}", item.item_type)).or_insert(0) += 1;
            }
        }
        
        AttentionStatistics {
            total_items: total,
            unread_items: unread,
            overdue_items: overdue,
            by_priority,
            by_type,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AttentionStatistics {
    pub total_items: usize,
    pub unread_items: usize,
    pub overdue_items: usize,
    pub by_priority: HashMap<Priority, usize>,
    pub by_type: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_add_and_get_item() {
        let mut service = AttentionService::new();
        
        let item = AttentionItem::new(
            "Test Task".to_string(),
            "Test description".to_string(),
            AttentionType::Task,
            Priority::High,
        );
        
        let id = service.add_item(item.clone());
        let retrieved = service.get_item(id).unwrap();
        
        assert_eq!(retrieved.title, "Test Task");
        assert_eq!(retrieved.priority, Priority::High);
    }
    
    #[test]
    fn test_mark_as_read() {
        let mut service = AttentionService::new();
        
        let item = AttentionItem::new(
            "Test".to_string(),
            "".to_string(),
            AttentionType::Task,
            Priority::Medium,
        );
        
        let id = service.add_item(item);
        assert!(!service.get_item(id).unwrap().is_read);
        
        service.mark_as_read(id).unwrap();
        assert!(service.get_item(id).unwrap().is_read);
    }
    
    #[test]
    fn test_overdue_detection() {
        let mut item = AttentionItem::new(
            "Overdue Task".to_string(),
            "".to_string(),
            AttentionType::Task,
            Priority::High,
        );
        
        // 设置过期时间
        item.due_date = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        
        assert!(item.is_overdue());
    }
    
    #[test]
    fn test_urgency_score() {
        let mut item = AttentionItem::new(
            "Task".to_string(),
            "".to_string(),
            AttentionType::Task,
            Priority::High,
        );
        
        let normal_score = item.urgency_score();
        
        // 设置过期
        item.due_date = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        let overdue_score = item.urgency_score();
        
        assert!(overdue_score > normal_score);
    }
    
    #[test]
    fn test_query_filter() {
        let mut service = AttentionService::new();
        
        let item1 = AttentionItem::new(
            "High Priority".to_string(),
            "".to_string(),
            AttentionType::Task,
            Priority::High,
        );
        
        let item2 = AttentionItem::new(
            "Low Priority".to_string(),
            "".to_string(),
            AttentionType::Task,
            Priority::Low,
        );
        
        service.add_item(item1);
        service.add_item(item2);
        
        let filter = AttentionFilter {
            priority: Some(Priority::High),
            ..Default::default()
        };
        
        let results = service.query_items(filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "High Priority");
    }
}
