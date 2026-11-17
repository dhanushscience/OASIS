//! The internet connection as Windows sees it: none, metered (a mobile hotspot, a capped plan,
//! roaming) or ordinary. Large voice model downloads wait on a metered one (`docs/VOICE_DECISIONS.md`
//! V30).

use windows::Networking::Connectivity::{NetworkCostType, NetworkInformation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    Offline,
    /// Paid by use, roaming or over its data limit.
    Metered,
    Unmetered,
}

/// The connection now. Anything Windows will not answer counts as unmetered, so a download is
/// never held back by a failed question.
pub fn connection() -> Connection {
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else { return Connection::Offline };
    let Ok(cost) = profile.GetConnectionCost() else { return Connection::Unmetered };
    let kind = cost.NetworkCostType().unwrap_or(NetworkCostType::Unrestricted);
    let metered = matches!(kind, NetworkCostType::Fixed | NetworkCostType::Variable)
        || cost.Roaming().unwrap_or(false)
        || cost.OverDataLimit().unwrap_or(false)
        || cost.ApproachingDataLimit().unwrap_or(false);
    if metered { Connection::Metered } else { Connection::Unmetered }
}

#[cfg(test)]
mod tests {
    #[test]
    fn asking_never_fails() {
        let _ = super::connection();
    }
}
