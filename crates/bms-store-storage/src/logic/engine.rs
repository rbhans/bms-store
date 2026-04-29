use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use rhai::{Dynamic, Engine, Scope, AST};
use tokio::task::JoinHandle;

use crate::config::profile::PointValue;
use crate::event::bus::{Event, EventBus};
use crate::store::point_store::{PointKey, PointStore};

use super::compiler::{compile_program, CompiledProgram};
use super::model::{ExecutionResult, ProgramId, Trigger};
use super::store::ProgramStore;

/// Per-program persistent state for timing blocks and stateful computations.
type ProgramState = Arc<RwLock<HashMap<String, Dynamic>>>;

/// Callback for device writes from the logic engine.
/// Arguments: (node_id, value, priority)
pub type WriteCallback = Arc<dyn Fn(&str, PointValue, Option<u8>) + Send + Sync>;

/// The execution engine runs compiled programs on their configured triggers.
pub struct ExecutionEngine {
    pub program_store: ProgramStore,
    pub point_store: PointStore,
    pub event_bus: EventBus,
    pub write_callback: Option<WriteCallback>,
}

impl ExecutionEngine {
    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    async fn run(self) {
        // Load and compile all enabled programs
        let mut compiled: HashMap<ProgramId, (CompiledProgram, AST)> = HashMap::new();
        let mut states: HashMap<ProgramId, ProgramState> = HashMap::new();
        let mut intervals: HashMap<ProgramId, u64> = HashMap::new(); // interval_ms per periodic program

        self.reload_programs(&mut compiled, &mut states, &mut intervals)
            .await;

        // Subscribe to value change events for OnChange triggers
        let mut event_rx = self.event_bus.subscribe();
        // Subscribe to program store changes for reloads
        let mut store_version = self.program_store.subscribe();

        // Tick interval: run at 1 second resolution (programs with finer intervals still
        // get checked each second; the minimum practical interval is ~1s)
        let mut tick_interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
        let mut tick_counters: HashMap<ProgramId, u64> = HashMap::new(); // ms elapsed since last run

        loop {
            tokio::select! {
                _ = tick_interval.tick() => {
                    // Check each periodic program
                    let programs_to_run: Vec<ProgramId> = intervals.iter()
                        .filter_map(|(pid, interval_ms)| {
                            let counter = tick_counters.entry(pid.clone()).or_insert(0);
                            *counter += 1000; // 1 second per tick
                            if *counter >= *interval_ms {
                                *counter = 0;
                                Some(pid.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    for pid in programs_to_run {
                        if let Some((cp, ast)) = compiled.get(&pid) {
                            let state = states.entry(pid.clone()).or_insert_with(default_state);
                            let result = execute_program(
                                &self.point_store,
                                &self.write_callback,
                                cp,
                                ast,
                                state,
                            );
                            self.program_store.log_execution(result);
                        }
                    }
                }

                Ok(event) = event_rx.recv() => {
                    if let Event::ValueChanged { ref node_id, .. } = *event {
                        // Find OnChange programs that care about this node
                        let programs_to_run: Vec<ProgramId> = compiled.iter()
                            .filter_map(|(pid, (cp, _))| {
                                if cp.trigger_nodes.contains(node_id) {
                                    Some(pid.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        for pid in programs_to_run {
                            if let Some((cp, ast)) = compiled.get(&pid) {
                                let state = states.entry(pid.clone()).or_insert_with(default_state);
                                let result = execute_program(
                                    &self.point_store,
                                    &self.write_callback,
                                    cp,
                                    ast,
                                    state,
                                );
                                self.program_store.log_execution(result);
                            }
                        }
                    }
                }

                Ok(()) = store_version.changed() => {
                    self.reload_programs(&mut compiled, &mut states, &mut intervals).await;
                    tick_counters.clear();
                }
            }
        }
    }

    async fn reload_programs(
        &self,
        compiled: &mut HashMap<ProgramId, (CompiledProgram, AST)>,
        states: &mut HashMap<ProgramId, ProgramState>,
        intervals: &mut HashMap<ProgramId, u64>,
    ) {
        let programs = self.program_store.list(true).await;
        let engine = create_rhai_engine(&self.point_store, &self.write_callback);

        // Clear old data but preserve state for programs that still exist
        let old_states: HashMap<ProgramId, ProgramState> = std::mem::take(states);
        compiled.clear();
        intervals.clear();

        for prog in &programs {
            match compile_program(prog) {
                Ok(cp) => {
                    match engine.compile(&cp.rhai_source) {
                        Ok(ast) => {
                            // Track periodic intervals
                            if let Trigger::Periodic { interval_ms } = &prog.trigger {
                                intervals.insert(prog.id.clone(), *interval_ms);
                            }

                            // Preserve existing state or create new
                            let state = old_states
                                .get(&prog.id)
                                .cloned()
                                .unwrap_or_else(default_state);
                            states.insert(prog.id.clone(), state);

                            compiled.insert(prog.id.clone(), (cp, ast));
                        }
                        Err(e) => {
                            tracing::error!(program = %prog.id, "Logic: failed to compile Rhai: {e}");
                            self.program_store.log_execution(ExecutionResult {
                                program_id: prog.id.clone(),
                                success: false,
                                error: Some(format!("Rhai compile error: {e}")),
                                duration_us: 0,
                                outputs_written: 0,
                            });
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(program = %prog.id, "Logic: failed to compile program: {e}");
                }
            }
        }

        if !compiled.is_empty() {
            tracing::info!(
                total = compiled.len(),
                periodic = intervals.len(),
                on_change = compiled.len() - intervals.len(),
                "Logic engine: programs loaded"
            );
        }
    }
}

/// Execute a single compiled program.
fn execute_program(
    point_store: &PointStore,
    write_callback: &Option<WriteCallback>,
    cp: &CompiledProgram,
    ast: &AST,
    state: &ProgramState,
) -> ExecutionResult {
    let engine = create_rhai_engine(point_store, write_callback);

    // Register state_get/state_set with this program's state
    let mut engine = engine;
    let state_r = state.clone();
    engine.register_fn("state_get", move |key: &str| -> Dynamic {
        let map = state_r.read().unwrap();
        map.get(key).cloned().unwrap_or(Dynamic::UNIT)
    });
    let state_w = state.clone();
    engine.register_fn("state_set", move |key: &str, val: Dynamic| {
        let mut map = state_w.write().unwrap();
        map.insert(key.to_string(), val);
    });

    // Register timestamp() for timing blocks
    engine.register_fn("timestamp", || -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    });

    let mut scope = Scope::new();
    let start = Instant::now();

    match engine.eval_ast_with_scope::<Dynamic>(&mut scope, ast) {
        Ok(_) => {
            let duration_us = start.elapsed().as_micros() as u64;
            ExecutionResult {
                program_id: cp.program_id.clone(),
                success: true,
                error: None,
                duration_us,
                outputs_written: cp.write_nodes.len(),
            }
        }
        Err(e) => {
            let duration_us = start.elapsed().as_micros() as u64;
            tracing::error!(program = %cp.program_id, "Logic: program execution error: {e}");
            ExecutionResult {
                program_id: cp.program_id.clone(),
                success: false,
                error: Some(e.to_string()),
                duration_us,
                outputs_written: 0,
            }
        }
    }
}

/// Create a Rhai engine with read/write/alarm/log functions bound to stores.
fn create_rhai_engine(point_store: &PointStore, write_callback: &Option<WriteCallback>) -> Engine {
    let mut engine = Engine::new();

    // Safety limits
    engine.set_max_operations(10_000);

    // read(node_id) → Dynamic
    let ps_read = point_store.clone();
    engine.register_fn("read", move |node_id: &str| -> Dynamic {
        // Split "device/point" into PointKey
        if let Some((dev, pt)) = node_id.split_once('/') {
            let key = PointKey {
                device_instance_id: dev.to_string(),
                point_id: pt.to_string(),
            };
            match ps_read.get(&key) {
                Some(tv) => point_value_to_dynamic(&tv.value),
                None => Dynamic::from(0.0_f64),
            }
        } else {
            Dynamic::from(0.0_f64)
        }
    });

    // read_status(node_id) → String ("ok", "fault", "down", "stale", "overridden", "alarm", "unknown")
    let ps_status = point_store.clone();
    engine.register_fn("read_status", move |node_id: &str| -> String {
        if let Some((dev, pt)) = node_id.split_once('/') {
            let key = PointKey {
                device_instance_id: dev.to_string(),
                point_id: pt.to_string(),
            };
            match ps_status.get(&key) {
                Some(tv) => tv.status.worst_status().unwrap_or("ok").to_string(),
                None => "unknown".into(),
            }
        } else {
            "unknown".into()
        }
    });

    // write(node_id, value) — 2-arg version (no priority)
    let ps_write = point_store.clone();
    let write_cb_2 = write_callback.clone();
    engine.register_fn("write", move |node_id: &str, value: Dynamic| {
        if let Some(pv) = dynamic_to_point_value(&value) {
            if let Some((dev, pt)) = node_id.split_once('/') {
                let key = PointKey {
                    device_instance_id: dev.to_string(),
                    point_id: pt.to_string(),
                };
                ps_write.set(key, pv.clone());
            }
            if let Some(ref cb) = write_cb_2 {
                cb(node_id, pv, None);
            }
        }
    });

    // write(node_id, value, priority) — 3-arg version
    let ps_write3 = point_store.clone();
    let write_cb_3 = write_callback.clone();
    engine.register_fn(
        "write",
        move |node_id: &str, value: Dynamic, priority: i64| {
            if let Some(pv) = dynamic_to_point_value(&value) {
                if let Some((dev, pt)) = node_id.split_once('/') {
                    let key = PointKey {
                        device_instance_id: dev.to_string(),
                        point_id: pt.to_string(),
                    };
                    ps_write3.set(key, pv.clone());
                }
                if let Some(ref cb) = write_cb_3 {
                    cb(node_id, pv, Some(priority as u8));
                }
            }
        },
    );

    // alarm(node_id, message) — log for now, alarm store integration later
    engine.register_fn("alarm", |node_id: &str, message: &str| {
        tracing::warn!(node_id, message, "Logic: alarm triggered");
    });

    // log(message)
    engine.register_fn("log", |message: &str| {
        tracing::info!(message, "Logic: script log");
    });

    engine
}

fn point_value_to_dynamic(pv: &PointValue) -> Dynamic {
    match pv {
        PointValue::Float(f) => Dynamic::from(*f),
        PointValue::Integer(i) => Dynamic::from(*i),
        PointValue::Bool(b) => Dynamic::from(*b),
    }
}

fn dynamic_to_point_value(d: &Dynamic) -> Option<PointValue> {
    if let Ok(f) = d.as_float() {
        Some(PointValue::Float(f))
    } else if let Ok(i) = d.as_int() {
        Some(PointValue::Integer(i))
    } else if let Ok(b) = d.as_bool() {
        Some(PointValue::Bool(b))
    } else {
        None
    }
}

fn default_state() -> ProgramState {
    Arc::new(RwLock::new(HashMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::model::*;

    #[test]
    fn rhai_read_write_integration() {
        let store = PointStore::new();

        // Seed a value
        let key = PointKey {
            device_instance_id: "ahu-1".into(),
            point_id: "oat".into(),
        };
        store.set(key, PointValue::Float(72.5));

        let engine = create_rhai_engine(&store, &None);
        let ast = engine
            .compile(r#"let t = read("ahu-1/oat"); write("ahu-1/cmd", t + 1.0);"#)
            .unwrap();

        let mut scope = Scope::new();
        engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &ast)
            .unwrap();

        // Check the written value
        let out_key = PointKey {
            device_instance_id: "ahu-1".into(),
            point_id: "cmd".into(),
        };
        let written = store.get(&out_key).unwrap().value;
        assert!(matches!(written, PointValue::Float(f) if (f - 73.5).abs() < f64::EPSILON));
    }

    #[test]
    fn rhai_state_persistence() {
        let store = PointStore::new();
        let engine = create_rhai_engine(&store, &None);
        let state = default_state();

        // Register state functions
        let mut engine = engine;
        let sr = state.clone();
        engine.register_fn("state_get", move |key: &str| -> Dynamic {
            sr.read()
                .unwrap()
                .get(key)
                .cloned()
                .unwrap_or(Dynamic::UNIT)
        });
        let sw = state.clone();
        engine.register_fn("state_set", move |key: &str, val: Dynamic| {
            sw.write().unwrap().insert(key.to_string(), val);
        });

        // First run: set state
        let ast = engine.compile(r#"state_set("counter", 42);"#).unwrap();
        let mut scope = Scope::new();
        engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &ast)
            .unwrap();

        // Second run: read state
        let ast2 = engine
            .compile(r#"let c = state_get("counter"); write("dev/count", c);"#)
            .unwrap();
        let mut scope2 = Scope::new();
        engine
            .eval_ast_with_scope::<Dynamic>(&mut scope2, &ast2)
            .unwrap();

        let key = PointKey {
            device_instance_id: "dev".into(),
            point_id: "count".into(),
        };
        let val = store.get(&key).unwrap().value;
        assert!(matches!(val, PointValue::Integer(42)));
    }

    #[test]
    fn compiled_program_executes() {
        let store = PointStore::new();

        // Seed input
        store.set(
            PointKey {
                device_instance_id: "ahu-1".into(),
                point_id: "oat".into(),
            },
            PointValue::Float(50.0),
        );

        // Build a program: read OAT, compare < 55, write result
        let prog = Program {
            id: "eco".into(),
            name: "Economizer".into(),
            description: String::new(),
            enabled: true,
            trigger: Trigger::Periodic { interval_ms: 5000 },
            blocks: vec![
                Block {
                    id: "r1".into(),
                    block_type: BlockType::PointRead {
                        node_id: "ahu-1/oat".into(),
                    },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
                Block {
                    id: "c1".into(),
                    block_type: BlockType::Constant {
                        value: PointValue::Float(55.0),
                    },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
                Block {
                    id: "cmp1".into(),
                    block_type: BlockType::Compare { op: CompareOp::Lt },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
                Block {
                    id: "w1".into(),
                    block_type: BlockType::PointWrite {
                        node_id: "ahu-1/eco-enable".into(),
                        priority: None,
                    },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
            ],
            wires: vec![
                Wire {
                    from_block: "r1".into(),
                    from_port: "value".into(),
                    to_block: "cmp1".into(),
                    to_port: "a".into(),
                },
                Wire {
                    from_block: "c1".into(),
                    from_port: "value".into(),
                    to_block: "cmp1".into(),
                    to_port: "b".into(),
                },
                Wire {
                    from_block: "cmp1".into(),
                    from_port: "result".into(),
                    to_block: "w1".into(),
                    to_port: "value".into(),
                },
            ],
            rhai_override: None,
            created_ms: 0,
            updated_ms: 0,
        };

        let cp = compile_program(&prog).unwrap();
        let engine = create_rhai_engine(&store, &None);
        let ast = engine.compile(&cp.rhai_source).unwrap();
        let state = default_state();

        let result = execute_program(&store, &None, &cp, &ast, &state);
        assert!(result.success);

        // OAT=50 < 55 → true → write true to eco-enable
        let key = PointKey {
            device_instance_id: "ahu-1".into(),
            point_id: "eco-enable".into(),
        };
        let val = store.get(&key).unwrap().value;
        assert!(matches!(val, PointValue::Bool(true)));
    }
}
