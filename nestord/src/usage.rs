//! Suivi de la consommation du quota de la session Claude locale.
//!
//! Le CLI emet des evenements `rate_limit_event` decrivant le remplissage des
//! fenetres glissantes (5 h et 7 j). On s'en sert pour router les missions :
//! quand le quota Claude est presque epuise, mieux vaut confier la mission a
//! un autre backend que de la voir echouer en cours de route.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use serde_json::Value;

/// Etat partage, mis a jour par le lecteur du flux `claude` et consulte par le
/// routeur de missions. Les taux sont stockes en pour-mille pour rester sur des
/// entiers atomiques (pas de verrou sur un chemin chaud).
#[derive(Debug, Default)]
pub struct UsageState {
    five_hour_permille: AtomicU32,
    seven_day_permille: AtomicU32,
    resets_at: AtomicU64,
    seen: AtomicU32,
}

impl UsageState {
    pub fn update_from_event(&self, value: &Value) -> Option<(f32, f32, Option<u64>)> {
        let info = value.get("rate_limit_info")?;
        let windows = info.get("unifiedWindows")?;

        let five_hour = windows.pointer("/five_hour/utilization").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let seven_day = windows.pointer("/seven_day/utilization").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let resets_at = info.get("resetsAt").and_then(Value::as_u64);

        self.five_hour_permille.store((five_hour * 1000.0) as u32, Ordering::Relaxed);
        self.seven_day_permille.store((seven_day * 1000.0) as u32, Ordering::Relaxed);
        self.resets_at.store(resets_at.unwrap_or(0), Ordering::Relaxed);
        self.seen.store(1, Ordering::Relaxed);

        Some((five_hour, seven_day, resets_at))
    }

    /// Dernieres valeurs connues, pour l'instantane envoye a un client qui se
    /// connecte apres coup. `None` tant qu'aucun evenement n'a ete recu.
    pub fn snapshot(&self) -> Option<(f32, f32, Option<u64>)> {
        if self.seen.load(Ordering::Relaxed) == 0 {
            return None;
        }
        let five = self.five_hour_permille.load(Ordering::Relaxed) as f32 / 1000.0;
        let seven = self.seven_day_permille.load(Ordering::Relaxed) as f32 / 1000.0;
        let resets_at = match self.resets_at.load(Ordering::Relaxed) {
            0 => None,
            value => Some(value),
        };
        Some((five, seven, resets_at))
    }

    /// Taux de remplissage de la fenetre la plus contraignante, `None` tant
    /// qu'aucun evenement de quota n'a ete recu.
    pub fn worst_utilization(&self) -> Option<f32> {
        if self.seen.load(Ordering::Relaxed) == 0 {
            return None;
        }
        let five = self.five_hour_permille.load(Ordering::Relaxed);
        let seven = self.seven_day_permille.load(Ordering::Relaxed);
        Some(five.max(seven) as f32 / 1000.0)
    }
}
