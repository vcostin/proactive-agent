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

const TABLE_NAME: &str = "semantic";

/// A distilled long-term fact about the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticFact {
    pub id: String,
    pub fact: String,
    /// Loose category: "preference", "project", "person", "habit", etc.
    pub category: String,
    pub timestamp: String,
    pub vector: Vec<f32>,
}

pub struct SemanticStore {
    table: Table,
}

impl SemanticStore {
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

    pub async fn store(&self, fact: &str, category: &str, vector: Vec<f32>) -> Result<()> {
        let entry = SemanticFact {
            id: Uuid::new_v4().to_string(),
            fact: fact.to_string(),
            category: category.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            vector,
        };

        let batch = Self::fact_to_batch(&entry)?;
        let schema = Arc::new(Self::schema());
        self.table
            .add(RecordBatchIterator::new(vec![Ok(batch)], schema))
            .execute()
            .await?;
        Ok(())
    }

    pub async fn retrieve_relevant(
        &self,
        vector: Vec<f32>,
        limit: usize,
    ) -> Result<Vec<SemanticFact>> {
        let stream = self
            .table
            .query()
            .nearest_to(vector)?
            .limit(limit)
            .execute()
            .await?;

        pin_mut!(stream);
        let mut facts = Vec::new();
        while let Some(batch) = stream.try_next().await? {
            facts.extend(Self::batch_to_facts(&batch)?);
        }
        Ok(facts)
    }

    /// Total rows in the semantic table.
    pub async fn count(&self) -> Result<u64> {
        Ok(self.table.count_rows(None).await? as u64)
    }

    /// Background distillation: extract long-term facts from recent episodic entries.
    /// Called from a tokio task — never blocks a conversation turn.
    /// EXTEND Phase 3: implement LLM-assisted fact extraction
    pub async fn distill(&self) -> Result<()> {
        Ok(())
    }

    fn schema() -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("fact", DataType::Utf8, false),
            Field::new("category", DataType::Utf8, false),
            Field::new("timestamp", DataType::Utf8, false),
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

    fn fact_to_batch(fact: &SemanticFact) -> Result<RecordBatch> {
        let schema = Arc::new(Self::schema());

        let mut id_b = StringBuilder::new();
        let mut fact_b = StringBuilder::new();
        let mut cat_b = StringBuilder::new();
        let mut ts_b = StringBuilder::new();
        let mut vec_b =
            FixedSizeListBuilder::new(Float32Builder::new(), EMBED_DIM as i32);

        id_b.append_value(&fact.id);
        fact_b.append_value(&fact.fact);
        cat_b.append_value(&fact.category);
        ts_b.append_value(&fact.timestamp);
        for &v in &fact.vector {
            vec_b.values().append_value(v);
        }
        vec_b.append(true);

        Ok(RecordBatch::try_new(
            schema,
            vec![
                Arc::new(id_b.finish()),
                Arc::new(fact_b.finish()),
                Arc::new(cat_b.finish()),
                Arc::new(ts_b.finish()),
                Arc::new(vec_b.finish()),
            ],
        )?)
    }

    fn batch_to_facts(batch: &RecordBatch) -> Result<Vec<SemanticFact>> {
        let ids = batch.column_by_name("id").and_then(|c| Some(c.as_string::<i32>()));
        let facts = batch.column_by_name("fact").and_then(|c| Some(c.as_string::<i32>()));
        let cats = batch.column_by_name("category").and_then(|c| Some(c.as_string::<i32>()));
        let timestamps = batch.column_by_name("timestamp").and_then(|c| Some(c.as_string::<i32>()));

        let (Some(ids), Some(facts), Some(cats), Some(timestamps)) =
            (ids, facts, cats, timestamps)
        else {
            return Ok(vec![]);
        };

        let mut result = Vec::with_capacity(batch.num_rows());
        for i in 0..batch.num_rows() {
            result.push(SemanticFact {
                id: ids.value(i).to_string(),
                fact: facts.value(i).to_string(),
                category: cats.value(i).to_string(),
                timestamp: timestamps.value(i).to_string(),
                vector: vec![],
            });
        }
        Ok(result)
    }
}
