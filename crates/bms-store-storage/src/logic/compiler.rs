use std::collections::{HashMap, HashSet, VecDeque};

use crate::node::NodeId;

use super::model::*;

/// Compiled program ready for Rhai execution.
#[derive(Debug, Clone)]
pub struct CompiledProgram {
    pub program_id: ProgramId,
    pub rhai_source: String,
    /// All node IDs read by the program.
    pub read_nodes: Vec<NodeId>,
    /// All node IDs written by the program.
    pub write_nodes: Vec<NodeId>,
    /// Node IDs that trigger execution (for OnChange programs).
    pub trigger_nodes: Vec<NodeId>,
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("cycle detected in block graph")]
    CycleDetected,
    #[error("unknown block referenced in wire: {0}")]
    UnknownBlock(String),
    #[error("empty program")]
    EmptyProgram,
}

/// Compile a block program into Rhai source code.
pub fn compile_program(program: &Program) -> Result<CompiledProgram, CompileError> {
    // If user provided raw Rhai, use it directly
    if let Some(ref code) = program.rhai_override {
        let mut read_nodes = Vec::new();
        let mut write_nodes = Vec::new();

        // Extract node references from blocks even when using override
        for block in &program.blocks {
            match &block.block_type {
                BlockType::PointRead { node_id } => read_nodes.push(node_id.clone()),
                BlockType::PointWrite { node_id, .. } | BlockType::VirtualPoint { node_id } => {
                    write_nodes.push(node_id.clone())
                }
                _ => {}
            }
        }

        let trigger_nodes = match &program.trigger {
            Trigger::OnChange { node_ids } => node_ids.clone(),
            Trigger::Periodic { .. } => vec![],
        };

        return Ok(CompiledProgram {
            program_id: program.id.clone(),
            rhai_source: code.clone(),
            read_nodes,
            write_nodes,
            trigger_nodes,
        });
    }

    if program.blocks.is_empty() {
        return Err(CompileError::EmptyProgram);
    }

    // Build block lookup
    let block_map: HashMap<&str, &Block> =
        program.blocks.iter().map(|b| (b.id.as_str(), b)).collect();

    // Validate wires reference existing blocks
    for wire in &program.wires {
        if !block_map.contains_key(wire.from_block.as_str()) {
            return Err(CompileError::UnknownBlock(wire.from_block.clone()));
        }
        if !block_map.contains_key(wire.to_block.as_str()) {
            return Err(CompileError::UnknownBlock(wire.to_block.clone()));
        }
    }

    // Topological sort (Kahn's algorithm)
    let sorted = topological_sort(&program.blocks, &program.wires)?;

    // Build wire lookup: (to_block, to_port) → (from_block, from_port)
    let mut input_map: HashMap<(&str, &str), (&str, &str)> = HashMap::new();
    for wire in &program.wires {
        input_map.insert(
            (wire.to_block.as_str(), wire.to_port.as_str()),
            (wire.from_block.as_str(), wire.from_port.as_str()),
        );
    }

    // Generate Rhai code
    let mut code = String::new();
    let mut read_nodes = Vec::new();
    let mut write_nodes = Vec::new();

    for block_id in &sorted {
        let block = block_map[block_id.as_str()];
        let var_prefix = sanitize_id(block_id);
        let (inputs, outputs) = block_ports(&block.block_type);

        // Skip disabled blocks — emit default outputs so downstream still compiles
        if !block.enabled {
            for out_port in &outputs {
                let default = default_for_type(&out_port.data_type);
                code.push_str(&format!(
                    "let {}_{} = {};\n",
                    var_prefix, out_port.name, default
                ));
            }
            continue;
        }

        // Resolve input variables
        let mut input_vars: HashMap<String, String> = HashMap::new();
        for port in &inputs {
            let var_name = if let Some(&(from_block, from_port)) =
                input_map.get(&(block_id.as_str(), port.name.as_str()))
            {
                format!("{}_{}", sanitize_id(from_block), from_port)
            } else {
                // Unconnected — use default value
                let default = port
                    .default_value
                    .as_ref()
                    .map(json_to_rhai_literal)
                    .unwrap_or_else(|| default_for_type(&port.data_type));
                // Emit a local variable for the default
                let def_var = format!("{}_{}", var_prefix, port.name);
                code.push_str(&format!("let {} = {};\n", def_var, default));
                def_var
            };
            input_vars.insert(port.name.clone(), var_name);
        }

        // Emit code for this block
        match &block.block_type {
            BlockType::PointRead { node_id } => {
                code.push_str(&format!(
                    "let {}_value = read(\"{}\");\n",
                    var_prefix, node_id
                ));
                read_nodes.push(node_id.clone());
            }

            BlockType::Constant { value } => {
                let lit = point_value_to_rhai(value);
                code.push_str(&format!("let {}_value = {};\n", var_prefix, lit));
            }

            BlockType::Math { op } => {
                let a = input_vars.get("a").or_else(|| input_vars.get("value"));
                let expr = match op {
                    MathOp::Add => format!("{} + {}", a.unwrap(), input_vars["b"]),
                    MathOp::Sub => format!("{} - {}", a.unwrap(), input_vars["b"]),
                    MathOp::Mul => format!("{} * {}", a.unwrap(), input_vars["b"]),
                    MathOp::Div => format!(
                        "if {} != 0.0 {{ {} / {} }} else {{ 0.0 }}",
                        input_vars["b"],
                        a.unwrap(),
                        input_vars["b"]
                    ),
                    MathOp::Min => format!(
                        "if {} < {} {{ {} }} else {{ {} }}",
                        a.unwrap(),
                        input_vars["b"],
                        a.unwrap(),
                        input_vars["b"]
                    ),
                    MathOp::Max => format!(
                        "if {} > {} {{ {} }} else {{ {} }}",
                        a.unwrap(),
                        input_vars["b"],
                        a.unwrap(),
                        input_vars["b"]
                    ),
                    MathOp::Abs => {
                        let v = input_vars.get("value").unwrap();
                        format!("if {} < 0.0 {{ -{} }} else {{ {} }}", v, v, v)
                    }
                    MathOp::Clamp => format!(
                        "if {v} < {mn} {{ {mn} }} else if {v} > {mx} {{ {mx} }} else {{ {v} }}",
                        v = input_vars["value"],
                        mn = input_vars["min"],
                        mx = input_vars["max"]
                    ),
                };
                code.push_str(&format!("let {}_result = {};\n", var_prefix, expr));
            }

            BlockType::Logic { op } => {
                let expr = match op {
                    LogicOp::And => format!("{} && {}", input_vars["a"], input_vars["b"]),
                    LogicOp::Or => format!("{} || {}", input_vars["a"], input_vars["b"]),
                    LogicOp::Not => format!("!{}", input_vars["value"]),
                    LogicOp::Xor => {
                        format!(
                            "({a} && !{b}) || (!{a} && {b})",
                            a = input_vars["a"],
                            b = input_vars["b"]
                        )
                    }
                };
                code.push_str(&format!("let {}_result = {};\n", var_prefix, expr));
            }

            BlockType::Compare { op } => {
                let op_str = match op {
                    CompareOp::Gt => ">",
                    CompareOp::Lt => "<",
                    CompareOp::Gte => ">=",
                    CompareOp::Lte => "<=",
                    CompareOp::Eq => "==",
                    CompareOp::Neq => "!=",
                };
                code.push_str(&format!(
                    "let {}_result = {} {} {};\n",
                    var_prefix, input_vars["a"], op_str, input_vars["b"]
                ));
            }

            BlockType::Select => {
                code.push_str(&format!(
                    "let {}_result = if {} {{ {} }} else {{ {} }};\n",
                    var_prefix,
                    input_vars["condition"],
                    input_vars["if_true"],
                    input_vars["if_false"]
                ));
            }

            BlockType::Timing { op, period_ms } => {
                let v = &input_vars["value"];
                let state_key = format!("{}_state", var_prefix);
                match op {
                    TimingOp::MovingAverage => {
                        // Use period_ms as window. EMA alpha = 2 / (N + 1) where N = period_ms / 1000 (min 1)
                        let n = (*period_ms / 1000).max(1);
                        code.push_str(&format!(
                            "let {pfx}_prev_avg = state_get(\"{key}_avg\");\n\
                             let {pfx}_result = if {pfx}_prev_avg == () {{ {v} }} else {{\n\
                                 let {pfx}_alpha = 2.0 / ({n}.0 + 1.0);\n\
                                 {pfx}_alpha * {v} + (1.0 - {pfx}_alpha) * {pfx}_prev_avg\n\
                             }};\n\
                             state_set(\"{key}_avg\", {pfx}_result);\n",
                            pfx = var_prefix,
                            key = state_key,
                            v = v,
                            n = n,
                        ));
                    }
                    TimingOp::RateOfChange => {
                        code.push_str(&format!(
                            "let {pfx}_prev = state_get(\"{key}_prev\");\n\
                             let {pfx}_result = if {pfx}_prev == () {{ 0.0 }} else {{ ({v} - {pfx}_prev) / ({period}.0 / 1000.0) }};\n\
                             state_set(\"{key}_prev\", {v});\n",
                            pfx = var_prefix,
                            key = state_key,
                            v = v,
                            period = period_ms,
                        ));
                    }
                    TimingOp::DelayOn | TimingOp::DelayOff => {
                        let check_val = matches!(op, TimingOp::DelayOn);
                        code.push_str(&format!(
                            "let {pfx}_start = state_get(\"{key}_start\");\n\
                             let {pfx}_result = false;\n\
                             if {v} == {check} {{\n\
                                 if {pfx}_start == () {{ state_set(\"{key}_start\", timestamp()); }}\n\
                                 else if timestamp() - {pfx}_start >= {period} {{ {pfx}_result = {check}; }}\n\
                             }} else {{\n\
                                 state_set(\"{key}_start\", ());\n\
                                 {pfx}_result = !{check};\n\
                             }}\n",
                            pfx = var_prefix,
                            key = state_key,
                            v = v,
                            check = check_val,
                            period = period_ms,
                        ));
                    }
                }
            }

            BlockType::Pid {
                kp,
                ki,
                kd,
                output_min,
                output_max,
            } => {
                let pv = &input_vars["process_variable"];
                let sp = &input_vars["setpoint"];
                let state_key = format!("{}_state", var_prefix);
                code.push_str(&format!(
                    "let {pfx}_error = {sp} - {pv};\n\
                     let {pfx}_integral = state_get(\"{key}_integral\");\n\
                     let {pfx}_prev_error = state_get(\"{key}_prev_error\");\n\
                     if {pfx}_integral == () {{ {pfx}_integral = 0.0; {pfx}_prev_error = 0.0; }}\n\
                     {pfx}_integral = {pfx}_integral + {pfx}_error;\n\
                     let {pfx}_derivative = {pfx}_error - {pfx}_prev_error;\n\
                     let {pfx}_raw = {kp} * {pfx}_error + {ki} * {pfx}_integral + {kd} * {pfx}_derivative;\n\
                     let {pfx}_output = if {pfx}_raw < {omin} {{ {omin} }} else if {pfx}_raw > {omax} {{ {omax} }} else {{ {pfx}_raw }};\n\
                     state_set(\"{key}_integral\", {pfx}_integral);\n\
                     state_set(\"{key}_prev_error\", {pfx}_error);\n",
                    pfx = var_prefix,
                    key = state_key,
                    pv = pv,
                    sp = sp,
                    kp = kp,
                    ki = ki,
                    kd = kd,
                    omin = output_min,
                    omax = output_max,
                ));
            }

            BlockType::PointWrite { node_id, priority } => {
                let v = &input_vars["value"];
                match priority {
                    Some(p) => code.push_str(&format!("write(\"{}\", {}, {});\n", node_id, v, p)),
                    None => code.push_str(&format!("write(\"{}\", {});\n", node_id, v)),
                }
                write_nodes.push(node_id.clone());
            }

            BlockType::VirtualPoint { node_id } => {
                let v = &input_vars["value"];
                code.push_str(&format!("write(\"{}\", {});\n", node_id, v));
                write_nodes.push(node_id.clone());
            }

            BlockType::AlarmTrigger { node_id, message } => {
                let cond = &input_vars["condition"];
                code.push_str(&format!(
                    "if {} {{ alarm(\"{}\", \"{}\"); }}\n",
                    cond,
                    node_id,
                    message.replace('"', "\\\"")
                ));
            }

            BlockType::Log { prefix } => {
                let v = &input_vars["value"];
                code.push_str(&format!("log(\"[{}] \" + {});\n", prefix, v));
            }

            BlockType::Latch => {
                let set = &input_vars["set"];
                let reset = &input_vars["reset"];
                let state_key = format!("{}_state", var_prefix);
                code.push_str(&format!(
                    "let {pfx}_prev = state_get(\"{key}\");\n\
                     if {pfx}_prev == () {{ {pfx}_prev = false; }}\n\
                     let {pfx}_result = if {reset} {{ false }} else if {set} {{ true }} else {{ {pfx}_prev }};\n\
                     state_set(\"{key}\", {pfx}_result);\n",
                    pfx = var_prefix,
                    key = state_key,
                    set = set,
                    reset = reset,
                ));
            }

            BlockType::OneShot => {
                let trigger = &input_vars["trigger"];
                let state_key = format!("{}_state", var_prefix);
                code.push_str(&format!(
                    "let {pfx}_prev = state_get(\"{key}\");\n\
                     if {pfx}_prev == () {{ {pfx}_prev = false; }}\n\
                     let {pfx}_result = {trigger} && !{pfx}_prev;\n\
                     state_set(\"{key}\", {trigger});\n",
                    pfx = var_prefix,
                    key = state_key,
                    trigger = trigger,
                ));
            }

            BlockType::Scale {
                in_min,
                in_max,
                out_min,
                out_max,
            } => {
                let v = &input_vars["value"];
                let range_in = in_max - in_min;
                let range_out = out_max - out_min;
                if range_in.abs() < 1e-12 {
                    code.push_str(&format!("let {}_result = {};\n", var_prefix, out_min));
                } else {
                    code.push_str(&format!(
                        "let {pfx}_result = {out_min} + ({v} - {in_min}) * ({range_out} / {range_in});\n\
                         if {pfx}_result < {lo} {{ {pfx}_result = {lo}; }}\n\
                         if {pfx}_result > {hi} {{ {pfx}_result = {hi}; }}\n",
                        pfx = var_prefix,
                        v = v,
                        in_min = in_min,
                        out_min = out_min,
                        range_out = range_out,
                        range_in = range_in,
                        lo = out_min.min(*out_max),
                        hi = out_min.max(*out_max),
                    ));
                }
            }

            BlockType::RampLimit { max_rate } => {
                let v = &input_vars["value"];
                let state_key = format!("{}_state", var_prefix);
                code.push_str(&format!(
                    "let {pfx}_prev = state_get(\"{key}\");\n\
                     let {pfx}_result = if {pfx}_prev == () {{ {v} }} else {{\n\
                         let {pfx}_delta = {v} - {pfx}_prev;\n\
                         let {pfx}_max_delta = {rate};\n\
                         if {pfx}_delta > {pfx}_max_delta {{ {pfx}_prev + {pfx}_max_delta }}\n\
                         else if {pfx}_delta < -{pfx}_max_delta {{ {pfx}_prev - {pfx}_max_delta }}\n\
                         else {{ {v} }}\n\
                     }};\n\
                     state_set(\"{key}\", {pfx}_result);\n",
                    pfx = var_prefix,
                    key = state_key,
                    v = v,
                    rate = max_rate,
                ));
            }

            BlockType::CustomScript { code: user_code } => {
                code.push_str(&format!("// Custom script block: {}\n", block.id));
                // Create user-friendly aliases (in1, in2, in3, out)
                code.push_str(&format!(
                    "let in1 = {in1};\nlet in2 = {in2};\nlet in3 = {in3};\nlet out = ();\n",
                    in1 = input_vars.get("in1").map(|s| s.as_str()).unwrap_or("()"),
                    in2 = input_vars.get("in2").map(|s| s.as_str()).unwrap_or("()"),
                    in3 = input_vars.get("in3").map(|s| s.as_str()).unwrap_or("()"),
                ));
                code.push_str(user_code);
                // Capture output for downstream wires
                code.push_str(&format!("\nlet {}_out = out;\n", var_prefix));
            }
        }
    }

    let trigger_nodes = match &program.trigger {
        Trigger::OnChange { node_ids } => node_ids.clone(),
        Trigger::Periodic { .. } => vec![],
    };

    Ok(CompiledProgram {
        program_id: program.id.clone(),
        rhai_source: code,
        read_nodes,
        write_nodes,
        trigger_nodes,
    })
}

/// Topological sort using Kahn's algorithm.
fn topological_sort(blocks: &[Block], wires: &[Wire]) -> Result<Vec<BlockId>, CompileError> {
    let block_ids: HashSet<&str> = blocks.iter().map(|b| b.id.as_str()).collect();

    // Build adjacency list and in-degree counts
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();

    for id in &block_ids {
        in_degree.insert(id, 0);
        successors.insert(id, vec![]);
    }

    for wire in wires {
        if let Some(deg) = in_degree.get_mut(wire.to_block.as_str()) {
            *deg += 1;
        }
        if let Some(succs) = successors.get_mut(wire.from_block.as_str()) {
            succs.push(wire.to_block.as_str());
        }
    }

    let mut queue: VecDeque<&str> = VecDeque::new();
    for (&id, &deg) in &in_degree {
        if deg == 0 {
            queue.push_back(id);
        }
    }

    let mut sorted = Vec::new();
    while let Some(id) = queue.pop_front() {
        sorted.push(id.to_string());
        if let Some(succs) = successors.get(id) {
            for &succ in succs {
                if let Some(deg) = in_degree.get_mut(succ) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(succ);
                    }
                }
            }
        }
    }

    if sorted.len() != block_ids.len() {
        return Err(CompileError::CycleDetected);
    }

    Ok(sorted)
}

/// Sanitize a block ID for use as a Rhai variable name.
fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn json_to_rhai_literal(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                format!("{:.1}", f)
            } else {
                n.to_string()
            }
        }
        _ => "0.0".to_string(),
    }
}

fn default_for_type(dt: &PortDataType) -> String {
    match dt {
        PortDataType::Float => "0.0".to_string(),
        PortDataType::Integer => "0".to_string(),
        PortDataType::Bool => "false".to_string(),
        PortDataType::Any => "0.0".to_string(),
    }
}

fn point_value_to_rhai(v: &crate::config::profile::PointValue) -> String {
    match v {
        crate::config::profile::PointValue::Bool(b) => b.to_string(),
        crate::config::profile::PointValue::Integer(i) => i.to_string(),
        crate::config::profile::PointValue::Float(f) => {
            if f.fract() == 0.0 {
                format!("{:.1}", f)
            } else {
                f.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile::PointValue;

    fn simple_program() -> Program {
        Program {
            id: "test".into(),
            name: "Test".into(),
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
                        node_id: "ahu-1/eco-cmd".into(),
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
        }
    }

    #[test]
    fn compile_simple_program() {
        let prog = simple_program();
        let compiled = compile_program(&prog).unwrap();

        assert_eq!(compiled.program_id, "test");
        assert!(compiled.rhai_source.contains("read(\"ahu-1/oat\")"));
        assert!(compiled.rhai_source.contains("55.0"));
        assert!(compiled.rhai_source.contains("<"));
        assert!(compiled.rhai_source.contains("write(\"ahu-1/eco-cmd\""));
        assert_eq!(compiled.read_nodes, vec!["ahu-1/oat"]);
        assert_eq!(compiled.write_nodes, vec!["ahu-1/eco-cmd"]);
    }

    #[test]
    fn compile_detects_cycle() {
        let prog = Program {
            id: "cycle".into(),
            name: "Cycle".into(),
            description: String::new(),
            enabled: true,
            trigger: Trigger::Periodic { interval_ms: 1000 },
            blocks: vec![
                Block {
                    id: "a".into(),
                    block_type: BlockType::Math { op: MathOp::Add },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
                Block {
                    id: "b".into(),
                    block_type: BlockType::Math { op: MathOp::Add },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
            ],
            wires: vec![
                Wire {
                    from_block: "a".into(),
                    from_port: "result".into(),
                    to_block: "b".into(),
                    to_port: "a".into(),
                },
                Wire {
                    from_block: "b".into(),
                    from_port: "result".into(),
                    to_block: "a".into(),
                    to_port: "a".into(),
                },
            ],
            rhai_override: None,
            created_ms: 0,
            updated_ms: 0,
        };

        let result = compile_program(&prog);
        assert!(matches!(result, Err(CompileError::CycleDetected)));
    }

    #[test]
    fn compile_rhai_override() {
        let mut prog = simple_program();
        prog.rhai_override = Some("let x = read(\"test\"); write(\"out\", x);".into());

        let compiled = compile_program(&prog).unwrap();
        assert_eq!(
            compiled.rhai_source,
            "let x = read(\"test\"); write(\"out\", x);"
        );
    }

    #[test]
    fn compile_empty_program() {
        let prog = Program {
            id: "empty".into(),
            name: "Empty".into(),
            description: String::new(),
            enabled: true,
            trigger: Trigger::Periodic { interval_ms: 1000 },
            blocks: vec![],
            wires: vec![],
            rhai_override: None,
            created_ms: 0,
            updated_ms: 0,
        };
        assert!(matches!(
            compile_program(&prog),
            Err(CompileError::EmptyProgram)
        ));
    }

    #[test]
    fn compile_pid_block() {
        let prog = Program {
            id: "pid-test".into(),
            name: "PID".into(),
            description: String::new(),
            enabled: true,
            trigger: Trigger::Periodic { interval_ms: 1000 },
            blocks: vec![
                Block {
                    id: "pv".into(),
                    block_type: BlockType::PointRead {
                        node_id: "ahu-1/dat".into(),
                    },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
                Block {
                    id: "sp".into(),
                    block_type: BlockType::Constant {
                        value: PointValue::Float(72.0),
                    },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
                Block {
                    id: "pid1".into(),
                    block_type: BlockType::Pid {
                        kp: 1.0,
                        ki: 0.1,
                        kd: 0.01,
                        output_min: 0.0,
                        output_max: 100.0,
                    },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
                Block {
                    id: "out".into(),
                    block_type: BlockType::PointWrite {
                        node_id: "ahu-1/cool-valve".into(),
                        priority: Some(8),
                    },
                    x: 0.0,
                    y: 0.0,
                    enabled: true,
                },
            ],
            wires: vec![
                Wire {
                    from_block: "pv".into(),
                    from_port: "value".into(),
                    to_block: "pid1".into(),
                    to_port: "process_variable".into(),
                },
                Wire {
                    from_block: "sp".into(),
                    from_port: "value".into(),
                    to_block: "pid1".into(),
                    to_port: "setpoint".into(),
                },
                Wire {
                    from_block: "pid1".into(),
                    from_port: "output".into(),
                    to_block: "out".into(),
                    to_port: "value".into(),
                },
            ],
            rhai_override: None,
            created_ms: 0,
            updated_ms: 0,
        };

        let compiled = compile_program(&prog).unwrap();
        assert!(compiled.rhai_source.contains("state_get"));
        assert!(compiled.rhai_source.contains("state_set"));
        assert!(compiled.rhai_source.contains("1")); // kp
        assert!(compiled.rhai_source.contains("write(\"ahu-1/cool-valve\""));
    }

    #[test]
    fn topological_sort_linear() {
        let blocks = vec![
            Block {
                id: "c".into(),
                block_type: BlockType::Constant {
                    value: PointValue::Float(1.0),
                },
                x: 0.0,
                y: 0.0,
                enabled: true,
            },
            Block {
                id: "b".into(),
                block_type: BlockType::Math { op: MathOp::Abs },
                x: 0.0,
                y: 0.0,
                enabled: true,
            },
            Block {
                id: "a".into(),
                block_type: BlockType::PointRead {
                    node_id: "x".into(),
                },
                x: 0.0,
                y: 0.0,
                enabled: true,
            },
        ];
        let wires = vec![
            Wire {
                from_block: "a".into(),
                from_port: "value".into(),
                to_block: "b".into(),
                to_port: "value".into(),
            },
            Wire {
                from_block: "b".into(),
                from_port: "result".into(),
                to_block: "c".into(),
                to_port: "value".into(),
            },
        ];

        let sorted = topological_sort(&blocks, &wires).unwrap();
        let a_pos = sorted.iter().position(|s| s == "a").unwrap();
        let b_pos = sorted.iter().position(|s| s == "b").unwrap();
        let c_pos = sorted.iter().position(|s| s == "c").unwrap();
        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }
}
