//! Integration smoke test for the HTTP facade. Builds the router with a
//! minimal in-memory `HaystackState` and exercises representative endpoints
//! end-to-end (GET about, GET defs, GET read with filter, POST watchSub →
//! watchPoll → watchUnsub round-trip).

#![cfg(feature = "server")]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bms_haystack::filter::{eval, FilterExpr, NoResolver};
use bms_haystack::server::{router, HaystackError, HaystackState, PointWriteRequest};
use bms_haystack::val::{Dict, Grid, Number, Ref, Value};
use http_body_util::BodyExt;
use tower::ServiceExt;

struct MockState {
    entities: Vec<Dict>,
}

impl MockState {
    fn new() -> Self {
        let mut ahu = Dict::default();
        ahu.insert("id", Value::Ref(Ref::with_dis("ahu-1", "AHU 1")));
        ahu.insert("dis", Value::Str("AHU 1".into()));
        ahu.marker("equip");
        ahu.marker("ahu");
        ahu.marker("air");

        let mut sensor = Dict::default();
        sensor.insert("id", Value::Ref(Ref::with_dis("p-temp", "Discharge Air Temp")));
        sensor.insert("dis", Value::Str("Discharge Air Temp".into()));
        sensor.marker("point");
        sensor.marker("sensor");
        sensor.marker("temp");
        sensor.marker("air");
        sensor.marker("discharge");
        sensor.insert("equipRef", Value::Ref(Ref::new("ahu-1")));
        sensor.insert("temp", Value::Number(Number::with_unit(72.0, "°F")));

        Self {
            entities: vec![ahu, sensor],
        }
    }
}

#[async_trait]
impl HaystackState for MockState {
    async fn about(&self) -> Dict {
        let mut d = Dict::default();
        d.insert("serverName", Value::Str("test-server".into()));
        d.insert("productName", Value::Str("bms-haystack-test".into()));
        d
    }

    async fn read(
        &self,
        filter: &FilterExpr,
        limit: Option<usize>,
    ) -> Result<Grid, HaystackError> {
        let mut matched: Vec<Dict> = self
            .entities
            .iter()
            .filter(|d| eval(filter, d, &NoResolver))
            .cloned()
            .collect();
        if let Some(n) = limit {
            matched.truncate(n);
        }
        Ok(Grid::from_rows(matched))
    }

    async fn read_by_id(&self, id: &Ref) -> Result<Dict, HaystackError> {
        self.entities
            .iter()
            .find(|d| matches!(d.get("id"), Some(Value::Ref(r)) if r.id == id.id))
            .cloned()
            .ok_or(HaystackError::NotFound)
    }

    async fn nav(&self, _nav_id: Option<&Ref>) -> Result<Grid, HaystackError> {
        Ok(Grid::from_rows(self.entities.clone()))
    }

    async fn his_read(&self, _id: &Ref, _range: &str) -> Result<Grid, HaystackError> {
        Ok(Grid::default())
    }

    async fn his_write(&self, _id: &Ref, _items: &Grid) -> Result<Dict, HaystackError> {
        Ok(Dict::default())
    }

    async fn point_write(&self, _req: &PointWriteRequest) -> Result<Dict, HaystackError> {
        Ok(Dict::default())
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
        for d in &self.entities {
            if let Some(Value::Ref(r)) = d.get("id") {
                out.insert(r.id.clone(), d.clone());
            }
        }
        out
    }
}

fn app() -> axum::Router {
    router(Arc::new(MockState::new()))
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn about_returns_server_identity() {
    let resp = app()
        .oneshot(Request::get("/about").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["_kind"], "grid");
    let row = &json["rows"][0];
    assert_eq!(row["serverName"], "test-server");
}

#[tokio::test]
async fn defs_lists_globals() {
    let resp = app()
        .oneshot(Request::get("/defs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["_kind"], "grid");
    let rows = json["rows"].as_array().unwrap();
    assert!(rows.len() > 100, "expected >100 def rows, got {}", rows.len());
}

#[tokio::test]
async fn read_with_filter_returns_matching_entities() {
    let resp = app()
        .oneshot(
            Request::get("/read?filter=ahu")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let rows = json["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["dis"], "AHU 1");
}

#[tokio::test]
async fn read_with_arrow_path_filter() {
    // discharge-air-temp sensor has equipRef=ahu-1; ahu-1 has `air` marker
    let resp = app()
        .oneshot(
            Request::get("/read?filter=point%20and%20discharge")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let rows = json["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["dis"], "Discharge Air Temp");
}

#[tokio::test]
async fn ops_lists_supported_operations() {
    let resp = app()
        .oneshot(Request::get("/ops").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let rows = json["rows"].as_array().unwrap();
    let names: Vec<&str> = rows
        .iter()
        .filter_map(|r| r["name"].as_str())
        .collect();
    for op in ["about", "defs", "read", "watchSub", "pointWrite", "hisRead"] {
        assert!(names.contains(&op), "missing op: {op}");
    }
}

#[tokio::test]
async fn watch_sub_unsub_round_trip() {
    let app = app();
    // Subscribe
    let sub_body = serde_json::json!({
        "_kind": "grid",
        "meta": {},
        "cols": [{"name": "id"}],
        "rows": [{"id": {"_kind": "ref", "val": "ahu-1"}}]
    });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/watchSub")
                .header("content-type", "application/json")
                .body(Body::from(sub_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    let watch_id = json["rows"][0]["watchId"].as_str().unwrap().to_string();
    assert!(!watch_id.is_empty());

    // Unsub
    let unsub_body = serde_json::json!({
        "_kind": "grid",
        "meta": {"watchId": watch_id.clone()},
        "cols": [],
        "rows": []
    });
    let resp = app
        .oneshot(
            Request::post("/watchUnsub")
                .header("content-type", "application/json")
                .body(Body::from(unsub_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
