use crate::{
    common::{AppError, AppState},
    entities::{server, server_machine, ServerMachine},
};
use axum::{
    extract::{FromRequestParts, Query},
    http::request::Parts,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct ServerQuery {
    pub token: Option<String>,
    pub node_id: Option<String>,
    pub node_type: Option<String>,
    pub machine_id: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedNode {
    pub server: server::Model,
    pub machine: Option<server_machine::Model>,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedMachine {
    pub machine: server_machine::Model,
}

#[derive(Debug, Clone)]
pub struct NodeOrMachineAuth {
    pub server: Option<server::Model>,
    pub machine: Option<server_machine::Model>,
}

impl FromRequestParts<AppState> for AuthenticatedNode {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let query_params: ServerQuery = Query::try_from_uri(&parts.uri)
            .map(|Query(q)| q)
            .unwrap_or_default();

        let token = query_params
            .token
            .or_else(|| {
                parts
                    .headers
                    .get("token")
                    .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            })
            .or_else(|| {
                parts
                    .headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.to_string()))
            })
            .ok_or_else(|| AppError::Unauthorized("Invalid token".into()))?;

        let node_id = query_params
            .node_id
            .or_else(|| {
                parts
                    .headers
                    .get("node-id")
                    .or_else(|| parts.headers.get("node_id"))
                    .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            })
            .ok_or_else(|| AppError::BadRequest("node_id is required".into()))?;

        let machine_id = query_params.machine_id.or_else(|| {
            parts
                .headers
                .get("machine-id")
                .or_else(|| parts.headers.get("machine_id"))
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<i32>().ok())
        });

        let node_type = query_params.node_type.or_else(|| {
            parts
                .headers
                .get("node-type")
                .or_else(|| parts.headers.get("node_type"))
                .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
        });

        if let Some(m_id) = machine_id {
            let machine = ServerMachine::find()
                .filter(server_machine::Column::Id.eq(m_id))
                .filter(server_machine::Column::Token.eq(&token))
                .one(&state.db)
                .await?
                .ok_or_else(|| {
                    AppError::Unauthorized("Machine not found or invalid token".into())
                })?;

            if !machine.is_active {
                return Err(AppError::Forbidden("Machine is disabled".into()));
            }

            let server = state
                .server_service
                .get_server(&node_id, node_type.as_deref())
                .await?
                .ok_or_else(|| AppError::NotFound("Server does not exist".into()))?;

            if server.machine_id != Some(machine.id) {
                return Err(AppError::Forbidden("Node not found on this machine".into()));
            }

            Ok(AuthenticatedNode {
                server,
                machine: Some(machine),
            })
        } else {
            let expected_token = state.setting_service.get_string("server_token", "").await;
            if token != expected_token {
                return Err(AppError::Unauthorized("Invalid token".into()));
            }

            let server = state
                .server_service
                .get_server(&node_id, node_type.as_deref())
                .await?
                .ok_or_else(|| AppError::NotFound("Server does not exist".into()))?;

            Ok(AuthenticatedNode {
                server,
                machine: None,
            })
        }
    }
}

impl FromRequestParts<AppState> for AuthenticatedMachine {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let query_params: ServerQuery = Query::try_from_uri(&parts.uri)
            .map(|Query(q)| q)
            .unwrap_or_default();

        let token = query_params
            .token
            .or_else(|| {
                parts
                    .headers
                    .get("token")
                    .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            })
            .ok_or_else(|| AppError::Unauthorized("Invalid token".into()))?;

        let machine_id = query_params
            .machine_id
            .or_else(|| {
                parts
                    .headers
                    .get("machine-id")
                    .or_else(|| parts.headers.get("machine_id"))
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<i32>().ok())
            })
            .ok_or_else(|| AppError::BadRequest("machine_id is required".into()))?;

        let machine = ServerMachine::find()
            .filter(server_machine::Column::Id.eq(machine_id))
            .filter(server_machine::Column::Token.eq(&token))
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Unauthorized("Machine not found or invalid token".into()))?;

        if !machine.is_active {
            return Err(AppError::Forbidden("Machine is disabled".into()));
        }

        Ok(AuthenticatedMachine { machine })
    }
}

impl FromRequestParts<AppState> for NodeOrMachineAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let query_params: ServerQuery = Query::try_from_uri(&parts.uri)
            .map(|Query(q)| q)
            .unwrap_or_default();

        let token = query_params
            .token
            .or_else(|| {
                parts
                    .headers
                    .get("token")
                    .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            })
            .ok_or_else(|| AppError::Unauthorized("Invalid token".into()))?;

        let machine_id = query_params.machine_id.or_else(|| {
            parts
                .headers
                .get("machine-id")
                .or_else(|| parts.headers.get("machine_id"))
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<i32>().ok())
        });

        let node_id = query_params.node_id.or_else(|| {
            parts
                .headers
                .get("node-id")
                .or_else(|| parts.headers.get("node_id"))
                .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
        });

        let node_type = query_params.node_type.or_else(|| {
            parts
                .headers
                .get("node-type")
                .or_else(|| parts.headers.get("node_type"))
                .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
        });

        if let Some(m_id) = machine_id {
            let machine = ServerMachine::find()
                .filter(server_machine::Column::Id.eq(m_id))
                .filter(server_machine::Column::Token.eq(&token))
                .one(&state.db)
                .await?
                .ok_or_else(|| {
                    AppError::Unauthorized("Machine not found or invalid token".into())
                })?;

            if !machine.is_active {
                return Err(AppError::Forbidden("Machine is disabled".into()));
            }

            let server = if let Some(ref nid) = node_id {
                state
                    .server_service
                    .get_server(nid, node_type.as_deref())
                    .await?
            } else {
                None
            };

            Ok(NodeOrMachineAuth {
                server,
                machine: Some(machine),
            })
        } else {
            let expected_token = state.setting_service.get_string("server_token", "").await;
            if token != expected_token {
                return Err(AppError::Unauthorized("Invalid token".into()));
            }

            let server = if let Some(ref nid) = node_id {
                state
                    .server_service
                    .get_server(nid, node_type.as_deref())
                    .await?
            } else {
                None
            };

            Ok(NodeOrMachineAuth {
                server,
                machine: None,
            })
        }
    }
}
