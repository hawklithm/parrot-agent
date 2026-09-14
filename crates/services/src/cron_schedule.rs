//! 统一的 cron 表达式规范化与解析。
//!
//! 应用层（含 paperclip 契约）统一使用 **5 字段** cron（分 时 日 月 周），
//! 而 `cron` crate 0.12 的解析器要求 **6/7 字段**（秒在最前），且其
//! day-of-week 编号为 `1=周日 .. 7=周六`（`0` 非法），
//! 而 paperclip 使用 `0=周日 .. 6=周六`（`server/src/services/cron.ts`）。
//!
//! 因此这里集中处理两件事：
//! 1. 5 字段时补秒字段 `0`；
//! 2. 5 字段时把 day-of-week 从 paperclip 编号平移到 crate 编号（`+1`）。
//!
//! 所有调用点必须经由本模块解析，避免各自漂移出一套编号约定。
//!
//! 注意：crate 与 paperclip 都要求 day-of-month 与 day-of-week **同时**匹配
//! （AND 语义），无需额外转换。

use cron::Schedule;
use std::str::FromStr;

/// 将 5 字段 cron 规范化为 `cron` crate 可解析的形式。
///
/// - 5 字段：补秒字段 `0`，并把 day-of-week 平移为 crate 编号。
/// - 6/7 字段：视为已含秒字段，原样透传（调用方自行保证编号约定）。
///
/// 字段数不是 5/6/7 时返回错误。
pub fn normalize_cron_expression(expression: &str) -> Result<String, String> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return Err("Cron expression must not be empty".to_string());
    }

    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    match fields.len() {
        5 => Ok(format!(
            "0 {} {} {} {} {}",
            fields[0],
            fields[1],
            fields[2],
            fields[3],
            translate_day_of_week(fields[4])
        )),
        6 | 7 => Ok(trimmed.to_string()),
        count => Err(format!(
            "Cron expression must have 5, 6 or 7 fields, got {count}: \"{trimmed}\""
        )),
    }
}

/// 解析 cron 表达式，返回 `cron::Schedule`。
///
/// 错误已包含规范化后的表达式说明，便于日志定位。
pub fn parse_cron_schedule(expression: &str) -> Result<cron::Schedule, String> {
    let normalized = normalize_cron_expression(expression)?;
    Schedule::from_str(&normalized).map_err(|error| format!("Invalid cron expression: {error}"))
}

/// 把 paperclip 编号（0=周日..6=周六）的 day-of-week 字段平移为 crate 编号。
///
/// 支持逗号列表、`*`、区间 `a-b`、步进 `/s` 及其组合：
/// `3` → `4`，`0,6` → `1,7`，`1-5` → `2-6`，`1-5/2` → `2-6/2`，`*/2` 保持不变。
/// 无法识别的片段原样保留，交由 crate 报错。
fn translate_day_of_week(token: &str) -> String {
    token
        .split(',')
        .map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return String::new();
            }
            match part.split_once('/') {
                Some((base, step)) => format!("{}/{}", translate_day_base(base), step.trim()),
                None => translate_day_base(part),
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// 平移单个 day-of-week 片段（不含步进部分）。
fn translate_day_base(base: &str) -> String {
    let base = base.trim();
    if base == "*" {
        return "*".to_string();
    }
    if let Some((start, end)) = base.split_once('-') {
        return match (start.trim().parse::<u32>(), end.trim().parse::<u32>()) {
            (Ok(start), Ok(end)) => format!("{}-{}", start + 1, end + 1),
            _ => base.to_string(),
        };
    }
    match base.parse::<u32>() {
        Ok(value) => (value + 1).to_string(),
        Err(_) => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn next_occurrences(expression: &str, count: usize) -> Vec<String> {
        let schedule = parse_cron_schedule(expression).expect("expression should parse");
        let after = Utc.with_ymd_and_hms(2026, 9, 12, 13, 0, 0).unwrap();
        schedule
            .after(&after)
            .take(count)
            .map(|value| value.format("%Y-%m-%d %H:%M %a").to_string())
            .collect()
    }

    #[test]
    fn five_field_expressions_are_accepted() {
        // paperclip 契约：5 字段（分 时 日 月 周）必须可用。
        for expression in ["* * * * *", "0 0 * * *", "*/5 * * * *", "0 9 * * *", "0 0 1 * *"] {
            assert!(
                parse_cron_schedule(expression).is_ok(),
                "expected `{expression}` to parse"
            );
        }
    }

    #[test]
    fn day_of_week_uses_paperclip_numbering() {
        // paperclip: 1=周一，0=周日，6=周六。
        let monday = next_occurrences("0 9 * * 1", 3);
        assert!(monday.iter().all(|value| value.ends_with("Mon")), "{monday:?}");

        let sunday = next_occurrences("0 9 * * 0", 3);
        assert!(sunday.iter().all(|value| value.ends_with("Sun")), "{sunday:?}");

        let saturday = next_occurrences("0 9 * * 6", 3);
        assert!(saturday.iter().all(|value| value.ends_with("Sat")), "{saturday:?}");

        // 区间 1-5（周一至周五）不得包含周日。
        let weekdays = next_occurrences("0 10 * * 1-5", 5);
        assert!(
            weekdays.iter().all(|value| !value.ends_with("Sun") && !value.ends_with("Sat")),
            "{weekdays:?}"
        );
    }

    #[test]
    fn native_six_and_seven_field_expressions_pass_through() {
        assert!(parse_cron_schedule("0 0 9 * * *").is_ok());
        assert!(parse_cron_schedule("0 * * * * * *").is_ok());
    }

    #[test]
    fn malformed_expressions_are_rejected() {
        for expression in ["", "invalid", "0 9 *", "0 0 9 * * * * extra", "0 9 * * 9", "0 9 * * 7"] {
            assert!(
                parse_cron_schedule(expression).is_err(),
                "expected `{expression}` to be rejected"
            );
        }
    }
}
