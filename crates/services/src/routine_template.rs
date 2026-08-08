//! Routine Template Service
//! 
//! 完整迁移自 paperclip:
//! - packages/shared/src/routine-variables.ts:6-143
//!
//! 提供模板插值、变量名提取、变量同步功能

use models::{RoutineVariable, RoutineVariableType};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use crate::routine_variable_service::{RoutineVariableValue, BUILTIN_ROUTINE_VARIABLE_NAMES};

/// 验证 routine 变量名格式
/// 格式: ^[A-Za-z][A-Za-z0-9_]*$
pub fn is_valid_routine_variable_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    
    if !first.is_ascii_alphabetic() {
        return false;
    }
    
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 判断变量名是否为日期类型（以 "Date" 结尾）
/// 例如: "startDate", "endDate", "dueDate" 都会被识别为 date 类型
pub fn is_routine_date_variable_name(name: &str) -> bool {
    name.ends_with("Date")
}

/// 检查是否为内置变量
pub fn is_builtin_routine_variable(name: &str) -> bool {
    BUILTIN_ROUTINE_VARIABLE_NAMES.contains(&name)
}

/// 反转义变量名中的下划线
/// WYSIWYG Markdown 编辑器会将 `{{pr_url}}` 转义为 `{{pr\_url}}`
/// 此函数将 `\_` 还原为 `_`
pub fn unescape_routine_variable_name(raw: &str) -> String {
    raw.replace("\\_", "_")
}

/// 从模板字符串中提取所有变量名
/// 正则: \{\{\s*([A-Za-z](?:\\_|[A-Za-z0-9_])*)\s*\}\}
/// 
/// 支持:
/// - `{{name}}` -> "name"
/// - `{{ name }}` -> "name" (前后空格)
/// - `{{pr\_url}}` -> "pr_url" (转义下划线)
/// 
/// 对应 Paperclip: packages/shared/src/routine-variables.ts:83-99
pub fn extract_routine_variable_names(templates: &[&str]) -> Vec<String> {
    // Regex: {{  前后可有空格，变量名可以包含 \_ 转义序列
    let re = Regex::new(r"\{\{\s*([A-Za-z](?:\\_|[A-Za-z0-9_])*)\s*\}\}").unwrap();
    
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    
    for template in templates {
        for cap in re.captures_iter(template) {
            let raw_name = &cap[1];
            let unescaped = unescape_routine_variable_name(raw_name);
            
            // 跳过内置变量和已见过的变量
            if !is_builtin_routine_variable(&unescaped) && seen.insert(unescaped.clone()) {
                result.push(unescaped);
            }
        }
    }
    
    result
}

/// 创建默认的 routine 变量定义
/// 根据变量名推断类型: 以 "Date" 结尾 -> "date", 否则 -> "text"
/// 
/// 对应 Paperclip: packages/shared/src/routine-variables.ts:101-108
pub fn default_routine_variable(name: String) -> RoutineVariable {
    let var_type = if is_routine_date_variable_name(&name) {
        RoutineVariableType::Date
    } else {
        RoutineVariableType::Text
    };
    
    RoutineVariable {
        name,
        label: String::new(), // 将在调用处设置为 name
        var_type,
        default_value: None,
        required: true,
        options: None,
    }
}

/// 同步模板中的变量与现有变量定义
/// 
/// 从模板中提取变量名，对于每个变量名:
/// - 如果已存在定义，保留原定义
/// - 如果不存在，创建默认定义
/// 
/// 返回的变量列表按模板中出现的顺序排列
/// 
/// 对应 Paperclip: packages/shared/src/routine-variables.ts:110-119
pub fn sync_routine_variables_with_template(
    templates: &[&str],
    existing: Option<&[RoutineVariable]>,
) -> Vec<RoutineVariable> {
    let template_var_names = extract_routine_variable_names(templates);
    
    let existing_map: HashMap<String, RoutineVariable> = existing
        .unwrap_or(&[])
        .iter()
        .map(|v| (v.name.clone(), v.clone()))
        .collect();
    
    template_var_names
        .into_iter()
        .map(|name| {
            existing_map.get(&name).cloned().unwrap_or_else(|| {
                let mut var = default_routine_variable(name.clone());
                var.label = name.clone(); // 默认 label 与 name 相同
                var
            })
        })
        .collect()
}

/// 模板插值: 将 {{variableName}} 替换为实际值
/// 
/// 支持:
/// - 空格容忍: `{{ name }}` 也能识别
/// - 转义下划线: `{{pr\_url}}` 识别为 `pr_url`
/// - 未找到的变量: 保留原始占位符
/// 
/// 对应 Paperclip: packages/shared/src/routine-variables.ts:132-143
pub fn interpolate_routine_template(
    template: Option<&str>,
    values: &HashMap<String, RoutineVariableValue>,
) -> Option<String> {
    let template = template?;
    
    // 正则: 匹配 {{variableName}} (支持前后空格和转义下划线)
    let re = Regex::new(r"\{\{\s*([A-Za-z](?:\\_|[A-Za-z0-9_])*)\s*\}\}").unwrap();
    
    let result = re.replace_all(template, |caps: &regex::Captures| {
        let raw_name = &caps[1];
        let unescaped = unescape_routine_variable_name(raw_name);
        
        // 如果找到变量值，替换；否则保留原始占位符
        values
            .get(&unescaped)
            .map(|v| v.to_string_value())
            .unwrap_or_else(|| caps[0].to_string())
    });
    
    Some(result.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_routine_variable_name() {
        assert!(is_valid_routine_variable_name("name"));
        assert!(is_valid_routine_variable_name("userName"));
        assert!(is_valid_routine_variable_name("user_name"));
        assert!(is_valid_routine_variable_name("a1"));
        
        assert!(!is_valid_routine_variable_name(""));
        assert!(!is_valid_routine_variable_name("1name"));
        assert!(!is_valid_routine_variable_name("user-name"));
        assert!(!is_valid_routine_variable_name("user name"));
    }

    #[test]
    fn test_is_routine_date_variable_name() {
        assert!(is_routine_date_variable_name("startDate"));
        assert!(is_routine_date_variable_name("endDate"));
        assert!(is_routine_date_variable_name("dueDate"));
        
        assert!(!is_routine_date_variable_name("date"));
        assert!(!is_routine_date_variable_name("dateValue"));
        assert!(!is_routine_date_variable_name("userName"));
    }

    #[test]
    fn test_unescape_routine_variable_name() {
        assert_eq!(unescape_routine_variable_name("pr\\_url"), "pr_url");
        assert_eq!(unescape_routine_variable_name("user\\_name"), "user_name");
        assert_eq!(unescape_routine_variable_name("simple"), "simple");
    }

    #[test]
    fn test_extract_routine_variable_names() {
        let templates = vec![
            "Hello {{name}}, welcome!",
            "Your email is {{ email }}",
            "PR URL: {{pr\\_url}}",
            "Built-in: {{date}} and {{timestamp}}",
        ];
        
        let names = extract_routine_variable_names(&templates);
        
        // 应该提取 name, email, pr_url (但不包括内置变量 date, timestamp)
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"name".to_string()));
        assert!(names.contains(&"email".to_string()));
        assert!(names.contains(&"pr_url".to_string()));
        assert!(!names.contains(&"date".to_string()));
        assert!(!names.contains(&"timestamp".to_string()));
    }

    #[test]
    fn test_interpolate_routine_template() {
        let mut values = HashMap::new();
        values.insert("name".to_string(), RoutineVariableValue::String("Alice".to_string()));
        values.insert("age".to_string(), RoutineVariableValue::Number(30.0));
        values.insert("pr_url".to_string(), RoutineVariableValue::String("https://github.com/pr/123".to_string()));
        
        // 基本插值
        assert_eq!(
            interpolate_routine_template(Some("Hello {{name}}!"), &values),
            Some("Hello Alice!".to_string())
        );
        
        // 空格容忍
        assert_eq!(
            interpolate_routine_template(Some("Age: {{ age }}"), &values),
            Some("Age: 30".to_string())
        );
        
        // 转义下划线
        assert_eq!(
            interpolate_routine_template(Some("URL: {{pr\\_url}}"), &values),
            Some("URL: https://github.com/pr/123".to_string())
        );
        
        // 未找到的变量保留原样
        assert_eq!(
            interpolate_routine_template(Some("Missing: {{unknown}}"), &values),
            Some("Missing: {{unknown}}".to_string())
        );
        
        // None 输入
        assert_eq!(interpolate_routine_template(None, &values), None);
    }

    #[test]
    fn test_sync_routine_variables_with_template() {
        let templates = vec!["Hello {{name}}, {{startDate}}"];
        
        // 没有现有定义
        let synced = sync_routine_variables_with_template(&templates, None);
        assert_eq!(synced.len(), 2);
        assert_eq!(synced[0].name, "name");
        assert_eq!(synced[0].var_type, RoutineVariableType::Text);
        assert_eq!(synced[1].name, "startDate");
        assert_eq!(synced[1].var_type, RoutineVariableType::Date); // 自动推断为 date
        
        // 有现有定义，应该保留
        let existing = vec![
            RoutineVariable {
                name: "name".to_string(),
                label: "User Name".to_string(),
                var_type: RoutineVariableType::Text,
                default_value: Some(serde_json::json!("Guest")),
                required: false,
                options: None,
            }
        ];
        
        let synced = sync_routine_variables_with_template(&templates, Some(&existing));
        assert_eq!(synced.len(), 2);
        assert_eq!(synced[0].name, "name");
        assert_eq!(synced[0].label, "User Name"); // 保留了原 label
        assert_eq!(synced[0].required, false); // 保留了原设置
    }
}
