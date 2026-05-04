use anyhow::Result;
use arrow_array::{
    builder::{FixedSizeListBuilder, Float32Builder, StringBuilder},
    cast::AsArray,
    RecordBatch, RecordBatchIterator,
};
use arrow_schema::{DataType, Field, Schema};
use chrono::Utc;
use futures::{pin_mut, TryStreamExt};
use lancedb::{
    query::{ExecutableQuery, QueryBase},
    Connection, Table,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::embedding::EMBED_DIM;

const TABLE_NAME: &str = "episodic";

/// A conversation turn before processing — the raw input to the store.
#[derive(Debug, Clone)]
pub struct RawTurn {
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub importance_score: f32,
}

/// A processed turn as stored in LanceDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicEntry {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub importance_score: f32,
    pub vector: Vec<f32>,
}

pub struct EpisodicStore {
    table: Table,
}

impl EpisodicStore {
    pub async fn new(conn: Arc<Connection>) -> Result<Self> {
        let table = match conn.open_table(TABLE_NAME).execute().await {
            Ok(t) => t,
            Err(_) => {
                let schema = Arc::new(Self::schema());
                let empty: Vec<Result<RecordBatch, arrow_schema::ArrowError>> = vec![];
                conn.create_table(TABLE_NAME, RecordBatchIterator::new(empty, schema))
                    .execute()
                    .await?
            }
        };
        Ok(Self { table })
    }

    /// Embed and store a conversation turn.
    pub async fn store(&self, turn: RawTurn, vector: Vec<f32>) -> Result<()> {
        let entry = EpisodicEntry {
            id: Uuid::new_v4().to_string(),
            session_id: turn.session_id,
            role: turn.role,
            content: turn.content,
            timestamp: Utc::now().to_rfc3339(),
            importance_score: turn.importance_score,
            vector,
        };

        let batch = Self::entry_to_batch(&entry)?;
        let schema = Arc::new(Self::schema());
        self.table
            .add(RecordBatchIterator::new(vec![Ok(batch)], schema))
            .execute()
            .await?;
        Ok(())
    }

    /// Retrieve top-K entries by vector similarity.
    pub async fn retrieve_similar(
        &self,
        vector: Vec<f32>,
        limit: usize,
    ) -> Result<Vec<EpisodicEntry>> {
        let stream = self
            .table
            .query()
            .nearest_to(vector)?
            .limit(limit)
            .execute()
            .await?;

        pin_mut!(stream);
        let mut entries = Vec::new();
        while let Some(batch) = stream.try_next().await? {
            entries.extend(Self::batch_to_entries(&batch)?);
        }
        Ok(entries)
    }

    /// Total rows in the episodic table.
    pub async fn count(&self) -> Result<u64> {
        Ok(self.table.count_rows(None).await? as u64)
    }

    /// Return the N most recently stored entries (for distillation).
    pub async fn retrieve_recent(&self, limit: usize) -> Result<Vec<EpisodicEntry>> {
        use lancedb::query::ExecutableQuery;
        let mut stream = self.table.query().limit(limit).execute().await?;
        let mut entries = Vec::new();
        while let Some(batch) = stream.try_next().await? {
            entries.extend(Self::batch_to_entries(&batch)?);
        }
        Ok(entries)
    }

    fn schema() -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, false),
            Field::new("role", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("timestamp", DataType::Utf8, false),
            Field::new("importance_score", DataType::Float32, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    EMBED_DIM as i32,
                ),
                false,
            ),
        ])
    }

    fn entry_to_batch(entry: &EpisodicEntry) -> Result<RecordBatch> {
        let schema = Arc::new(Self::schema());

        let mut id_b = StringBuilder::new();
        let mut session_b = StringBuilder::new();
        let mut role_b = StringBuilder::new();
        let mut content_b = StringBuilder::new();
        let mut ts_b = StringBuilder::new();
        let mut score_b = Float32Builder::new();
        let mut vec_b =
            FixedSizeListBuilder::new(Float32Builder::new(), EMBED_DIM as i32);

        id_b.append_value(&entry.id);
        session_b.append_value(&entry.session_id);
        role_b.append_value(&entry.role);
        content_b.append_value(&entry.content);
        ts_b.append_value(&entry.timestamp);
        score_b.append_value(entry.importance_score);
        for &v in &entry.vector {
            vec_b.values().append_value(v);
        }
        vec_b.append(true);

        Ok(RecordBatch::try_new(
            schema,
            vec![
                Arc::new(id_b.finish()),
                Arc::new(session_b.finish()),
                Arc::new(role_b.finish()),
                Arc::new(content_b.finish()),
                Arc::new(ts_b.finish()),
                Arc::new(score_b.finish()),
                Arc::new(vec_b.finish()),
            ],
        )?)
    }

    fn batch_to_entries(batch: &RecordBatch) -> Result<Vec<EpisodicEntry>> {
        let ids = batch.column_by_name("id").and_then(|c| Some(c.as_string::<i32>()));
        let sessions = batch.column_by_name("session_id").and_then(|c| Some(c.as_string::<i32>()));
        let roles = batch.column_by_name("role").and_then(|c| Some(c.as_string::<i32>()));
        let contents = batch.column_by_name("content").and_then(|c| Some(c.as_string::<i32>()));
        let timestamps = batch.column_by_name("timestamp").and_then(|c| Some(c.as_string::<i32>()));
        let scores = batch
            .column_by_name("importance_score")
            .and_then(|c| Some(c.as_primitive::<arrow_array::types::Float32Type>()));

        let (Some(ids), Some(sessions), Some(roles), Some(contents), Some(timestamps), Some(scores)) =
            (ids, sessions, roles, contents, timestamps, scores)
        else {
            return Ok(vec![]);
        };

        let mut entries = Vec::with_capacity(batch.num_rows());
        for i in 0..batch.num_rows() {
            entries.push(EpisodicEntry {
                id: ids.value(i).to_string(),
                session_id: sessions.value(i).to_string(),
                role: roles.value(i).to_string(),
                content: contents.value(i).to_string(),
                timestamp: timestamps.value(i).to_string(),
                importance_score: scores.value(i),
                // vector not deserialised — not needed for context assembly
                vector: vec![],
            });
        }
        Ok(entries)
    }
}
