//! Horloge commune : un seul `now_ms` pour tout le daemon.

/// Instant courant en millisecondes depuis l'epoque Unix.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
