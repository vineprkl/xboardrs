use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::RwLock;

const TTL: i64 = 300; // 5 minutes TTL for device state

/// In-memory thread-safe device state tracker matching Laravel's DeviceStateService.
#[derive(Clone, Default)]
pub struct DeviceStateService {
    // user_id -> { "node_id:ip" -> timestamp }
    devices: Arc<RwLock<HashMap<i32, HashMap<String, i64>>>>,
}

impl DeviceStateService {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Normalizes IP addresses by removing port suffixes.
    /// E.g. "1.2.3.4:5678" -> "1.2.3.4", "[::1]:443" -> "::1"
    pub fn normalize_ip(ip: &str) -> String {
        let trimmed = ip.trim();
        // Check IPv6 bracketed format [::1]:port
        if trimmed.starts_with('[') {
            if let Some(end_bracket) = trimmed.find(']') {
                return trimmed[1..end_bracket].to_string();
            }
        }
        // Check IPv4 format with port: 1.2.3.4:1234
        if let Some((addr, port)) = trimmed.rsplit_once(':') {
            if port.chars().all(|c| c.is_ascii_digit()) && addr.contains('.') {
                return addr.to_string();
            }
        }
        trimmed.to_string()
    }

    /// Records online device IPs for a given user and node.
    pub async fn set_devices(&self, user_id: i32, node_id: i32, ips: &[String]) {
        let now = chrono::Utc::now().timestamp();
        let prefix = format!("{}:", node_id);

        let mut all_devices = self.devices.write().await;
        let user_map = all_devices.entry(user_id).or_default();

        // 1. Remove old records for this node
        user_map.retain(|k, _| !k.starts_with(&prefix));

        // 2. Normalize and deduplicate IPs
        let mut unique_ips = HashSet::new();
        for ip in ips {
            let norm = Self::normalize_ip(ip);
            if !norm.is_empty() {
                unique_ips.insert(norm);
            }
        }

        // 3. Insert new records with current timestamp
        for ip in unique_ips {
            user_map.insert(format!("{}{}", prefix, ip), now);
        }
    }

    /// Returns the number of distinct active IP addresses for a user across all nodes.
    pub async fn get_device_count(&self, user_id: i32) -> usize {
        let now = chrono::Utc::now().timestamp();
        let all_devices = self.devices.read().await;

        if let Some(user_map) = all_devices.get(&user_id) {
            let mut unique_ips = HashSet::new();
            for (field, ts) in user_map.iter() {
                if now - *ts <= TTL {
                    if let Some((_, ip)) = field.split_once(':') {
                        unique_ips.insert(ip);
                    }
                }
            }
            unique_ips.len()
        } else {
            0
        }
    }

    /// Returns alive count for specified users where count > 0.
    pub async fn get_alive_list(&self, user_ids: &[i32]) -> HashMap<i32, usize> {
        let mut result = HashMap::new();
        for &uid in user_ids {
            let count = self.get_device_count(uid).await;
            if count > 0 {
                result.insert(uid, count);
            }
        }
        result
    }

    /// Retrieves active devices for a node: { user_id -> [ip1, ip2, ...] }
    pub async fn get_node_devices(&self, node_id: i32) -> HashMap<i32, Vec<String>> {
        let now = chrono::Utc::now().timestamp();
        let prefix = format!("{}:", node_id);
        let all_devices = self.devices.read().await;

        let mut result = HashMap::new();
        for (&uid, user_map) in all_devices.iter() {
            let mut ips = Vec::new();
            for (field, &ts) in user_map.iter() {
                if now - ts <= TTL && field.starts_with(&prefix) {
                    let ip = &field[prefix.len()..];
                    ips.push(ip.to_string());
                }
            }
            if !ips.is_empty() {
                result.insert(uid, ips);
            }
        }
        result
    }

    /// Clears all devices associated with a node (e.g. when node disconnects).
    pub async fn clear_all_node_devices(&self, node_id: i32) -> Vec<i32> {
        let prefix = format!("{}:", node_id);
        let mut all_devices = self.devices.write().await;
        let mut affected = Vec::new();

        for (&uid, user_map) in all_devices.iter_mut() {
            let before = user_map.len();
            user_map.retain(|k, _| !k.starts_with(&prefix));
            if user_map.len() != before {
                affected.push(uid);
            }
        }
        affected
    }

    /// Cleans up stale devices where now - ts > TTL.
    /// Returns the number of removed stale records.
    pub async fn cleanup_stale(&self) -> usize {
        let now = chrono::Utc::now().timestamp();
        let mut all_devices = self.devices.write().await;
        let mut removed = 0;

        for user_map in all_devices.values_mut() {
            let before = user_map.len();
            user_map.retain(|_, &mut ts| now - ts <= TTL);
            removed += before - user_map.len();
        }

        all_devices.retain(|_, map| !map.is_empty());
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ip_normalization() {
        assert_eq!(
            DeviceStateService::normalize_ip("192.168.1.1:8080"),
            "192.168.1.1"
        );
        assert_eq!(
            DeviceStateService::normalize_ip("[2001:db8::1]:443"),
            "2001:db8::1"
        );
        assert_eq!(DeviceStateService::normalize_ip("10.0.0.1"), "10.0.0.1");
        assert_eq!(DeviceStateService::normalize_ip("::1"), "::1");
    }

    #[tokio::test]
    async fn test_device_counting_and_deduplication() {
        let service = DeviceStateService::new();

        // Node 1 reports 2 IPs for user 10 (one duplicate when normalized)
        service
            .set_devices(
                10,
                1,
                &[
                    "1.1.1.1:1234".to_string(),
                    "1.1.1.1:5678".to_string(),
                    "2.2.2.2:1000".to_string(),
                ],
            )
            .await;

        assert_eq!(service.get_device_count(10).await, 2);

        // Node 2 reports 1 overlapping IP and 1 new IP for user 10
        service
            .set_devices(
                10,
                2,
                &["2.2.2.2:9999".to_string(), "3.3.3.3:8888".to_string()],
            )
            .await;

        // Total unique IPs across nodes: 1.1.1.1, 2.2.2.2, 3.3.3.3 => 3
        assert_eq!(service.get_device_count(10).await, 3);

        // Alive list
        let alive = service.get_alive_list(&[10, 20]).await;
        assert_eq!(alive.get(&10), Some(&3));
        assert_eq!(alive.get(&20), None);

        // Clear node 1
        service.clear_all_node_devices(1).await;
        // Remaining from node 2: 2.2.2.2, 3.3.3.3 => 2
        assert_eq!(service.get_device_count(10).await, 2);
    }
}
