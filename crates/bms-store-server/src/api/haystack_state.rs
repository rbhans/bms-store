//! Adapter that implements [`bms_haystack::server::HaystackState`] over the
//! storage stores held in [`crate::api::ApiState`]. Handlers in the
//! `bms-haystack` crate dispatch through this adapter to the actual
//! EntityStore / PointStore / HistoryStore / OverrideStore.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use bms_haystack::filter::{eval, lower_to_sql, FilterExpr, NoResolver, SqlParam};
use bms_haystack::server::{HaystackError, HaystackState, PointWriteRequest};
use bms_haystack::val::{Dict, Grid, Number, Ref, Value};
use bms_store_storage::store::entity_store::{Entity, EntityStore};
use bms_store_storage::store::history_store::{HistoryQuery, HistoryStore};
use bms_store_storage::store::override_store::OverrideStore;
use bms_store_storage::store::point_store::PointStore;

use super::ApiState;

/// Wraps the relevant [`ApiState`] stores into a single trait object.
pub struct StoreAdapter {
    pub entity: EntityStore,
    pub point: PointStore,
    pub history: HistoryStore,
    pub override_: OverrideStore,
    pub server_name: String,
    pub product_version: String,
}

impl StoreAdapter {
    pub fn from_api_state(state: &ApiState) -> Arc<dyn HaystackState> {
        Arc::new(Self {
            entity: state.entity_store.clone(),
            point: state.point_store.clone(),
            history: state.history_store.clone(),
            override_: state.override_store.clone(),
            server_name: state.scenario_name.clone(),
            product_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    async fn entities_to_dicts(&self) -> Vec<Dict> {
        self.entity
            .list_entities(None, None)
            .await
            .into_iter()
            .map(entity_to_dict)
            .collect()
    }
}

#[async_trait]
impl HaystackState for StoreAdapter {
    async fn about(&self) -> Dict {
        let mut d = Dict::default();
        d.insert("serverName", Value::Str(self.server_name.clone()));
        d.insert("vendorName", Value::Str("bms-store".into()));
        d.insert("productName", Value::Str("bms-stored".into()));
        d.insert(
            "productVersion",
            Value::Str(self.product_version.clone()),
        );
        d.insert("haystackVersion", Value::Str("5.0".into()));
        d.insert(
            "phVersion",
            Value::Str(bms_haystack::xeto::version::VENDORED_PH_VERSION.into()),
        );
        d
    }

    async fn read(
        &self,
        filter: &FilterExpr,
        limit: Option<usize>,
    ) -> Result<Grid, HaystackError> {
        // Try SQL push-down first. Falls back to load-then-eval if the
        // filter has constructs the lowerer can't represent (arrow paths,
        // unit comparisons, etc.).
        let mut matched: Vec<Dict> = match lower_to_sql(filter) {
            Ok(frag) => {
                let params = frag.params.into_iter().map(sql_param_to_json).collect();
                let entities = self.entity.find_by_sql_filter(frag.sql, params).await;
                entities.into_iter().map(entity_to_dict).collect()
            }
            Err(_) => {
                let dicts = self.entities_to_dicts().await;
                let resolver = DictResolver::new(&dicts);
                dicts
                    .iter()
                    .filter(|d| eval(filter, d, &resolver))
                    .cloned()
                    .collect()
            }
        };
        if let Some(n) = limit {
            matched.truncate(n);
        }
        Ok(Grid::from_rows(matched))
    }

    async fn read_by_id(&self, id: &Ref) -> Result<Dict, HaystackError> {
        match self.entity.get_entity(&id.id).await {
            Ok(e) => Ok(entity_to_dict(e)),
            Err(_) => Err(HaystackError::NotFound),
        }
    }

    async fn nav(&self, nav_id: Option<&Ref>) -> Result<Grid, HaystackError> {
        let parent = nav_id.map(|r| r.id.as_str());
        let parent_arg = parent.unwrap_or("__root__");
        let entities = self.entity.list_entities(None, Some(parent_arg)).await;
        let rows: Vec<Dict> = entities.into_iter().map(entity_to_dict).collect();
        Ok(Grid::from_rows(rows))
    }

    async fn his_read(&self, id: &Ref, range: &str) -> Result<Grid, HaystackError> {
        // Range parsing: minimal — `today`, `yesterday`, or
        // `<start_iso>,<end_iso>` (Unix-millis fallback).
        let (start_ms, end_ms) = parse_range(range)?;
        let q = HistoryQuery {
            device_id: id_device(id)?,
            point_id: id_point(id)?,
            start_ms,
            end_ms,
            max_results: None,
        };
        let result = self
            .history
            .query(q)
            .await
            .map_err(|e| HaystackError::Backend(e.to_string()))?;
        let mut rows = Vec::with_capacity(result.samples.len());
        for s in result.samples {
            let mut row = Dict::default();
            row.insert(
                "ts",
                Value::DateTime(bms_haystack::val::HDateTime {
                    val: chrono::DateTime::from_timestamp_millis(s.timestamp_ms)
                        .unwrap_or_default(),
                    tz: "UTC".into(),
                }),
            );
            row.insert("val", Value::Number(Number::unitless(s.value)));
            rows.push(row);
        }
        Ok(Grid::from_rows(rows))
    }

    async fn his_write(&self, _id: &Ref, _items: &Grid) -> Result<Dict, HaystackError> {
        Err(HaystackError::NotImplemented)
    }

    async fn point_write(&self, req: &PointWriteRequest) -> Result<Dict, HaystackError> {
        let (device_id, point_id) = split_device_point(&req.id)?;
        let override_value: serde_json::Value = match &req.val {
            Some(Value::Number(n)) => serde_json::json!(n.val),
            Some(Value::Bool(b)) => serde_json::json!(*b),
            Some(Value::Str(s)) => serde_json::json!(s),
            None => serde_json::Value::Null,
            _ => {
                return Err(HaystackError::BadRequest(
                    "point_write only accepts Bool, Number, Str, or null".into(),
                ))
            }
        };
        let who = req.who.clone().unwrap_or_else(|| "haystack-api".into());
        // Convert duration into an absolute expiry timestamp (ms-since-epoch).
        let expires_ms = req.duration_ms.map(|d| {
            chrono::Utc::now().timestamp_millis() + d
        });
        self.override_
            .record(
                &device_id,
                &point_id,
                None,
                override_value,
                Some(req.level),
                expires_ms,
                &who,
            )
            .await
            .map_err(|e| HaystackError::Backend(e.to_string()))?;

        let mut d = Dict::default();
        d.insert("id", Value::Ref(req.id.clone()));
        d.insert("level", Value::Number(Number::unitless(req.level as f64)));
        if let Some(v) = &req.val {
            d.insert("val", v.clone());
        }
        d.marker("ok");
        Ok(d)
    }

    async fn invoke_action(
        &self,
        _id: &Ref,
        _action: &str,
        _args: &Dict,
    ) -> Result<Grid, HaystackError> {
        Err(HaystackError::NotImplemented)
    }

    async fn snapshot_entities(&self) -> HashMap<String, Dict> {
        let mut out = HashMap::new();
        for e in self.entity.list_entities(None, None).await {
            let id = e.id.clone();
            out.insert(id, entity_to_dict(e));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Conversion: Entity ↔ Dict
// ---------------------------------------------------------------------------

fn entity_to_dict(e: Entity) -> Dict {
    let mut d = Dict::default();
    d.insert("id", Value::Ref(Ref::with_dis(e.id.clone(), e.dis.clone())));
    d.insert("dis", Value::Str(e.dis));
    // entity_type marker — preserves discoverability via filter `equip`/`point`/etc.
    d.marker(&e.entity_type);
    for (name, opt_val) in e.tags {
        let v = match opt_val {
            None => Value::Marker,
            Some(s) => parse_tag_value(&s),
        };
        d.tags.insert(name, v);
    }
    for (name, target_id) in e.refs {
        d.tags.insert(name, Value::Ref(Ref::new(target_id)));
    }
    d
}

fn parse_tag_value(s: &str) -> Value {
    if s == "true" {
        return Value::Bool(true);
    }
    if s == "false" {
        return Value::Bool(false);
    }
    if let Ok(n) = s.parse::<f64>() {
        return Value::Number(Number::unitless(n));
    }
    // Number with unit suffix: split the leading numeric portion.
    let trimmed = s.trim();
    let split = trimmed
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(trimmed.len());
    if split > 0 {
        if let Ok(n) = trimmed[..split].parse::<f64>() {
            let unit = trimmed[split..].trim().to_string();
            if !unit.is_empty() {
                return Value::Number(Number::with_unit(n, unit));
            }
        }
    }
    Value::Str(s.to_string())
}

// ---------------------------------------------------------------------------
// Resolver — backs the filter evaluator's arrow-path walks against the
// in-memory dict snapshot.
// ---------------------------------------------------------------------------

struct DictResolver<'a> {
    by_id: HashMap<&'a str, &'a Dict>,
}

impl<'a> DictResolver<'a> {
    fn new(rows: &'a [Dict]) -> Self {
        let mut by_id = HashMap::with_capacity(rows.len());
        for d in rows {
            if let Some(Value::Ref(r)) = d.get("id") {
                by_id.insert(r.id.as_str(), d);
            }
        }
        Self { by_id }
    }
}

impl<'a> bms_haystack::filter::Resolver for DictResolver<'a> {
    fn resolve(&self, id: &str) -> Option<&Dict> {
        self.by_id.get(id).copied()
    }
}

// Silence the NoResolver dead-code warning by referencing it here.
#[allow(dead_code)]
fn _resolver_marker() -> NoResolver {
    NoResolver
}

fn sql_param_to_json(p: SqlParam) -> serde_json::Value {
    match p {
        SqlParam::Text(s) => serde_json::Value::String(s),
        SqlParam::Integer(i) => serde_json::json!(i),
        SqlParam::Real(f) => serde_json::json!(f),
        SqlParam::Bool(b) => serde_json::Value::Bool(b),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a `range` parameter to (start_ms, end_ms).
fn parse_range(s: &str) -> Result<(i64, i64), HaystackError> {
    let now = chrono::Utc::now().timestamp_millis();
    let day = 86_400_000_i64;
    match s.trim() {
        "today" | "" => Ok((now - now % day, now)),
        "yesterday" => {
            let start = now - now % day - day;
            Ok((start, start + day))
        }
        "last24h" => Ok((now - day, now)),
        "lastWeek" => Ok((now - 7 * day, now)),
        "lastMonth" => Ok((now - 30 * day, now)),
        other => {
            // Accept `<start_ms>,<end_ms>` as a fallback.
            let parts: Vec<&str> = other.split(',').collect();
            if parts.len() == 2 {
                let a: i64 = parts[0]
                    .parse()
                    .map_err(|_| HaystackError::BadRequest(format!("bad range: {other}")))?;
                let b: i64 = parts[1]
                    .parse()
                    .map_err(|_| HaystackError::BadRequest(format!("bad range: {other}")))?;
                return Ok((a, b));
            }
            Err(HaystackError::BadRequest(format!("unknown range: {other}")))
        }
    }
}

/// Haystack his refs are typically `<deviceInstance>:<pointId>`.
fn id_device(r: &Ref) -> Result<String, HaystackError> {
    r.id
        .split_once(':')
        .map(|(d, _)| d.to_string())
        .ok_or_else(|| HaystackError::BadRequest("ref must be <device>:<point>".into()))
}

fn id_point(r: &Ref) -> Result<String, HaystackError> {
    r.id
        .split_once(':')
        .map(|(_, p)| p.to_string())
        .ok_or_else(|| HaystackError::BadRequest("ref must be <device>:<point>".into()))
}

fn split_device_point(r: &Ref) -> Result<(String, String), HaystackError> {
    r.id
        .split_once(':')
        .map(|(d, p)| (d.to_string(), p.to_string()))
        .ok_or_else(|| HaystackError::BadRequest("ref must be <device>:<point>".into()))
}
