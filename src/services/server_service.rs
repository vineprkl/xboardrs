use crate::{
    common::AppError,
    entities::{
        server, server_machine, server_machine_load_history, server_route,
        stat::{server as stat_server, user as stat_user},
        user, Server, ServerRoute, User,
    },
    services::{DeviceStateService, SettingService},
    utils::get_server_key,
};
use chrono::{Datelike, Local, NaiveDate, Utc};
use sea_orm::{
    sea_query::{Expr, ExprTrait},
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use stat_server::Entity as StatServer;
use stat_user::Entity as StatUser;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserNodeItem {
    pub id: i32,
    pub uuid: String,
    pub speed_limit: Option<i32>,
    pub device_limit: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RateTimeRange {
    pub start: String,
    pub end: String,
    pub rate: f64,
}

#[derive(Clone)]
pub struct ServerService {
    db: DatabaseConnection,
    setting_service: SettingService,
    device_state_service: DeviceStateService,
    // Node runtime state caches
    last_check_at: Arc<RwLock<HashMap<i32, i64>>>,
    last_push_at: Arc<RwLock<HashMap<i32, i64>>>,
    online_users: Arc<RwLock<HashMap<i32, usize>>>,
    node_status: Arc<RwLock<HashMap<i32, serde_json::Value>>>,
    node_metrics: Arc<RwLock<HashMap<i32, serde_json::Value>>>,
}

impl ServerService {
    pub fn new(
        db: DatabaseConnection,
        setting_service: SettingService,
        device_state_service: DeviceStateService,
    ) -> Self {
        Self {
            db,
            setting_service,
            device_state_service,
            last_check_at: Arc::new(RwLock::new(HashMap::new())),
            last_push_at: Arc::new(RwLock::new(HashMap::new())),
            online_users: Arc::new(RwLock::new(HashMap::new())),
            node_status: Arc::new(RwLock::new(HashMap::new())),
            node_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Normalizes protocol type aliases.
    pub fn normalize_type(server_type: &str) -> String {
        let s = server_type.to_lowercase();
        match s.as_str() {
            "v2ray" => "vmess".to_string(),
            "hysteria2" => "hysteria".to_string(),
            _ => s,
        }
    }

    /// Retrieves server by numeric ID or unique code.
    pub async fn get_server(
        &self,
        server_id: &str,
        server_type: Option<&str>,
    ) -> Result<Option<server::Model>, AppError> {
        let norm_type = server_type.map(Self::normalize_type);
        let mut query = Server::find();

        if let Some(ref t) = norm_type {
            query = query.filter(server::Column::Type.eq(t.as_str()));
        }

        if let Ok(id) = server_id.parse::<i32>() {
            query = query.filter(
                server::Column::Id
                    .eq(id)
                    .or(server::Column::Code.eq(server_id)),
            );
        } else {
            query = query.filter(server::Column::Code.eq(server_id));
        }

        let servers = query.all(&self.db).await?;
        // Priority: exact code match first, then ID match
        let matched = servers.into_iter().min_by_key(|s| {
            if s.code.as_deref() == Some(server_id) {
                0
            } else {
                1
            }
        });

        Ok(matched)
    }

    /// Computes the active traffic multiplier for a server based on time ranges.
    pub fn get_current_rate(&self, server: &server::Model) -> f64 {
        if !server.rate_time_enable {
            return server.rate;
        }

        if let Some(ref ranges_str) = server.rate_time_ranges {
            if let Ok(ranges) = serde_json::from_str::<Vec<RateTimeRange>>(ranges_str) {
                let now = Local::now().format("%H:%M").to_string();
                for r in ranges {
                    if now >= r.start && now <= r.end {
                        return r.rate;
                    }
                }
            }
        }

        server.rate
    }

    /// Marks server heartbeat in memory.
    pub async fn touch_node(&self, node_id: i32) {
        let now = Utc::now().timestamp();
        let mut check_at = self.last_check_at.write().await;
        check_at.insert(node_id, now);
    }

    /// Gets last check-in timestamp.
    pub async fn get_last_check_at(&self, node_id: i32) -> Option<i64> {
        let check_at = self.last_check_at.read().await;
        check_at.get(&node_id).copied()
    }

    /// Retrieves all active users entitled to connect to this server.
    pub async fn get_available_users(
        &self,
        server: &server::Model,
    ) -> Result<Vec<UserNodeItem>, AppError> {
        let group_ids: Vec<i32> = if let Some(ref raw) = server.group_ids {
            if let Ok(nums) = serde_json::from_str::<Vec<i32>>(raw) {
                nums
            } else if let Ok(strs) = serde_json::from_str::<Vec<String>>(raw) {
                strs.iter().filter_map(|s| s.parse::<i32>().ok()).collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        if group_ids.is_empty() {
            return Ok(vec![]);
        }

        let now = Utc::now().timestamp();
        let users = User::find()
            .filter(user::Column::GroupId.is_in(group_ids))
            .filter(user::Column::Banned.eq(false))
            .filter(
                user::Column::ExpiredAt
                    .is_null()
                    .or(user::Column::ExpiredAt.gte(now)),
            )
            .filter(
                Expr::col(user::Column::U)
                    .add(Expr::col(user::Column::D))
                    .lt(Expr::col(user::Column::TransferEnable)),
            )
            .all(&self.db)
            .await?;

        let items = users
            .into_iter()
            .map(|u| UserNodeItem {
                id: u.id,
                uuid: u.uuid,
                speed_limit: u.speed_limit,
                device_limit: u.device_limit,
            })
            .collect();

        Ok(items)
    }

    /// Retrieves all available servers visible to a specific user.
    pub async fn get_available_servers_for_user(
        &self,
        user: &user::Model,
    ) -> Result<Vec<server::Model>, AppError> {
        let gid = match user.group_id {
            Some(g) => g,
            None => return Ok(vec![]),
        };

        let servers = Server::find()
            .filter(server::Column::Show.eq(true))
            .filter(
                server::Column::TransferEnable
                    .is_null()
                    .or(server::Column::TransferEnable.eq(0))
                    .or(Expr::col(server::Column::U)
                        .add(Expr::col(server::Column::D))
                        .lt(Expr::col(server::Column::TransferEnable))),
            )
            .order_by_asc(server::Column::Sort)
            .all(&self.db)
            .await?;

        let mut available = Vec::new();
        for mut s in servers {
            let in_group = if let Some(ref raw) = s.group_ids {
                if let Ok(nums) = serde_json::from_str::<Vec<i32>>(raw) {
                    nums.contains(&gid)
                } else if let Ok(strs) = serde_json::from_str::<Vec<String>>(raw) {
                    strs.iter().any(|st| st.parse::<i32>().ok() == Some(gid))
                } else {
                    false
                }
            } else {
                false
            };

            if in_group {
                s.port = crate::utils::resolve_port(&s.port);
                available.push(s);
            }
        }

        Ok(available)
    }

    /// Builds UniProxy node configuration JSON.
    pub async fn build_node_config(
        &self,
        server: &server::Model,
    ) -> Result<serde_json::Value, AppError> {
        let protocol_settings: serde_json::Value = server
            .protocol_settings
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::json!({}));

        let network = protocol_settings
            .get("network")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let network_settings = protocol_settings
            .get("network_settings")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let mut base = json!({
            "protocol": server.r#type,
            "listen_ip": "0.0.0.0",
            "server_port": server.server_port,
            "network": network,
            "networkSettings": network_settings,
        });

        match server.r#type.as_str() {
            "shadowsocks" => {
                let cipher = protocol_settings["cipher"].as_str().unwrap_or("");
                let server_key = match cipher {
                    "2022-blake3-aes-128-gcm" => Some(get_server_key(server.created_at, 16)),
                    "2022-blake3-aes-256-gcm" => Some(get_server_key(server.created_at, 32)),
                    _ => None,
                };
                base["cipher"] = protocol_settings["cipher"].clone();
                base["plugin"] = protocol_settings["plugin"].clone();
                base["plugin_opts"] = protocol_settings["plugin_opts"].clone();
                base["server_key"] = json!(server_key);
            }
            "vmess" => {
                base["tls"] = protocol_settings["tls"].clone();
                base["tls_settings"] = protocol_settings["tls_settings"].clone();
                base["multiplex"] = protocol_settings["multiplex"].clone();
            }
            "trojan" => {
                let tls = protocol_settings["tls"].as_i64().unwrap_or(0);
                base["host"] = json!(server.host);
                base["server_name"] = protocol_settings["tls_settings"]["server_name"].clone();
                base["multiplex"] = protocol_settings["multiplex"].clone();
                base["tls"] = json!(tls);
                base["tls_settings"] = if tls == 2 {
                    protocol_settings["reality_settings"].clone()
                } else {
                    protocol_settings["tls_settings"].clone()
                };
            }
            "vless" => {
                let tls = protocol_settings["tls"].as_i64().unwrap_or(0);
                base["tls"] = json!(tls);
                base["flow"] = protocol_settings["flow"].clone();
                if protocol_settings["encryption"]["enabled"]
                    .as_bool()
                    .unwrap_or(false)
                {
                    base["decryption"] = protocol_settings["encryption"]["decryption"].clone();
                }
                base["tls_settings"] = if tls == 2 {
                    protocol_settings["reality_settings"].clone()
                } else {
                    protocol_settings["tls_settings"].clone()
                };
                base["multiplex"] = protocol_settings["multiplex"].clone();
            }
            "hysteria" => {
                let version = protocol_settings["version"].as_i64().unwrap_or(2);
                base["version"] = json!(version);
                base["host"] = json!(server.host);
                base["server_name"] = protocol_settings["tls"]["server_name"].clone();
                base["tls_settings"] = protocol_settings["tls"].clone();
                base["up_mbps"] = protocol_settings["bandwidth"]["up"].clone();
                base["down_mbps"] = protocol_settings["bandwidth"]["down"].clone();
                if version == 1 {
                    base["obfs"] = protocol_settings["obfs"]["password"].clone();
                } else if version == 2 {
                    let open = protocol_settings["obfs"]["open"].as_bool().unwrap_or(false);
                    base["obfs"] = if open {
                        protocol_settings["obfs"]["type"].clone()
                    } else {
                        serde_json::Value::Null
                    };
                    base["obfs-password"] = protocol_settings["obfs"]["password"].clone();
                }
            }
            "tuic" => {
                base["version"] = protocol_settings["version"].clone();
                base["server_name"] = protocol_settings["tls"]["server_name"].clone();
                base["congestion_control"] = protocol_settings["congestion_control"].clone();
                base["tls_settings"] = protocol_settings["tls"].clone();
                base["auth_timeout"] = json!("3s");
                base["zero_rtt_handshake"] = json!(false);
                base["heartbeat"] = json!("3s");
            }
            "anytls" => {
                base["server_name"] = protocol_settings["tls"]["server_name"].clone();
                base["tls_settings"] = protocol_settings["tls"].clone();
                base["padding_scheme"] = protocol_settings["padding_scheme"].clone();
            }
            "socks" => {
                base["tls"] = protocol_settings["tls"].clone();
                base["tls_settings"] = protocol_settings["tls_settings"].clone();
            }
            "naive" | "http" => {
                base["tls"] = protocol_settings["tls"].clone();
                base["tls_settings"] = protocol_settings["tls_settings"].clone();
            }
            "mieru" => {
                base["transport"] = protocol_settings
                    .get("transport")
                    .cloned()
                    .unwrap_or(json!("TCP"));
                base["traffic_pattern"] = protocol_settings["traffic_pattern"].clone();
            }
            _ => {}
        }

        // Attach server routes if configured
        if let Some(ref route_ids_str) = server.route_ids {
            let route_ids: Vec<i32> =
                if let Ok(ids) = serde_json::from_str::<Vec<i32>>(route_ids_str) {
                    ids
                } else if let Ok(strs) = serde_json::from_str::<Vec<String>>(route_ids_str) {
                    strs.iter().filter_map(|s| s.parse::<i32>().ok()).collect()
                } else {
                    vec![]
                };

            if !route_ids.is_empty() {
                let routes = ServerRoute::find()
                    .filter(server_route::Column::Id.is_in(route_ids))
                    .all(&self.db)
                    .await?;

                let route_list: Vec<_> = routes
                    .into_iter()
                    .map(|r| {
                        json!({
                            "id": r.id,
                            "match": r.r#match,
                            "action": r.action,
                            "action_value": r.action_value,
                        })
                    })
                    .collect();
                base["routes"] = json!(route_list);
            }
        }

        // Attach custom outbounds, custom routes and cert_config
        if let Some(ref outbounds) = server.custom_outbounds {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(outbounds) {
                base["custom_outbounds"] = v;
            }
        }

        if let Some(ref routes) = server.custom_routes {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(routes) {
                base["custom_routes"] = v;
            }
        }

        if let Some(ref cert) = server.cert_config {
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(cert) {
                if let Some(mode) = v.get("mode").cloned() {
                    if v.get("cert_mode").is_none() {
                        v["cert_mode"] = mode;
                    }
                }
                if v.get("cert_mode").and_then(|m| m.as_str()) != Some("none") {
                    base["cert_config"] = v;
                }
            }
        }

        // Attach base intervals
        let push_interval = self
            .setting_service
            .get_int("server_push_interval", 60)
            .await;
        let pull_interval = self
            .setting_service
            .get_int("server_pull_interval", 60)
            .await;
        base["base_config"] = json!({
            "push_interval": push_interval,
            "pull_interval": pull_interval,
        });

        Ok(base)
    }

    /// Processes batch traffic report from a node.
    pub async fn process_traffic(
        &self,
        server: &server::Model,
        traffic: HashMap<i32, [i64; 2]>,
    ) -> Result<(), AppError> {
        if traffic.is_empty() {
            return Ok(());
        }

        let rate = self.get_current_rate(server);
        let now = Utc::now().timestamp();
        let today = Utc::now();
        let record_at = NaiveDate::from_ymd_opt(today.year(), today.month(), today.day())
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();

        let mut total_u: i64 = 0;
        let mut total_d: i64 = 0;

        for (&uid, &[u_raw, d_raw]) in traffic.iter() {
            if u_raw <= 0 && d_raw <= 0 {
                continue;
            }

            let u_actual = (u_raw as f64 * rate).round() as i64;
            let d_actual = (d_raw as f64 * rate).round() as i64;
            total_u += u_raw;
            total_d += d_raw;

            // 1. Update user total traffic atomically
            if let Some(u_model) = User::find_by_id(uid).one(&self.db).await? {
                let mut u_active: user::ActiveModel = u_model.into();
                u_active.u = Set(u_active.u.unwrap() + u_actual);
                u_active.d = Set(u_active.d.unwrap() + d_actual);
                u_active.t = Set(now as i32);
                u_active.update(&self.db).await?;
            }

            // 2. Record / increment StatUser
            let existing_stat = StatUser::find()
                .filter(stat_user::Column::UserId.eq(uid))
                .filter(stat_user::Column::ServerRate.eq(rate))
                .filter(stat_user::Column::RecordAt.eq(record_at))
                .filter(stat_user::Column::RecordType.eq("d"))
                .one(&self.db)
                .await?;

            if let Some(stat) = existing_stat {
                let mut active: stat_user::ActiveModel = stat.into();
                active.u = Set(active.u.unwrap() + u_actual);
                active.d = Set(active.d.unwrap() + d_actual);
                active.updated_at = Set(now);
                active.update(&self.db).await?;
            } else {
                let active = stat_user::ActiveModel {
                    user_id: Set(uid),
                    server_rate: Set(rate),
                    u: Set(u_actual),
                    d: Set(d_actual),
                    record_type: Set("d".to_string()),
                    record_at: Set(record_at),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                };
                active.insert(&self.db).await?;
            }
        }

        // 3. Update server total traffic atomically
        if let Some(s_model) = Server::find_by_id(server.id).one(&self.db).await? {
            let mut s_active: server::ActiveModel = s_model.into();
            s_active.u = Set(Some(s_active.u.unwrap().unwrap_or(0) + total_u));
            s_active.d = Set(Some(s_active.d.unwrap().unwrap_or(0) + total_d));
            s_active.updated_at = Set(now);
            s_active.update(&self.db).await?;
        }

        // 4. Record / increment StatServer
        let existing_server_stat = StatServer::find()
            .filter(stat_server::Column::ServerId.eq(server.id))
            .filter(stat_server::Column::ServerType.eq(&server.r#type))
            .filter(stat_server::Column::RecordAt.eq(record_at))
            .filter(stat_server::Column::RecordType.eq("d"))
            .one(&self.db)
            .await?;

        if let Some(stat) = existing_server_stat {
            let mut active: stat_server::ActiveModel = stat.into();
            active.u = Set(active.u.unwrap() + total_u);
            active.d = Set(active.d.unwrap() + total_d);
            active.updated_at = Set(now);
            active.update(&self.db).await?;
        } else {
            let active = stat_server::ActiveModel {
                server_id: Set(server.id),
                server_type: Set(server.r#type.clone()),
                u: Set(total_u),
                d: Set(total_d),
                record_type: Set("d".to_string()),
                record_at: Set(record_at),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };
            active.insert(&self.db).await?;
        }

        // 5. Update runtime caches
        {
            let mut online = self.online_users.write().await;
            online.insert(server.id, traffic.len());
            let mut last_push = self.last_push_at.write().await;
            last_push.insert(server.id, now);
        }

        Ok(())
    }

    /// Records online device IPs from a node.
    pub async fn process_alive(&self, node_id: i32, alive: HashMap<i32, Vec<String>>) {
        for (uid, ips) in alive {
            self.device_state_service
                .set_devices(uid, node_id, &ips)
                .await;
        }
    }

    /// Records server load status in cache.
    pub async fn process_status(&self, node_id: i32, status: serde_json::Value) {
        let mut node_status = self.node_status.write().await;
        node_status.insert(node_id, status);
    }

    /// Records server telemetry metrics in cache.
    pub async fn update_metrics(&self, node_id: i32, metrics: serde_json::Value) {
        let mut node_metrics = self.node_metrics.write().await;
        node_metrics.insert(node_id, metrics);
    }

    /// Retrieves enabled nodes assigned to a machine.
    pub async fn get_machine_nodes(
        &self,
        machine: &server_machine::Model,
    ) -> Result<Vec<server::Model>, AppError> {
        let nodes = Server::find()
            .filter(server::Column::MachineId.eq(machine.id))
            .filter(server::Column::Enabled.eq(true))
            .order_by_asc(server::Column::Sort)
            .all(&self.db)
            .await?;
        Ok(nodes)
    }

    /// Records machine load status and load history.
    pub async fn process_machine_status(
        &self,
        machine: &server_machine::Model,
        status: serde_json::Value,
        net_in_speed: Option<f64>,
        net_out_speed: Option<f64>,
    ) -> Result<(), AppError> {
        let now = Utc::now().timestamp();

        // 1. Update machine load_status and last_seen_at
        let mut status_val = status.clone();
        if let Some(map) = status_val.as_object_mut() {
            map.insert("updated_at".to_string(), serde_json::json!(now));
        }

        let mut m_active: server_machine::ActiveModel = machine.clone().into();
        m_active.load_status = Set(Some(status_val.to_string()));
        m_active.last_seen_at = Set(Some(now));
        m_active.updated_at = Set(now);
        m_active.update(&self.db).await?;

        // 2. Insert load history
        let cpu = status.get("cpu").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let mem_total = status["mem"]["total"].as_i64().unwrap_or(0);
        let mem_used = status["mem"]["used"].as_i64().unwrap_or(0);
        let disk_total = status["disk"]["total"].as_i64().unwrap_or(0);
        let disk_used = status["disk"]["used"].as_i64().unwrap_or(0);

        let history = server_machine_load_history::ActiveModel {
            machine_id: Set(machine.id),
            cpu: Set(cpu),
            mem_total: Set(mem_total),
            mem_used: Set(mem_used),
            disk_total: Set(disk_total),
            disk_used: Set(disk_used),
            recorded_at: Set(now),
            net_in_speed: Set(net_in_speed),
            net_out_speed: Set(net_out_speed),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        history.insert(&self.db).await?;

        Ok(())
    }

    /// Checks if a node is currently online based on recent heartbeats (< 300 seconds).
    pub async fn is_node_online(&self, node_id: i32) -> bool {
        let last_check = self.last_check_at.read().await;
        if let Some(&ts) = last_check.get(&node_id) {
            return (chrono::Utc::now().timestamp() - ts) < 300;
        }
        false
    }

    /// Returns the number of online users reported by this node.
    pub async fn get_node_online_users(&self, node_id: i32) -> usize {
        let online = self.online_users.read().await;
        online.get(&node_id).copied().unwrap_or(0)
    }
}
