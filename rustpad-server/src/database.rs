//! Backend SQLite database handlers for persisting documents.

use std::str::FromStr;

use anyhow::{bail, Result};
use serde_json::Value;
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, SqlitePool};

/// Represents a document persisted in database storage.
#[derive(sqlx::FromRow, PartialEq, Eq, Clone, Debug)]
pub struct PersistedDocument {
    /// Text content of the document.
    pub text: String,
    /// Language of the document for editor syntax highlighting.
    pub language: Option<String>,
    /// Unix timestamp in milliseconds when the room was created.
    pub created_at: u64,
    /// Unix timestamp in milliseconds when the room was stopped.
    pub closed_at: Option<u64>,
    /// Opaque token that grants host privileges.
    pub host_token: Option<String>,
    /// JSON array containing replay timeline events.
    pub replay_events: Value,
}

/// A driver for database operations wrapping a pool connection.
#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Construct a new database from Postgres connection URI.
    pub async fn new(uri: &str) -> Result<Self> {
        {
            // Create database file if missing, and run migrations.
            let mut conn = SqliteConnectOptions::from_str(uri)?
                .create_if_missing(true)
                .connect()
                .await?;
            sqlx::migrate!().run(&mut conn).await?;
        }
        Ok(Database {
            pool: SqlitePool::connect(uri).await?,
        })
    }

    /// Load the text of a document from the database.
    pub async fn load(&self, document_id: &str) -> Result<PersistedDocument> {
        let row: (
            String,
            Option<String>,
            i64,
            Option<i64>,
            Option<String>,
            String,
        ) = sqlx::query_as(
            r#"
SELECT
    text, language, created_at, closed_at, host_token, replay_events
FROM
    document
WHERE
    id = $1"#,
        )
        .bind(document_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(PersistedDocument {
            text: row.0,
            language: row.1,
            created_at: row.2 as u64,
            closed_at: row.3.map(|value| value as u64),
            host_token: row.4,
            replay_events: serde_json::from_str(&row.5)?,
        })
    }

    /// Store the text of a document in the database.
    pub async fn store(&self, document_id: &str, document: &PersistedDocument) -> Result<()> {
        let result = sqlx::query(
            r#"
INSERT INTO
    document (id, text, language, created_at, closed_at, host_token, replay_events)
VALUES
    ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT(id) DO UPDATE SET
    text = excluded.text,
    language = excluded.language,
    created_at = excluded.created_at,
    closed_at = excluded.closed_at,
    host_token = excluded.host_token,
    replay_events = excluded.replay_events"#,
        )
        .bind(document_id)
        .bind(&document.text)
        .bind(&document.language)
        .bind(document.created_at as i64)
        .bind(document.closed_at.map(|value| value as i64))
        .bind(&document.host_token)
        .bind(document.replay_events.to_string())
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            bail!(
                "expected store() to receive 1 row affected, but it affected {} rows instead",
                result.rows_affected(),
            );
        }
        Ok(())
    }

    /// Count the number of documents in the database.
    pub async fn count(&self) -> Result<usize> {
        let row: (i64,) = sqlx::query_as("SELECT count(*) FROM document")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0 as usize)
    }
}
