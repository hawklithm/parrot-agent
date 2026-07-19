pub fn valid_transition(from: &str, to: &str) -> bool {
    matches!((from, to), ("installed", "ready"|"error"|"uninstalled") | ("ready", "disabled"|"error"|"upgrade_pending"|"ready") | ("disabled"|"error"|"upgrade_pending", "ready") | (_, "uninstalled"))
}
