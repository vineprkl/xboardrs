use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};

use crate::{
    common::AppError,
    entities::{ticket, ticket_message, Ticket, TicketMessage},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketMessageResource {
    pub id: i32,
    pub user_id: i32,
    pub ticket_id: i32,
    pub message: String,
    pub is_me: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDetailResource {
    pub id: i32,
    pub user_id: i32,
    pub subject: String,
    pub level: i32,
    pub status: i32,
    pub reply_status: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub message: Vec<TicketMessageResource>,
}

#[derive(Clone)]
pub struct TicketService {
    db: DatabaseConnection,
}

impl TicketService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Fetches all tickets submitted by a user, ordered by creation time descending.
    pub async fn fetch_user_tickets(&self, user_id: i32) -> Result<Vec<ticket::Model>, AppError> {
        let tickets = Ticket::find()
            .filter(ticket::Column::UserId.eq(user_id))
            .order_by_desc(ticket::Column::CreatedAt)
            .all(&self.db)
            .await?;
        Ok(tickets)
    }

    /// Fetches a single ticket with all its messages and `is_me` flag.
    pub async fn fetch_ticket_detail(
        &self,
        user_id: i32,
        ticket_id: i32,
    ) -> Result<TicketDetailResource, AppError> {
        let ticket_model = Ticket::find_by_id(ticket_id)
            .filter(ticket::Column::UserId.eq(user_id))
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::BadRequest("Ticket does not exist".into()))?;

        let messages = TicketMessage::find()
            .filter(ticket_message::Column::TicketId.eq(ticket_id))
            .order_by_asc(ticket_message::Column::Id)
            .all(&self.db)
            .await?;

        let msg_resources: Vec<TicketMessageResource> = messages
            .into_iter()
            .map(|m| TicketMessageResource {
                is_me: m.user_id == user_id,
                id: m.id,
                user_id: m.user_id,
                ticket_id: m.ticket_id,
                message: m.message,
                created_at: m.created_at,
                updated_at: m.updated_at,
            })
            .collect();

        Ok(TicketDetailResource {
            id: ticket_model.id,
            user_id: ticket_model.user_id,
            subject: ticket_model.subject,
            level: ticket_model.level,
            status: ticket_model.status,
            reply_status: ticket_model.reply_status,
            created_at: ticket_model.created_at,
            updated_at: ticket_model.updated_at,
            message: msg_resources,
        })
    }

    /// Creates a new ticket along with its first message in a transaction.
    pub async fn create_ticket(
        &self,
        user_id: i32,
        subject: &str,
        level: i32,
        message: &str,
    ) -> Result<ticket::Model, AppError> {
        let now = Utc::now().timestamp();
        let txn = self.db.begin().await?;

        let new_ticket = ticket::ActiveModel {
            user_id: Set(user_id),
            subject: Set(subject.to_string()),
            level: Set(level),
            status: Set(0),       // 0: open
            reply_status: Set(0), // 0: pending reply
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        let inserted_ticket = new_ticket.insert(&txn).await?;

        let new_msg = ticket_message::ActiveModel {
            user_id: Set(user_id),
            ticket_id: Set(inserted_ticket.id),
            message: Set(message.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        new_msg.insert(&txn).await?;

        txn.commit().await?;
        Ok(inserted_ticket)
    }

    /// Appends a reply message to an existing ticket.
    pub async fn reply_ticket(
        &self,
        user_id: i32,
        ticket_id: i32,
        message: &str,
        must_wait_reply: bool,
    ) -> Result<bool, AppError> {
        let ticket_model = Ticket::find_by_id(ticket_id)
            .filter(ticket::Column::UserId.eq(user_id))
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::BadRequest("Ticket does not exist".into()))?;

        if ticket_model.status != 0 {
            return Err(AppError::BadRequest(
                "The ticket is closed and cannot be replied".into(),
            ));
        }

        if must_wait_reply {
            let last_msg = TicketMessage::find()
                .filter(ticket_message::Column::TicketId.eq(ticket_id))
                .order_by_desc(ticket_message::Column::Id)
                .one(&self.db)
                .await?;

            if let Some(msg) = last_msg {
                if msg.user_id == user_id {
                    return Err(AppError::BadRequest(
                        "Please wait for the technical engineer to reply".into(),
                    ));
                }
            }
        }

        let now = Utc::now().timestamp();
        let txn = self.db.begin().await?;

        let new_msg = ticket_message::ActiveModel {
            user_id: Set(user_id),
            ticket_id: Set(ticket_id),
            message: Set(message.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        new_msg.insert(&txn).await?;

        let mut active_ticket: ticket::ActiveModel = ticket_model.into();
        active_ticket.reply_status = Set(0);
        active_ticket.updated_at = Set(now);
        active_ticket.update(&txn).await?;

        txn.commit().await?;
        Ok(true)
    }

    /// Closes an open ticket.
    pub async fn close_ticket(&self, user_id: i32, ticket_id: i32) -> Result<bool, AppError> {
        let ticket_model = Ticket::find_by_id(ticket_id)
            .filter(ticket::Column::UserId.eq(user_id))
            .one(&self.db)
            .await?
            .ok_or_else(|| AppError::BadRequest("Ticket does not exist".into()))?;

        let mut active_ticket: ticket::ActiveModel = ticket_model.into();
        active_ticket.status = Set(1); // 1: closed
        active_ticket.updated_at = Set(Utc::now().timestamp());
        active_ticket.update(&self.db).await?;
        Ok(true)
    }
}
