//! Routine Variable Service - 迁移自 Paperclip
use models::{AppError, RoutineVariable, RoutineVariableType};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use chrono::Utc;

pub const BUILTIN_ROUTINE_VARIABLE_NAMES: &[&str] = &["date", "timestamp"];

#[derive(Debug, Clone, PartialEq)]
pub enum RoutineVariableValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
}

impl RoutineVariableValue {
    pub fn to_string_value(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Number(n) => n.to_string(),
            Self::Boolean(b) => b.to_string(),
            Self::Null => String::new(),
        }
    }
}

pub fn get_builtin_routine_variable_values() -> HashMap<String, String> {
    let now = Utc::now();
    let mut values = HashMap::new();
    values.insert("date".to_string(), now.format("%Y-%m-%d").to_string());
    values.insert("timestamp".to_string(), now.format("%B %-d, %Y at %-I:%M %p UTC").to_string());
    values
}

pub fn is_missing_routine_variable_value(value: &RoutineVariableValue) -> bool {
    matches!(value, RoutineVariableValue::Null) 
        || matches!(value, RoutineVariableValue::String(s) if s.trim().is_empty())
}

pub fn parse_boolean_value(name: &str, raw: &JsonValue) -> Result<bool, AppError> {
    match raw {
        JsonValue::Bool(b) => Ok(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                match i {
                    0 => Ok(false),
                    1 => Ok(true),
                    _ => Err(AppError::Unprocessable(format!("Variable \"{}\" must be a boolean", name))),
                }
            } else {
                Err(AppError::Unprocessable(format!("Variable \"{}\" must be a boolean", name)))
            }
        }
        JsonValue::String(s) => {
            let normalized = s.trim().to_lowercase();
            if ["true", "1", "yes", "y", "on"].contains(&normalized.as_str()) {
                Ok(true)
            } else if ["false", "0", "no", "n", "off"].contains(&normalized.as_str()) {
                Ok(false)
            } else {
                Err(AppError::Unprocessable(format!("Variable \"{}\" must be a boolean", name)))
            }
        }
        _ => Err(AppError::Unprocessable(format!("Variable \"{}\" must be a boolean", name))),
    }
}

pub fn parse_number_value(name: &str, raw: &JsonValue) -> Result<f64, AppError> {
    match raw {
        JsonValue::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() {
                    Ok(f)
                } else {
                    Err(AppError::Unprocessable(format!("Variable \"{}\" must be a number", name)))
                }
            } else {
                Err(AppError::Unprocessable(format!("Variable \"{}\" must be a number", name)))
            }
        }
        JsonValue::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Err(AppError::Unprocessable(format!("Variable \"{}\" must be a number", name)))
            } else {
                match trimmed.parse::<f64>() {
                    Ok(f) if f.is_finite() => Ok(f),
                    _ => Err(AppError::Unprocessable(format!("Variable \"{}\" must be a number", name))),
                }
            }
        }
        _ => Err(AppError::Unprocessable(format!("Variable \"{}\" must be a number", name))),
    }
}

fn json_value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => String::new(),
        _ => value.to_string(),
    }
}

pub fn normalize_routine_variable_value(
    variable: &RoutineVariable,
    raw: Option<&JsonValue>,
) -> Result<RoutineVariableValue, AppError> {
    let Some(raw) = raw else {
        return Ok(RoutineVariableValue::Null);
    };
    
    if raw.is_null() {
        return Ok(RoutineVariableValue::Null);
    }
    
    match variable.var_type {
        RoutineVariableType::Boolean => {
            Ok(RoutineVariableValue::Boolean(parse_boolean_value(&variable.name, raw)?))
        }
        RoutineVariableType::Number => {
            Ok(RoutineVariableValue::Number(parse_number_value(&variable.name, raw)?))
        }
        RoutineVariableType::Select => {
            let normalized = json_value_to_string(raw);
            let empty_vec = Vec::new();
            let options = variable.options.as_ref().unwrap_or(&empty_vec);
            if !options.contains(&normalized) {
                return Err(AppError::Unprocessable(format!(
                    "Variable \"{}\" must match one of: {}",
                    variable.name,
                    options.join(", ")
                )));
            }
            Ok(RoutineVariableValue::String(normalized))
        }
        RoutineVariableType::Date | RoutineVariableType::Text | 
        RoutineVariableType::Textarea | RoutineVariableType::Secret => {
            Ok(RoutineVariableValue::String(json_value_to_string(raw)))
        }
    }
}

pub fn assert_routine_variable_definitions(variables: &[RoutineVariable]) -> Result<(), AppError> {
    for variable in variables {
        if let Some(ref default_val) = variable.default_value {
            normalize_routine_variable_value(variable, Some(default_val))?;
        }
        
        if variable.var_type == RoutineVariableType::Select {
            let empty_vec = Vec::new();
            let options = variable.options.as_ref().unwrap_or(&empty_vec);
            if options.is_empty() {
                return Err(AppError::Unprocessable(format!(
                    "Variable \"{}\" must define at least one option",
                    variable.name
                )));
            }
        }
    }
    Ok(())
}

pub fn sanitize_routine_variable_inputs(
    variables: Option<Vec<serde_json::Value>>,
) -> Result<Vec<RoutineVariable>, AppError> {
    let vars = variables.unwrap_or_default();
    let mut result = Vec::new();
    
    for var in vars {
        let name = var.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Variable name is required".to_string()))?
            .to_string();
        
        let label = var.get("label")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| name.clone());
        
        let var_type = var.get("type")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "boolean" => Some(RoutineVariableType::Boolean),
                "number" => Some(RoutineVariableType::Number),
                "date" => Some(RoutineVariableType::Date),
                "select" => Some(RoutineVariableType::Select),
                "secret" => Some(RoutineVariableType::Secret),
                "text" => Some(RoutineVariableType::Text),
                "textarea" => Some(RoutineVariableType::Textarea),
                _ => None,
            })
            .unwrap_or(RoutineVariableType::Text);
        
        let default_value = var.get("defaultValue").cloned();
        let required = var.get("required").and_then(|v| v.as_bool()).unwrap_or(true);
        let options = var.get("options")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            });
        
        result.push(RoutineVariable {
            name,
            label,
            var_type,
            default_value,
            required,
            options,
        });
    }
    
    Ok(result)
}

pub fn assert_schedule_compatible_variables(variables: &[RoutineVariable]) -> Result<(), AppError> {
    let mut missing_defaults = Vec::new();
    
    for variable in variables {
        if !variable.required {
            continue;
        }
        
        let is_missing = match normalize_routine_variable_value(variable, variable.default_value.as_ref()) {
            Ok(value) => is_missing_routine_variable_value(&value),
            Err(_) => true,
        };
        
        if is_missing {
            missing_defaults.push(variable.name.clone());
        }
    }
    
    if !missing_defaults.is_empty() {
        return Err(AppError::Unprocessable(format!(
            "Scheduled routines require defaults for required variables: {}",
            missing_defaults.join(", ")
        )));
    }
    
    Ok(())
}

pub struct ResolveVariableInput<'a> {
    pub source: &'a str,
    pub payload: Option<&'a JsonValue>,
    pub variables: Option<&'a HashMap<String, JsonValue>>,
    pub automatic_variables: Option<&'a HashMap<String, RoutineVariableValue>>,
}

pub fn resolve_routine_variable_values(
    variables: &[RoutineVariable],
    input: ResolveVariableInput,
) -> Result<HashMap<String, RoutineVariableValue>, AppError> {
    if variables.is_empty() {
        return Ok(HashMap::new());
    }
    
    let mut provided = HashMap::new();
    
    if input.source == "webhook" {
        if let Some(JsonValue::Object(map)) = input.payload {
            for (k, v) in map {
                provided.insert(k.clone(), v.clone());
            }
        }
    }
    
    if let Some(JsonValue::Object(map)) = input.payload {
        if let Some(JsonValue::Object(nested_vars)) = map.get("variables") {
            for (k, v) in nested_vars {
                provided.insert(k.clone(), v.clone());
            }
        }
    }
    
    if let Some(vars) = input.variables {
        for (k, v) in vars {
            provided.insert(k.clone(), v.clone());
        }
    }
    
    provided.remove("variables");
    
    let empty_map = HashMap::new();
    let automatic_variables = input.automatic_variables.unwrap_or(&empty_map);
    let mut resolved = HashMap::new();
    let mut missing = Vec::new();
    
    for variable in variables {
        let candidate: Option<RoutineVariableValue> = if let Some(auto_val) = automatic_variables.get(&variable.name) {
            Some(auto_val.clone())
        } else if let Some(provided_val) = provided.get(&variable.name) {
            Some(normalize_routine_variable_value(variable, Some(provided_val))?)
        } else {
            let default_result = normalize_routine_variable_value(variable, variable.default_value.as_ref())?;
            if matches!(default_result, RoutineVariableValue::Null) {
                None
            } else {
                Some(default_result)
            }
        };
        
        let Some(value) = candidate else {
            if variable.required {
                missing.push(variable.name.clone());
            }
            continue;
        };
        
        if is_missing_routine_variable_value(&value) {
            if variable.required {
                missing.push(variable.name.clone());
            }
            continue;
        }
        
        resolved.insert(variable.name.clone(), value);
    }
    
    if !missing.is_empty() {
        return Err(AppError::Unprocessable(format!(
            "Missing routine variables: {}",
            missing.join(", ")
        )));
    }
    
    Ok(resolved)
}
