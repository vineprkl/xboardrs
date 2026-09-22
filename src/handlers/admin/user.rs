use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    Json,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    common::{AdminTableQuery, ApiResponse, AppError, AppState, PaginatedResponse},
    entities::{user, Plan, ServerGroup, User},
    handlers::auth::AuthenticatedAdmin,
    utils::{generate_uuid, get_subscribe_url, hash_password, random_char},
};

#[derive(Debug, Deserialize)]
pub struct UserUpdateRequest {
    pub id: i32,
    pub email: Option<String>,
    pub password: Option<String>,
    pub balance: Option<f64>,
    pub commission_balance: Option<f64>,
    pub transfer_enable: Option<f64>, // GB
    pub expired_at: Option<i64>,
    pub plan_id: Option<i32>,
    pub group_id: Option<i32>,
    pub speed_limit: Option<i32>,
    pub device_limit: Option<i32>,
    pub banned: Option<bool>,
    pub is_admin: Option<bool>,
    pub is_staff: Option<bool>,
    pub discount: Option<i32>,
    pub remarks: Option<String>,
}

fn default_email_suffix() -> String {
    "@gmail.com".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UserGenerateRequest {
    pub email_prefix: Option<String>,
    #[serde(default = "default_email_suffix")]
    pub email_suffix: String,
    pub password: Option<String>,
    #[serde(alias = "generate_count")]
    pub count: usize,
    pub plan_id: Option<i32>,
    pub expired_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UserIdRequest {
    pub id: Option<i32>,
    pub user_ids: Option<Vec<i32>>,
    pub banned: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SetInviteUserRequest {
    pub id: i32,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetUserInfoQuery {
    pub id: i32,
}

/// GET or POST /api/v2/admin/user/fetch
pub async fn fetch(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<AdminTableQuery>,
    body: Option<Json<AdminTableQuery>>,
) -> Result<Response, AppError> {
    let q = body.map(|b| b.0).unwrap_or(query);

    let page = q.page();
    let per_page = q.per_page();
    let offset = q.offset();

    let mut select = User::find();

    // Apply filters
    for f in q.filters() {
        let field = f.id.as_str();
        match field {
            "email" => {
                if let Some(s) = f.value.as_str() {
                    select = select.filter(user::Column::Email.contains(s));
                }
            }
            "id" => {
                if let Some(n) = f.value.as_i64() {
                    select = select.filter(user::Column::Id.eq(n as i32));
                } else if let Some(arr) = f.value.as_array() {
                    let ids: Vec<i32> = arr
                        .iter()
                        .filter_map(|v| v.as_i64().map(|n| n as i32))
                        .collect();
                    select = select.filter(user::Column::Id.is_in(ids));
                }
            }
            "plan_id" => {
                if let Some(n) = f.value.as_i64() {
                    select = select.filter(user::Column::PlanId.eq(n as i32));
                } else if let Some(arr) = f.value.as_array() {
                    let ids: Vec<i32> = arr
                        .iter()
                        .filter_map(|v| v.as_i64().map(|n| n as i32))
                        .collect();
                    select = select.filter(user::Column::PlanId.is_in(ids));
                }
            }
            "group_id" | "group_ids" => {
                if let Some(n) = f.value.as_i64() {
                    select = select.filter(user::Column::GroupId.eq(n as i32));
                } else if let Some(arr) = f.value.as_array() {
                    let ids: Vec<i32> = arr
                        .iter()
                        .filter_map(|v| v.as_i64().map(|n| n as i32))
                        .collect();
                    select = select.filter(user::Column::GroupId.is_in(ids));
                }
            }
            "banned" => {
                if let Some(b) = f.value.as_bool() {
                    select = select.filter(user::Column::Banned.eq(b));
                } else if let Some(n) = f.value.as_i64() {
                    select = select.filter(user::Column::Banned.eq(n != 0));
                }
            }
            "is_admin" => {
                if let Some(b) = f.value.as_bool() {
                    select = select.filter(user::Column::IsAdmin.eq(b));
                } else if let Some(n) = f.value.as_i64() {
                    select = select.filter(user::Column::IsAdmin.eq(n != 0));
                }
            }
            _ => {}
        }
    }

    // Apply sorting
    let sorts = q.sorts();
    if sorts.is_empty() {
        select = select.order_by_desc(user::Column::Id);
    } else {
        for s in sorts {
            let col = match s.id.as_str() {
                "id" => user::Column::Id,
                "created_at" => user::Column::CreatedAt,
                "balance" => user::Column::Balance,
                "expired_at" => user::Column::ExpiredAt,
                _ => user::Column::Id,
            };
            if s.desc {
                select = select.order_by_desc(col);
            } else {
                select = select.order_by_asc(col);
            }
        }
    }

    let total = select.clone().count(&state.db).await?;
    let users = select.offset(offset).limit(per_page).all(&state.db).await?;

    let subscribe_path = state
        .setting_service
        .get_string("subscribe_path", "s")
        .await;
    let mut list = Vec::new();

    for u in users {
        let plan_obj = if let Some(pid) = u.plan_id {
            Plan::find_by_id(pid)
                .one(&state.db)
                .await?
                .map(|p| json!({ "id": p.id, "name": p.name }))
        } else {
            None
        };

        let invite_user_obj = if let Some(iid) = u.invite_user_id {
            User::find_by_id(iid)
                .one(&state.db)
                .await?
                .map(|inv| json!({ "id": inv.id, "email": inv.email }))
        } else {
            None
        };

        let group_obj = if let Some(gid) = u.group_id {
            ServerGroup::find_by_id(gid)
                .one(&state.db)
                .await?
                .map(|g| json!({ "id": g.id, "name": g.name }))
        } else {
            None
        };

        let app_url = state
            .setting_service
            .get_string("app_url", "http://127.0.0.1")
            .await;
        let sub_url = get_subscribe_url(&app_url, &subscribe_path, &u.token);

        let mut val = serde_json::to_value(&u).unwrap_or_default();
        if let Some(obj) = val.as_object_mut() {
            obj.insert("balance".to_string(), json!((u.balance as f64) / 100.0));
            obj.insert(
                "commission_balance".to_string(),
                json!((u.commission_balance as f64) / 100.0),
            );
            obj.insert("total_used".to_string(), json!(u.u + u.d));
            obj.insert("subscribe_url".to_string(), json!(sub_url));
            obj.insert("plan".to_string(), json!(plan_obj));
            obj.insert("invite_user".to_string(), json!(invite_user_obj));
            obj.insert("group".to_string(), json!(group_obj));
        }
        list.push(val);
    }

    Ok(PaginatedResponse::new(list, total, page, per_page).into_response())
}

/// GET /api/v2/admin/user/getUserInfoById
pub async fn get_user_info_by_id(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Query(query): Query<GetUserInfoQuery>,
) -> Result<Response, AppError> {
    let u = User::find_by_id(query.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "用户不存在".to_string()))?;

    let invite_user_obj = if let Some(iid) = u.invite_user_id {
        User::find_by_id(iid)
            .one(&state.db)
            .await?
            .map(|inv| json!({ "id": inv.id, "email": inv.email }))
    } else {
        None
    };

    let mut val = serde_json::to_value(&u).unwrap_or_default();
    if let Some(obj) = val.as_object_mut() {
        obj.insert("invite_user".to_string(), json!(invite_user_obj));
    }

    Ok(ApiResponse::success(val).into_response())
}

/// POST /api/v2/admin/user/update
pub async fn update(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<UserUpdateRequest>,
) -> Result<Response, AppError> {
    let u = User::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "用户不存在".to_string()))?;

    let now = chrono::Utc::now().timestamp();
    let mut u_active: user::ActiveModel = u.clone().into();

    if let Some(email) = payload.email {
        if email != u.email {
            let exists = User::find()
                .filter(user::Column::Email.eq(&email))
                .one(&state.db)
                .await?;
            if exists.is_some() {
                return Err(AppError::Custom(400201, "邮箱已被使用".to_string()));
            }
            u_active.email = Set(email);
        }
    }

    if let Some(pwd) = payload.password {
        if !pwd.trim().is_empty() {
            let hashed = hash_password(&pwd).map_err(|e| AppError::Internal(e.to_string()))?;
            u_active.password = Set(hashed);
        }
    }

    if let Some(bal) = payload.balance {
        u_active.balance = Set((bal * 100.0).round() as i32);
    }
    if let Some(cb) = payload.commission_balance {
        u_active.commission_balance = Set((cb * 100.0).round() as i32);
    }
    if let Some(te) = payload.transfer_enable {
        u_active.transfer_enable = Set((te * 1_073_741_824.0).round() as i64);
    }
    if payload.expired_at.is_some() {
        u_active.expired_at = Set(payload.expired_at);
    }
    if payload.plan_id.is_some() {
        u_active.plan_id = Set(payload.plan_id);
    }
    if payload.group_id.is_some() {
        u_active.group_id = Set(payload.group_id);
    }
    if payload.speed_limit.is_some() {
        u_active.speed_limit = Set(payload.speed_limit);
    }
    if payload.device_limit.is_some() {
        u_active.device_limit = Set(payload.device_limit);
    }
    if let Some(b) = payload.banned {
        u_active.banned = Set(b);
    }
    if let Some(admin) = payload.is_admin {
        u_active.is_admin = Set(admin);
    }
    if let Some(staff) = payload.is_staff {
        u_active.is_staff = Set(staff);
    }
    if payload.discount.is_some() {
        u_active.discount = Set(payload.discount);
    }
    if payload.remarks.is_some() {
        u_active.remarks = Set(payload.remarks);
    }

    u_active.updated_at = Set(now);
    u_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/user/generate
pub async fn generate(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<UserGenerateRequest>,
) -> Result<Response, AppError> {
    if payload.count == 0 || payload.count > 1000 {
        return Err(AppError::Custom(422, "生成数量范围为 1-1000".to_string()));
    }

    let default_pwd = payload.password.unwrap_or_else(|| "12345678".to_string());
    let hashed = hash_password(&default_pwd).map_err(|e| AppError::Internal(e.to_string()))?;
    let now = chrono::Utc::now().timestamp();

    let txn = state.db.begin().await?;
    let mut generated_emails = Vec::new();

    for i in 0..payload.count {
        let prefix = payload
            .email_prefix
            .clone()
            .unwrap_or_else(|| random_char(8, false));
        let email = format!(
            "{}_{}@{}",
            prefix,
            i + 1,
            payload.email_suffix.trim_start_matches('@')
        );
        let token = random_char(32, false);
        let uuid = generate_uuid();

        let new_user = user::ActiveModel {
            email: Set(email.clone()),
            password: Set(hashed.clone()),
            token: Set(token),
            uuid: Set(uuid),
            plan_id: Set(payload.plan_id),
            expired_at: Set(payload.expired_at),
            balance: Set(0),
            commission_balance: Set(0),
            commission_type: Set(0),
            t: Set(0),
            u: Set(0),
            d: Set(0),
            transfer_enable: Set(10_737_418_240), // 10GB default
            banned: Set(false),
            is_admin: Set(false),
            is_staff: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        new_user.insert(&txn).await?;
        generated_emails.push(email);
    }

    txn.commit().await?;
    Ok(ApiResponse::success(generated_emails).into_response())
}

/// POST /api/v2/admin/user/ban
pub async fn ban(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<UserIdRequest>,
) -> Result<Response, AppError> {
    let banned = payload.banned.unwrap_or(true);
    let now = chrono::Utc::now().timestamp();

    if let Some(id) = payload.id {
        if let Some(u) = User::find_by_id(id).one(&state.db).await? {
            let mut u_active: user::ActiveModel = u.into();
            u_active.banned = Set(banned);
            u_active.updated_at = Set(now);
            u_active.update(&state.db).await?;
        }
    } else if let Some(ids) = payload.user_ids {
        let txn = state.db.begin().await?;
        for id in ids {
            if let Some(u) = User::find_by_id(id).one(&txn).await? {
                let mut u_active: user::ActiveModel = u.into();
                u_active.banned = Set(banned);
                u_active.updated_at = Set(now);
                u_active.update(&txn).await?;
            }
        }
        txn.commit().await?;
    }

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/user/resetSecret
pub async fn reset_secret(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<UserIdRequest>,
) -> Result<Response, AppError> {
    let id = payload
        .id
        .ok_or_else(|| AppError::Custom(422, "id is required".to_string()))?;

    let u = User::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "用户不存在".to_string()))?;

    let mut u_active: user::ActiveModel = u.into();
    u_active.token = Set(random_char(32, false));
    u_active.uuid = Set(generate_uuid());
    u_active.updated_at = Set(chrono::Utc::now().timestamp());
    u_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/user/setInviteUser
pub async fn set_invite_user(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<SetInviteUserRequest>,
) -> Result<Response, AppError> {
    let u = User::find_by_id(payload.id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::Custom(400202, "用户不存在".to_string()))?;

    let inv_id = if let Some(ref em) = payload.email {
        if em.trim().is_empty() {
            None
        } else {
            let inv = User::find()
                .filter(user::Column::Email.eq(em.trim()))
                .one(&state.db)
                .await?
                .ok_or_else(|| AppError::Custom(400202, "推荐人不存在".to_string()))?;
            Some(inv.id)
        }
    } else {
        None
    };

    let mut u_active: user::ActiveModel = u.into();
    u_active.invite_user_id = Set(inv_id);
    u_active.updated_at = Set(chrono::Utc::now().timestamp());
    u_active.update(&state.db).await?;

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/user/destroy
pub async fn destroy(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(payload): Json<UserIdRequest>,
) -> Result<Response, AppError> {
    if let Some(id) = payload.id {
        let u = User::find_by_id(id)
            .one(&state.db)
            .await?
            .ok_or_else(|| AppError::Custom(400202, "用户不存在".to_string()))?;
        let u_active: user::ActiveModel = u.into();
        u_active.delete(&state.db).await?;
    } else if let Some(ids) = payload.user_ids {
        User::delete_many()
            .filter(user::Column::Id.is_in(ids))
            .exec(&state.db)
            .await?;
    }

    Ok(ApiResponse::success(true).into_response())
}

/// POST /api/v2/admin/user/dumpCSV
pub async fn dump_csv(
    State(state): State<AppState>,
    _admin: AuthenticatedAdmin,
    Json(_payload): Json<AdminTableQuery>,
) -> Result<Response, AppError> {
    let users = User::find()
        .order_by_asc(user::Column::Id)
        .limit(1000)
        .all(&state.db)
        .await?;

    let mut csv = String::from("id,email,balance,plan_id,expired_at,created_at\n");
    for u in users {
        csv.push_str(&format!(
            "{},{},{:.2},{},{},{}\n",
            u.id,
            u.email,
            (u.balance as f64) / 100.0,
            u.plan_id.unwrap_or(0),
            u.expired_at.unwrap_or(0),
            u.created_at
        ));
    }

    Ok((
        [
            ("content-type", "text/csv; charset=utf-8"),
            ("content-disposition", "attachment; filename=\"users.csv\""),
        ],
        csv,
    )
        .into_response())
}

/// POST /api/v2/admin/user/sendMail
pub async fn send_mail(_admin: AuthenticatedAdmin) -> Result<Response, AppError> {
    Ok(ApiResponse::success(true).into_response())
}
