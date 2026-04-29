use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use crate::event::bus::{Event, EventBus};
use crate::fdd::model::*;
use crate::fdd::resolver::PointResolver;
use crate::store::fdd_store::FddStore;
use crate::store::node_store::NodeStore;
use crate::store::point_store::{PointKey, PointStore};

// ---------------------------------------------------------------------------
// Per-binding runtime state
// ---------------------------------------------------------------------------

/// Per-binding runtime state for tracking fault conditions.
struct BindingRuntime {
    confirmation_counter: u16,
    condition_since: Option<Instant>,
    /// node_id -> (value, timestamp) for stuck-value detection.
    last_values: HashMap<String, (f64, Instant)>,
    /// Timestamps of state transitions for short-cycling (CountInWindow).
    transition_timestamps: VecDeque<Instant>,
}

impl BindingRuntime {
    fn new() -> Self {
        Self {
            confirmation_counter: 0,
            condition_since: None,
            last_values: HashMap::new(),
            transition_timestamps: VecDeque::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// FddEngine
// ---------------------------------------------------------------------------

/// Fault Detection and Diagnostics evaluation engine.
///
/// Subscribes to the [`EventBus`] and evaluates FDD rules against equipment in
/// real-time. Time-dependent conditions (stuck value, short-cycling, schedule
/// deviation) are also re-evaluated on a periodic 30-second tick.
pub struct FddEngine {
    fdd_store: FddStore,
    node_store: NodeStore,
    point_store: PointStore,
    event_bus: EventBus,
}

impl FddEngine {
    pub fn new(
        fdd_store: FddStore,
        node_store: NodeStore,
        point_store: PointStore,
        event_bus: EventBus,
    ) -> Self {
        Self {
            fdd_store,
            node_store,
            point_store,
            event_bus,
        }
    }

    /// Spawn the engine as a background tokio task.
    pub fn start(self, shutdown: CancellationToken) {
        tokio::spawn(async move {
            self.run(shutdown).await;
        });
    }

    // -----------------------------------------------------------------------
    // Main event loop
    // -----------------------------------------------------------------------

    async fn run(self, shutdown: CancellationToken) {
        let resolver = PointResolver::new(self.node_store.clone());
        let mut event_rx = self.event_bus.subscribe();
        let mut store_version = self.fdd_store.subscribe();
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Live state
        let mut rules: HashMap<i64, FddRule> = HashMap::new();
        let mut bindings: Vec<FddBinding> = Vec::new();
        let mut runtime: HashMap<i64, BindingRuntime> = HashMap::new();
        // Reverse index: node_id -> parent equip_id
        let mut point_to_equip: HashMap<String, String> = HashMap::new();

        Self::reload_config(
            &self.fdd_store,
            &self.node_store,
            &mut rules,
            &mut bindings,
            &mut runtime,
            &mut point_to_equip,
        )
        .await;

        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    let event = match event {
                        Ok(e) => e,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "FDD engine lagged on event bus");
                            continue;
                        }
                        Err(_) => break,
                    };
                    if let Event::ValueChanged { node_id, .. } = event.as_ref() {
                        if let Some(equip_id) = point_to_equip.get(node_id) {
                            let equip_id = equip_id.clone();
                            let affected: Vec<i64> = bindings
                                .iter()
                                .filter(|b| b.equip_id == equip_id && b.enabled)
                                .map(|b| b.id)
                                .collect();

                            for binding_id in affected {
                                let binding = match bindings.iter().find(|b| b.id == binding_id) {
                                    Some(b) => b,
                                    None => continue,
                                };
                                if let Some(rule) = rules.get(&binding.rule_id) {
                                    if !rule.enabled {
                                        continue;
                                    }
                                    Self::evaluate_binding(
                                        &self.fdd_store,
                                        &self.point_store,
                                        &self.event_bus,
                                        &resolver,
                                        rule,
                                        binding,
                                        &mut runtime,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                }
                _ = tick.tick() => {
                    // Periodic evaluation for time-based conditions
                    for binding in &bindings {
                        if !binding.enabled {
                            continue;
                        }
                        if let Some(rule) = rules.get(&binding.rule_id) {
                            if !rule.enabled {
                                continue;
                            }
                            let is_time_based = matches!(
                                &rule.condition,
                                FddCondition::StuckValue { .. }
                                    | FddCondition::CountInWindow { .. }
                                    | FddCondition::ScheduleDeviation { .. }
                            );
                            if is_time_based {
                                Self::evaluate_binding(
                                    &self.fdd_store,
                                    &self.point_store,
                                    &self.event_bus,
                                    &resolver,
                                    rule,
                                    binding,
                                    &mut runtime,
                                )
                                .await;
                            }
                        }
                    }
                }
                _ = store_version.changed() => {
                    Self::reload_config(
                        &self.fdd_store,
                        &self.node_store,
                        &mut rules,
                        &mut bindings,
                        &mut runtime,
                        &mut point_to_equip,
                    )
                    .await;
                    resolver.invalidate().await;
                }
                _ = shutdown.cancelled() => {
                    tracing::debug!("FDD engine shutting down");
                    break;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Config reload
    // -----------------------------------------------------------------------

    async fn reload_config(
        fdd_store: &FddStore,
        node_store: &NodeStore,
        rules: &mut HashMap<i64, FddRule>,
        bindings: &mut Vec<FddBinding>,
        runtime: &mut HashMap<i64, BindingRuntime>,
        point_to_equip: &mut HashMap<String, String>,
    ) {
        // Reload rules
        let all_rules = fdd_store.list_rules().await;
        rules.clear();
        for r in all_rules {
            rules.insert(r.id, r);
        }

        // Reload bindings
        let new_bindings = fdd_store.list_enabled_bindings().await;

        // Preserve runtime state for existing bindings, create new for new ones
        let existing_ids: HashSet<i64> = runtime.keys().cloned().collect();
        let new_ids: HashSet<i64> = new_bindings.iter().map(|b| b.id).collect();

        // Remove stale runtime entries
        runtime.retain(|id, _| new_ids.contains(id));
        // Add new entries
        for id in &new_ids {
            if !existing_ids.contains(id) {
                runtime.insert(*id, BindingRuntime::new());
            }
        }

        *bindings = new_bindings;

        // Rebuild point-to-equip reverse index
        point_to_equip.clear();
        let equip_ids: HashSet<&str> = bindings.iter().map(|b| b.equip_id.as_str()).collect();
        for equip_id in equip_ids {
            let children = node_store.list_nodes(Some("point"), Some(equip_id)).await;
            for child in &children {
                point_to_equip.insert(child.id.clone(), equip_id.to_string());
            }
            let vchildren = node_store
                .list_nodes(Some("virtual_point"), Some(equip_id))
                .await;
            for child in &vchildren {
                point_to_equip.insert(child.id.clone(), equip_id.to_string());
            }
        }

        tracing::debug!(
            rules = rules.len(),
            bindings = bindings.len(),
            points = point_to_equip.len(),
            "FDD config reloaded"
        );
    }

    // -----------------------------------------------------------------------
    // Binding evaluation
    // -----------------------------------------------------------------------

    async fn evaluate_binding(
        fdd_store: &FddStore,
        point_store: &PointStore,
        event_bus: &EventBus,
        resolver: &PointResolver,
        rule: &FddRule,
        binding: &FddBinding,
        runtime: &mut HashMap<i64, BindingRuntime>,
    ) {
        let rt = runtime
            .entry(binding.id)
            .or_insert_with(BindingRuntime::new);

        // Parse per-binding parameter overrides
        let params: FddParams = binding
            .config_overrides
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let condition_met = Self::evaluate_condition(
            point_store,
            resolver,
            &rule.condition,
            &binding.equip_id,
            &params,
            rt,
        )
        .await;

        if condition_met {
            rt.confirmation_counter = rt.confirmation_counter.saturating_add(1);
            if rt.condition_since.is_none() {
                rt.condition_since = Some(Instant::now());
            }
        } else {
            rt.confirmation_counter = 0;
            rt.condition_since = None;
        }

        // Check if we should raise or clear a fault
        let existing_fault = fdd_store.get_fault_by_binding(binding.id).await;

        if condition_met && rt.confirmation_counter >= rule.confirmation_count {
            // Fault confirmed — raise if not already active
            if existing_fault.is_none() {
                let snapshot =
                    Self::build_snapshot(point_store, resolver, &rule.condition, &binding.equip_id)
                        .await;
                let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();

                if let Ok(fault_id) = fdd_store
                    .create_fault(
                        binding.id,
                        rule.id,
                        &binding.equip_id,
                        &rule.name,
                        &rule.severity,
                        &snapshot_json,
                        &rule.guidance,
                    )
                    .await
                {
                    event_bus.publish(Event::FddFaultRaised {
                        fault_id,
                        rule_id: rule.id,
                        equip_id: binding.equip_id.clone(),
                        severity: rule.severity.key().to_string(),
                    });
                    tracing::info!(
                        rule = rule.name,
                        equip = binding.equip_id,
                        "FDD fault raised"
                    );
                }
            }
        } else if !condition_met {
            // Condition cleared — clear fault if active
            if let Some(fault) = existing_fault {
                let _ = fdd_store.clear_fault(fault.id).await;
                event_bus.publish(Event::FddFaultCleared {
                    fault_id: fault.id,
                    rule_id: rule.id,
                    equip_id: binding.equip_id.clone(),
                });
                tracing::info!(
                    rule = rule.name,
                    equip = binding.equip_id,
                    "FDD fault cleared"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Condition evaluation
    // -----------------------------------------------------------------------

    async fn evaluate_condition(
        point_store: &PointStore,
        resolver: &PointResolver,
        condition: &FddCondition,
        equip_id: &str,
        params: &FddParams,
        rt: &mut BindingRuntime,
    ) -> bool {
        match condition {
            FddCondition::AllTrue {
                predicates,
                delay_secs,
                applicable_states,
            } => {
                // Check operating state applicability
                if let Some(states) = applicable_states {
                    let current =
                        Self::get_operating_state(point_store, resolver, equip_id, params).await;
                    if !states.contains(&current) {
                        return false;
                    }
                }
                // All predicates must be true
                for pred in predicates {
                    if !Self::evaluate_predicate(point_store, resolver, pred, equip_id, params)
                        .await
                    {
                        return false;
                    }
                }
                // Check delay
                Self::check_delay(*delay_secs, rt)
            }
            FddCondition::AnyTrue {
                predicates,
                delay_secs,
                applicable_states,
            } => {
                if let Some(states) = applicable_states {
                    let current =
                        Self::get_operating_state(point_store, resolver, equip_id, params).await;
                    if !states.contains(&current) {
                        return false;
                    }
                }
                let mut any_true = false;
                for pred in predicates {
                    if Self::evaluate_predicate(point_store, resolver, pred, equip_id, params).await
                    {
                        any_true = true;
                        break;
                    }
                }
                if !any_true {
                    return false;
                }
                Self::check_delay(*delay_secs, rt)
            }
            FddCondition::SensorBounds {
                point_ref,
                low,
                high,
            } => {
                if let Some(node_id) = resolver.resolve(equip_id, point_ref).await {
                    if let Some(val) = Self::read_float(point_store, &node_id) {
                        val < *low || val > *high
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            FddCondition::StuckValue {
                point_ref,
                duration_secs,
                tolerance,
            } => {
                if let Some(node_id) = resolver.resolve(equip_id, point_ref).await {
                    if let Some(val) = Self::read_float(point_store, &node_id) {
                        let now = Instant::now();
                        let entry = rt.last_values.entry(node_id.clone()).or_insert((val, now));
                        if (val - entry.0).abs() > *tolerance {
                            // Value changed — reset tracking
                            *entry = (val, now);
                            false
                        } else {
                            // Value stuck — check duration
                            entry.1.elapsed().as_secs() >= *duration_secs
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            FddCondition::CountInWindow {
                point_ref,
                threshold_count,
                window_secs,
            } => {
                if let Some(node_id) = resolver.resolve(equip_id, point_ref).await {
                    if let Some(val) = Self::read_float(point_store, &node_id) {
                        let now = Instant::now();
                        let window = std::time::Duration::from_secs(*window_secs);

                        // Track transitions (binary threshold crossing at 0.5)
                        let entry = rt.last_values.entry(node_id.clone()).or_insert((val, now));
                        let prev = entry.0;
                        let crossed = (prev < 0.5 && val >= 0.5) || (prev >= 0.5 && val < 0.5);
                        *entry = (val, now);

                        if crossed {
                            rt.transition_timestamps.push_back(now);
                        }
                        // Prune old transitions outside the window
                        while let Some(front) = rt.transition_timestamps.front() {
                            if front.elapsed() > window {
                                rt.transition_timestamps.pop_front();
                            } else {
                                break;
                            }
                        }
                        rt.transition_timestamps.len() as u32 >= *threshold_count
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            FddCondition::ScheduleDeviation { .. } => {
                // Placeholder: full implementation would check the schedule store.
                false
            }
            FddCondition::Custom { .. } => {
                // Placeholder: custom Rhai script evaluation for future implementation.
                false
            }
        }
    }

    // -----------------------------------------------------------------------
    // Predicate evaluation
    // -----------------------------------------------------------------------

    async fn evaluate_predicate(
        point_store: &PointStore,
        resolver: &PointResolver,
        pred: &PointPredicate,
        equip_id: &str,
        params: &FddParams,
    ) -> bool {
        let left_node = match resolver.resolve(equip_id, &pred.point_ref).await {
            Some(id) => id,
            None => return false,
        };
        let left_val = match Self::read_float(point_store, &left_node) {
            Some(v) => v,
            None => return false,
        };

        let right_val = match &pred.value {
            PredicateValue::Literal(v) => *v,
            PredicateValue::PointValue(ref pr) => match resolver.resolve(equip_id, pr).await {
                Some(id) => match Self::read_float(point_store, &id) {
                    Some(v) => v,
                    None => return false,
                },
                None => return false,
            },
        };

        // Determine effective tolerance from:
        // 1. Per-predicate explicit tolerance (if > 0)
        // 2. Per-param sensor tolerance based on point type tags
        // For two-sensor comparisons (PointValue), use RSS of both tolerances.
        let left_tol = if pred.tolerance > 0.0 {
            pred.tolerance
        } else {
            Self::infer_tolerance(&pred.point_ref, params)
        };

        let right_tol = match &pred.value {
            PredicateValue::PointValue(ref pr) => Self::infer_tolerance(pr, params),
            _ => 0.0,
        };

        // RSS for two-sensor comparison, single tolerance for literal comparison
        let tol = if right_tol > 0.0 {
            rss_tolerance(&[left_tol, right_tol])
        } else {
            left_tol
        };

        // Apply fan heat delta: when comparing SAT (discharge temp) against another
        // temperature, subtract fan motor heat rise from SAT.
        let adjusted_left = if Self::is_supply_air_temp(&pred.point_ref) {
            left_val - params.delta_t_supply_fan
        } else {
            left_val
        };

        // Shift threshold by tolerance to prevent false positives near boundary.
        let adjusted_right = match pred.op {
            CompareOp::Gt | CompareOp::Gte => right_val + tol,
            CompareOp::Lt | CompareOp::Lte => right_val - tol,
            _ => right_val,
        };

        pred.op.evaluate(adjusted_left, adjusted_right)
    }

    /// Infer sensor tolerance from point tags and FddParams.
    fn infer_tolerance(point_ref: &PointRef, params: &FddParams) -> f64 {
        let tags = &point_ref.tags;
        if tags.iter().any(|t| t == "temp") {
            params.temp_tolerance
        } else if tags.iter().any(|t| t == "pressure") {
            params.pressure_tolerance
        } else if tags.iter().any(|t| t == "humidity") {
            params.humidity_tolerance
        } else {
            0.0
        }
    }

    /// Check if a point ref represents supply/discharge air temperature.
    fn is_supply_air_temp(point_ref: &PointRef) -> bool {
        let tags = &point_ref.tags;
        tags.iter().any(|t| t == "temp") && tags.iter().any(|t| t == "discharge" || t == "supply")
    }

    // -----------------------------------------------------------------------
    // Operating state detection
    // -----------------------------------------------------------------------

    async fn get_operating_state(
        point_store: &PointStore,
        resolver: &PointResolver,
        equip_id: &str,
        params: &FddParams,
    ) -> OperatingState {
        let htg_ref = PointRef {
            tags: vec!["valve".into(), "heating".into()],
            role: "htg_valve".into(),
        };
        let clg_ref = PointRef {
            tags: vec!["valve".into(), "cooling".into()],
            role: "clg_valve".into(),
        };
        let dpr_ref = PointRef {
            tags: vec!["damper".into(), "outside".into()],
            role: "oa_damper".into(),
        };
        let fan_ref = PointRef {
            tags: vec!["fan".into(), "cmd".into()],
            role: "fan_cmd".into(),
        };

        let htg = resolver
            .resolve(equip_id, &htg_ref)
            .await
            .and_then(|id| Self::read_float(point_store, &id))
            .unwrap_or(0.0);
        let clg = resolver
            .resolve(equip_id, &clg_ref)
            .await
            .and_then(|id| Self::read_float(point_store, &id))
            .unwrap_or(0.0);
        let dpr = resolver
            .resolve(equip_id, &dpr_ref)
            .await
            .and_then(|id| Self::read_float(point_store, &id))
            .unwrap_or(0.0);
        let fan = resolver
            .resolve(equip_id, &fan_ref)
            .await
            .and_then(|id| Self::read_float(point_store, &id))
            .unwrap_or(0.0);

        determine_operating_state(htg, clg, dpr, fan, params.min_oa_damper_pct)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Read a point value as f64 from the PointStore.
    ///
    /// Node IDs follow the convention `"{device_instance_id}/{point_id}"`.
    fn read_float(point_store: &PointStore, node_id: &str) -> Option<f64> {
        let parts: Vec<&str> = node_id.splitn(2, '/').collect();
        if parts.len() == 2 {
            let key = PointKey {
                device_instance_id: parts[0].to_string(),
                point_id: parts[1].to_string(),
            };
            point_store.get(&key).map(|tv| tv.value.as_f64())
        } else {
            None
        }
    }

    /// Check whether a delay requirement has been met.
    fn check_delay(delay_secs: u64, rt: &BindingRuntime) -> bool {
        if delay_secs == 0 {
            return true;
        }
        match rt.condition_since {
            Some(since) => since.elapsed().as_secs() >= delay_secs,
            None => false,
        }
    }

    /// Build a point-value snapshot for a fault record.
    async fn build_snapshot(
        point_store: &PointStore,
        resolver: &PointResolver,
        condition: &FddCondition,
        equip_id: &str,
    ) -> HashMap<String, f64> {
        let mut snapshot = HashMap::new();
        let refs = Self::collect_point_refs(condition);
        for pr in refs {
            if let Some(node_id) = resolver.resolve(equip_id, pr).await {
                if let Some(val) = Self::read_float(point_store, &node_id) {
                    snapshot.insert(pr.role.clone(), val);
                }
            }
        }
        snapshot
    }

    /// Extract all [`PointRef`]s from a condition for snapshot building.
    fn collect_point_refs(condition: &FddCondition) -> Vec<&PointRef> {
        match condition {
            FddCondition::AllTrue { predicates, .. } | FddCondition::AnyTrue { predicates, .. } => {
                let mut refs: Vec<&PointRef> = predicates.iter().map(|p| &p.point_ref).collect();
                for pred in predicates {
                    if let PredicateValue::PointValue(ref pr) = pred.value {
                        refs.push(pr);
                    }
                }
                refs
            }
            FddCondition::SensorBounds { point_ref, .. } => vec![point_ref],
            FddCondition::StuckValue { point_ref, .. } => vec![point_ref],
            FddCondition::CountInWindow { point_ref, .. } => vec![point_ref],
            FddCondition::ScheduleDeviation { point_ref, .. } => vec![point_ref],
            FddCondition::Custom { .. } => vec![],
        }
    }
}
