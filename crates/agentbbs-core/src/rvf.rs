//! RVF — a minimal, clean-room implementation of a RuVector-style vector
//! memory format for AgentBBS agent memory and semantic board search.
//!
//! This is *not* a port of the `ruvnet/ruvector` crate; it is an
//! independent, self-contained store that reads and writes a documented
//! `.rvf` binary layout and supports cosine nearest-neighbour search. It
//! interoperates at the concept level (vectors + metadata + cosine search)
//! and is intended to be swappable for the full RuVector engine via the
//! `agentbbs-federation` AgentDB adapter when that engine is present.
//!
//! ## File layout (`agentbbs.rvf.v1`)
//!
//! ```text
//! magic:   8 bytes  = b"AGBBSRVF"
//! version: u16 LE   = 1
//! dim:     u32 LE   = vector dimensionality
//! count:   u32 LE   = number of records
//! records: count × {
//!     id_len:  u16 LE
//!     id:      id_len bytes (utf-8)
//!     meta_len:u32 LE
//!     meta:    meta_len bytes (utf-8 json)
//!     vector:  dim × f32 LE
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const MAGIC: &[u8; 8] = b"AGBBSRVF";
const VERSION: u16 = 1;

/// A single vector record: an id, an embedding, and arbitrary JSON metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Caller-chosen id (often a [`crate::board::MessageId`] or memory key).
    pub id: String,
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// Free-form metadata (kept small).
    pub meta: serde_json::Value,
}

/// A scored search hit.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// The matched record's id.
    pub id: String,
    /// Cosine similarity in `[-1, 1]` (higher is closer).
    pub score: f32,
    /// The matched record's metadata.
    pub meta: serde_json::Value,
}

/// An in-memory vector store with a documented on-disk `.rvf` format.
#[derive(Clone, Debug, Default)]
pub struct RvfStore {
    dim: usize,
    records: Vec<Record>,
}

impl RvfStore {
    /// Create an empty store for vectors of dimensionality `dim`.
    pub fn new(dim: usize) -> Self {
        RvfStore {
            dim,
            records: Vec::new(),
        }
    }

    /// Vector dimensionality.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Number of stored records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Insert or replace a record. Errors if the vector's length disagrees
    /// with the store dimensionality.
    pub fn upsert(&mut self, rec: Record) -> Result<()> {
        if rec.vector.len() != self.dim {
            return Err(Error::malformed(
                "vector",
                format!("expected dim {}, got {}", self.dim, rec.vector.len()),
            ));
        }
        if let Some(existing) = self.records.iter_mut().find(|r| r.id == rec.id) {
            *existing = rec;
        } else {
            self.records.push(rec);
        }
        Ok(())
    }

    /// Cosine `top_k` nearest neighbours to `query`.
    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<Hit>> {
        if query.len() != self.dim {
            return Err(Error::malformed(
                "query",
                format!("expected dim {}, got {}", self.dim, query.len()),
            ));
        }
        let qn = norm(query);
        let mut hits: Vec<Hit> = self
            .records
            .iter()
            .map(|r| Hit {
                id: r.id.clone(),
                score: cosine(query, qn, &r.vector),
                meta: r.meta.clone(),
            })
            .collect();
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k);
        Ok(hits)
    }

    /// Serialize to the `.rvf` byte layout.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(self.dim as u32).to_le_bytes());
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            let id = r.id.as_bytes();
            out.extend_from_slice(&(id.len() as u16).to_le_bytes());
            out.extend_from_slice(id);
            let meta = serde_json::to_vec(&r.meta).unwrap_or_else(|_| b"null".to_vec());
            out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
            out.extend_from_slice(&meta);
            for f in &r.vector {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        out
    }

    /// Parse a store from the `.rvf` byte layout.
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(buf);
        let magic = c.take(8)?;
        if magic != MAGIC {
            return Err(Error::malformed("rvf", "bad magic"));
        }
        let version = u16::from_le_bytes(c.take(2)?.try_into().unwrap());
        if version != VERSION {
            return Err(Error::malformed("rvf", format!("unsupported version {version}")));
        }
        let dim = u32::from_le_bytes(c.take(4)?.try_into().unwrap()) as usize;
        let count = u32::from_le_bytes(c.take(4)?.try_into().unwrap()) as usize;
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            let id_len = u16::from_le_bytes(c.take(2)?.try_into().unwrap()) as usize;
            let id = String::from_utf8(c.take(id_len)?.to_vec())
                .map_err(|e| Error::malformed("rvf id", e))?;
            let meta_len = u32::from_le_bytes(c.take(4)?.try_into().unwrap()) as usize;
            let meta: serde_json::Value = serde_json::from_slice(c.take(meta_len)?)?;
            let mut vector = Vec::with_capacity(dim);
            for _ in 0..dim {
                vector.push(f32::from_le_bytes(c.take(4)?.try_into().unwrap()));
            }
            records.push(Record { id, vector, meta });
        }
        Ok(RvfStore { dim, records })
    }
}

/// A tiny bounds-checked byte cursor.
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Cursor { buf, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| Error::malformed("rvf", "length overflow"))?;
        if end > self.buf.len() {
            return Err(Error::malformed("rvf", "unexpected end of buffer"));
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn cosine(a: &[f32], a_norm: f32, b: &[f32]) -> f32 {
    let bn = norm(b);
    if a_norm == 0.0 || bn == 0.0 {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot / (a_norm * bn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, v: Vec<f32>) -> Record {
        Record {
            id: id.into(),
            vector: v,
            meta: serde_json::json!({ "k": id }),
        }
    }

    #[test]
    fn search_ranks_by_cosine() {
        let mut s = RvfStore::new(3);
        s.upsert(rec("a", vec![1.0, 0.0, 0.0])).unwrap();
        s.upsert(rec("b", vec![0.0, 1.0, 0.0])).unwrap();
        s.upsert(rec("c", vec![0.9, 0.1, 0.0])).unwrap();
        let hits = s.search(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(hits[0].id, "a");
        assert_eq!(hits[1].id, "c");
    }

    #[test]
    fn upsert_replaces() {
        let mut s = RvfStore::new(2);
        s.upsert(rec("x", vec![1.0, 0.0])).unwrap();
        s.upsert(rec("x", vec![0.0, 1.0])).unwrap();
        assert_eq!(s.len(), 1);
        let hits = s.search(&[0.0, 1.0], 1).unwrap();
        assert!(hits[0].score > 0.99);
    }

    #[test]
    fn dim_mismatch_errors() {
        let mut s = RvfStore::new(3);
        assert!(s.upsert(rec("x", vec![1.0, 0.0])).is_err());
        assert!(s.search(&[1.0, 0.0], 1).is_err());
    }

    #[test]
    fn bytes_roundtrip() {
        let mut s = RvfStore::new(4);
        s.upsert(rec("a", vec![1.0, 2.0, 3.0, 4.0])).unwrap();
        s.upsert(rec("b", vec![-1.0, 0.5, 0.0, 2.0])).unwrap();
        let bytes = s.to_bytes();
        let back = RvfStore::from_bytes(&bytes).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back.dim(), 4);
        let hits = back.search(&[1.0, 2.0, 3.0, 4.0], 1).unwrap();
        assert_eq!(hits[0].id, "a");
    }

    #[test]
    fn truncated_buffer_errors() {
        let mut s = RvfStore::new(4);
        s.upsert(rec("a", vec![1.0, 2.0, 3.0, 4.0])).unwrap();
        let bytes = s.to_bytes();
        assert!(RvfStore::from_bytes(&bytes[..bytes.len() - 3]).is_err());
    }
}
