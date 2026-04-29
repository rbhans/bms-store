use std::collections::HashMap;

use dioxus::prelude::*;

use crate::auth::Permission;
use crate::config::profile::PointValue;
use crate::gui::state::AppState;
use crate::logic::compiler::compile_program;
use crate::logic::model::*;
use crate::logic::store::ExecutionLogEntry;
use crate::logic::templates::{self, TemplateCategory};
use bms_store_storage::store::node_store::NodeRecord;

// ----------------------------------------------------------------
// Wire sheet constants
// ----------------------------------------------------------------

const BLOCK_W: f64 = 160.0;
const PORT_RADIUS: f64 = 6.0;
const PORT_SPACING: f64 = 24.0;
const TITLE_HEIGHT: f64 = 28.0;
const GRID_SIZE: f64 = 20.0;
const CANVAS_W: f64 = 4000.0;
const CANVAS_H: f64 = 3000.0;
const PALETTE_WIDTH: f64 = 160.0;

// ----------------------------------------------------------------
// Wire sheet interaction state
// ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum WireDrag {
    None,
    /// Moving a block: (block_id, offset_x, offset_y)
    MoveBlock(BlockId, f64, f64),
    /// Drawing a wire from output port: (from_block, from_port, mouse_x, mouse_y)
    DrawWire(BlockId, String, f64, f64),
    /// Dragging from palette: (bt_key, canvas_x, canvas_y)
    PaletteDrop(String, f64, f64),
}

#[derive(Debug, Clone, PartialEq)]
enum WireSelection {
    None,
    Block(BlockId),
    Wire(usize),
}

// ----------------------------------------------------------------
// Tab state
// ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum ProgramTab {
    Blocks,
    Code,
    Log,
}

// ----------------------------------------------------------------
// ProgrammingView — 3-pane layout (browser | editor | properties)
// ----------------------------------------------------------------

#[component]
pub fn ProgrammingView() -> Element {
    let state = use_context::<AppState>();
    if !state.has_permission(Permission::ManagePrograms) {
        return rsx! {
            div { class: "view-placeholder",
                h2 { "Access Denied" }
                p { "Only administrators can access the programming view." }
            }
        };
    }

    let mut tab = use_signal(|| ProgramTab::Blocks);
    let selected_program: Signal<Option<ProgramId>> = use_signal(|| None);
    let refresh_counter = use_signal(|| 0u64);
    let selected_block: Signal<Option<BlockId>> = use_signal(|| None);
    let current_tab = *tab.read();
    let sel_id = selected_program.read().clone();

    rsx! {
        ProgramBrowser {
            selected_program,
            refresh_counter,
        }
        div { class: "main-content",
            if let Some(ref pid) = sel_id {
                div { class: "schedule-tabs",
                    button {
                        class: if current_tab == ProgramTab::Blocks { "schedule-tab active" } else { "schedule-tab" },
                        onclick: move |_| tab.set(ProgramTab::Blocks),
                        "Blocks"
                    }
                    button {
                        class: if current_tab == ProgramTab::Code { "schedule-tab active" } else { "schedule-tab" },
                        onclick: move |_| tab.set(ProgramTab::Code),
                        "Code"
                    }
                    button {
                        class: if current_tab == ProgramTab::Log { "schedule-tab active" } else { "schedule-tab" },
                        onclick: move |_| tab.set(ProgramTab::Log),
                        "Log"
                    }
                }
                div { class: "schedule-tab-content",
                    match current_tab {
                        ProgramTab::Blocks => rsx! { BlocksTab { program_id: pid.clone(), refresh_counter, selected_block } },
                        ProgramTab::Code => rsx! { CodeTab { program_id: pid.clone(), refresh_counter } },
                        ProgramTab::Log => rsx! { LogTab { program_id: pid.clone() } },
                    }
                }
            } else {
                div { class: "schedule-empty",
                    h3 { "Programming" }
                    p { "Select a program from the browser or create a new one." }
                    p { class: "prog-hint",
                        "Programs are visual block diagrams that compile to Rhai scripts. "
                        "They read sensor values, perform logic, and write outputs — automatically."
                    }
                }
            }
        }
        if sel_id.is_some() {
            ProgramPropertiesPanel {
                program_id: sel_id.clone().unwrap(),
                selected_program,
                refresh_counter,
                selected_block,
            }
        }
    }
}

// ----------------------------------------------------------------
// Left pane: Program browser
// ----------------------------------------------------------------

#[component]
fn ProgramBrowser(
    selected_program: Signal<Option<ProgramId>>,
    refresh_counter: Signal<u64>,
) -> Element {
    let state = use_context::<AppState>();
    let _refresh = *refresh_counter.read();
    let mut programs = use_signal(Vec::<Program>::new);
    let mut show_new = use_signal(|| false);
    let mut show_templates = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_trigger = use_signal(|| "periodic".to_string());
    let mut status = use_signal(|| Option::<String>::None);

    let prog_store = state.program_store.clone();
    let _ = use_resource(move || {
        let ps = prog_store.clone();
        let _r = *refresh_counter.read();
        async move {
            let p = ps.list(false).await;
            programs.set(p);
        }
    });

    let sel_id = selected_program.read().clone();

    rsx! {
        div { class: "sidebar dash-device-browser",
            div { class: "details-header",
                span { "Programs" }
            }
            div { class: "sidebar-content",
                for prog in programs.read().iter() {
                    {
                        let pid = prog.id.clone();
                        let name = prog.name.clone();
                        let enabled = prog.enabled;
                        let is_selected = sel_id.as_ref() == Some(&pid);
                        let trigger_label = match &prog.trigger {
                            Trigger::Periodic { interval_ms } => format!("{}s", interval_ms / 1000),
                            Trigger::OnChange { node_ids } => format!("{} pts", node_ids.len()),
                        };
                        rsx! {
                            div {
                                key: "{pid}",
                                class: if is_selected { "device-row selected" } else { "device-row" },
                                onclick: {
                                    let pid = pid.clone();
                                    move |_| selected_program.set(Some(pid.clone()))
                                },
                                div { class: "prog-row-content",
                                    span { class: "prog-name",
                                        if !enabled {
                                            span { class: "prog-disabled-badge", "OFF" }
                                        }
                                        "{name}"
                                    }
                                    span { class: "prog-trigger-badge", "{trigger_label}" }
                                }
                            }
                        }
                    }
                }

                if programs.read().is_empty() && !*show_new.read() {
                    div { class: "prog-empty-hint",
                        "No programs yet."
                    }
                }
            }

            // New program form
            div { class: "prog-browser-actions",
                if *show_new.read() {
                    div { class: "prog-new-form",
                        input {
                            class: "prog-input",
                            placeholder: "Program name",
                            value: "{new_name}",
                            oninput: move |e| new_name.set(e.value().clone()),
                        }
                        select {
                            class: "prog-select",
                            value: "{new_trigger}",
                            onchange: move |e| new_trigger.set(e.value().clone()),
                            option { value: "periodic", "Periodic (5s)" }
                            option { value: "onchange", "On Change" }
                        }
                        div { class: "prog-new-buttons",
                            button {
                                class: "prog-btn prog-btn-primary",
                                disabled: new_name.read().trim().is_empty(),
                                onclick: {
                                    let ps = state.program_store.clone();
                                    let audit_state = state.clone();
                                    move |_| {
                                        let name = new_name.read().trim().to_string();
                                        if name.is_empty() { return; }
                                        let id = name.to_lowercase().replace(' ', "-");
                                        let trigger = if *new_trigger.read() == "onchange" {
                                            Trigger::OnChange { node_ids: vec![] }
                                        } else {
                                            Trigger::Periodic { interval_ms: 5000 }
                                        };
                                        let ps = ps.clone();
                                        let audit_state = audit_state.clone();
                                        spawn(async move {
                                            let now = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_millis() as i64;
                                            let prog = Program {
                                                id: id.clone(),
                                                name: name.clone(),
                                                description: String::new(),
                                                enabled: true,
                                                trigger,
                                                blocks: vec![],
                                                wires: vec![],
                                                rhai_override: None,
                                                created_ms: now,
                                                updated_ms: now,
                                            };
                                            match ps.create(prog).await {
                                                Ok(()) => {
                                                    audit_state.audit(
                                                        bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                                            bms_store_storage::store::audit_store::AuditAction::CreateProgram, "program",
                                                        ).resource_id(&id).details(&name),
                                                    );
                                                    status.set(None);
                                                    selected_program.set(Some(id));
                                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                                    show_new.set(false);
                                                    new_name.set(String::new());
                                                }
                                                Err(e) => {
                                                    status.set(Some(format!("{e}")));
                                                }
                                            }
                                        });
                                    }
                                },
                                "Create"
                            }
                            button {
                                class: "prog-btn",
                                onclick: move |_| { show_new.set(false); new_name.set(String::new()); },
                                "Cancel"
                            }
                        }
                        if let Some(ref msg) = *status.read() {
                            div { class: "prog-status-error", "{msg}" }
                        }
                    }
                } else {
                    div { class: "prog-new-buttons-row",
                        button {
                            class: "prog-btn prog-btn-full",
                            onclick: move |_| show_new.set(true),
                            "+ New Program"
                        }
                        button {
                            class: "prog-btn prog-btn-template",
                            onclick: move |_| { let v = *show_templates.read(); show_templates.set(!v); },
                            "From Template"
                        }
                    }
                }
            }

            // Template picker
            if *show_templates.read() {
                div { class: "prog-template-picker",
                    h3 { class: "prog-template-title", "Program Templates" }
                    {TemplateCategory::all().iter().map(|cat| {
                        let cat = *cat;
                        let cat_label = cat.label().to_string();
                        let cat_templates: Vec<_> = templates::catalog()
                            .into_iter()
                            .filter(|t| t.category == cat)
                            .collect();
                        rsx! {
                            div { class: "prog-template-category",
                                h4 { class: "prog-template-cat-label", "{cat_label}" }
                                {cat_templates.iter().map(|t| {
                                    let tid = t.id.to_string();
                                    let tname = t.name.to_string();
                                    let tdesc = t.description.to_string();
                                    let ps = state.program_store.clone();
                                    let audit_state = state.clone();
                                    rsx! {
                                        div {
                                            class: "prog-template-card",
                                            onclick: {
                                                let tid = tid.clone();
                                                let tname = tname.clone();
                                                let ps = ps.clone();
                                                let audit_state = audit_state.clone();
                                                move |_| {
                                                    let tid = tid.clone();
                                                    let tname = tname.clone();
                                                    let ps = ps.clone();
                                                    let audit_state = audit_state.clone();
                                                    spawn(async move {
                                                        if let Some(mut prog) = templates::instantiate(&tid) {
                                                            let now = std::time::SystemTime::now()
                                                                .duration_since(std::time::UNIX_EPOCH)
                                                                .unwrap_or_default()
                                                                .as_millis() as i64;
                                                            let unique_id = format!("{tid}-{}", uuid::Uuid::new_v4());
                                                            prog.id = unique_id.clone();
                                                            prog.created_ms = now;
                                                            prog.updated_ms = now;
                                                            match ps.create(prog).await {
                                                                Ok(()) => {
                                                                    audit_state.audit(
                                                                        bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                                                            bms_store_storage::store::audit_store::AuditAction::CreateProgram, "program",
                                                                        ).resource_id(&unique_id).details(&format!("From template: {tname}")),
                                                                    );
                                                                    status.set(None);
                                                                    selected_program.set(Some(unique_id));
                                                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                                                    show_templates.set(false);
                                                                }
                                                                Err(e) => {
                                                                    status.set(Some(format!("{e}")));
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                            },
                                            div { class: "prog-template-card-name", "{tname}" }
                                            div { class: "prog-template-card-desc", "{tdesc}" }
                                        }
                                    }
                                })}
                            }
                        }
                    })}
                    button {
                        class: "prog-btn",
                        onclick: move |_| show_templates.set(false),
                        "Close"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Blocks tab — wire sheet canvas
// ----------------------------------------------------------------

/// Compute the nearest input port to a canvas point. Returns (block_id, port_name, port_x, port_y, distance).
fn find_nearest_input_port(
    prog: &Program,
    cx: f64,
    cy: f64,
    exclude_block: &str,
) -> Option<(String, String, f64, f64, f64)> {
    let mut best: Option<(String, String, f64, f64, f64)> = None;
    for block in &prog.blocks {
        if block.id == exclude_block {
            continue;
        }
        let (inputs, _) = block_ports(&block.block_type);
        let has_detail = has_detail_text(&block.block_type);
        for (i, port) in inputs.iter().enumerate() {
            let (px, py) = port_pos_with_detail(block.x, block.y, i, true, has_detail);
            let dist = ((cx - px).powi(2) + (cy - py).powi(2)).sqrt();
            if dist < PORT_RADIUS * 5.0 {
                if best.is_none() || dist < best.as_ref().unwrap().4 {
                    best = Some((block.id.clone(), port.name.clone(), px, py, dist));
                }
            }
        }
    }
    best
}

#[component]
fn BlocksTab(
    program_id: ProgramId,
    refresh_counter: Signal<u64>,
    selected_block: Signal<Option<BlockId>>,
) -> Element {
    let state = use_context::<AppState>();
    let mut program = use_signal(|| Option::<Program>::None);

    // Palette drag state: bt_key
    let mut palette_dragging: Signal<Option<String>> = use_signal(|| None);
    // Track mouse start position for palette drag to distinguish click from drag
    let mut palette_mouse_start: Signal<Option<(f64, f64)>> = use_signal(|| None);

    // Canvas state
    let mut vb_x = use_signal(|| 0.0f64);
    let mut vb_y = use_signal(|| 0.0f64);
    let mut vb_w = use_signal(|| 1200.0f64);
    let mut vb_h = use_signal(|| 800.0f64);
    let mut pan_active = use_signal(|| false);
    let mut pan_start_client = use_signal(|| (0.0f64, 0.0f64));
    let mut pan_start_vb = use_signal(|| (0.0f64, 0.0f64));
    let svg_rect = use_signal(|| (0.0f64, 0.0f64, 1.0f64, 1.0f64));
    let mut drag = use_signal(|| WireDrag::None);
    let mut selection = use_signal(|| WireSelection::None);
    // Nearest input port highlight during wire draw: (block_id, port_name, px, py)
    let mut hover_port = use_signal(|| Option::<(String, String, f64, f64)>::None);

    // Load program
    let prog_store = state.program_store.clone();
    let pid = program_id.clone();
    let _ = use_resource(move || {
        let ps = prog_store.clone();
        let pid = pid.clone();
        let _r = *refresh_counter.read();
        async move {
            if let Ok(p) = ps.get(&pid).await {
                program.set(Some(p));
            }
        }
    });

    // Measure SVG after render
    use_effect(move || {
        measure_wire_svg(svg_rect);
    });

    // Sync selected_block from selection
    {
        let sel = selection.read().clone();
        let current_sb = selected_block.read().clone();
        let new_sb = match &sel {
            WireSelection::Block(bid) => Some(bid.clone()),
            _ => None,
        };
        if new_sb != current_sb {
            selected_block.set(new_sb);
        }
    }

    let Some(prog) = program.read().clone() else {
        return rsx! { div { class: "prog-loading", "Loading..." } };
    };

    // Coordinate conversion helpers
    let to_canvas = move |elem_x: f64, elem_y: f64| -> (f64, f64) {
        let (_, _, rw, rh) = *svg_rect.read();
        wire_elem_to_canvas(
            elem_x,
            elem_y,
            rw,
            rh,
            *vb_x.read(),
            *vb_y.read(),
            *vb_w.read(),
            *vb_h.read(),
        )
    };

    // Convert container-relative coords to SVG canvas coords
    // Container includes palette (160px wide) on the left
    let container_to_canvas = move |container_x: f64, container_y: f64| -> (f64, f64) {
        let svg_elem_x = container_x - PALETTE_WIDTH;
        let svg_elem_y = container_y;
        let (_, _, rw, rh) = *svg_rect.read();
        wire_elem_to_canvas(
            svg_elem_x,
            svg_elem_y,
            rw,
            rh,
            *vb_x.read(),
            *vb_y.read(),
            *vb_w.read(),
            *vb_h.read(),
        )
    };

    let css_scale = move || -> f64 {
        let (_, _, rw, rh) = *svg_rect.read();
        let vw = *vb_w.read();
        let vh = *vb_h.read();
        if rw <= 0.0 || rh <= 0.0 {
            return 1.0;
        }
        (rw / vw).min(rh / vh)
    };

    // Build block lookup
    let block_map: HashMap<String, Block> = prog
        .blocks
        .iter()
        .map(|b| (b.id.clone(), b.clone()))
        .collect();

    // ── SVG Mouse handlers ──

    let onmousedown = {
        move |e: Event<MouseData>| {
            // Middle button → pan
            if e.data().trigger_button()
                == Some(dioxus_elements::input_data::MouseButton::Auxiliary)
            {
                let client = e.data().client_coordinates();
                pan_active.set(true);
                pan_start_client.set((client.x, client.y));
                pan_start_vb.set((*vb_x.peek(), *vb_y.peek()));
                return;
            }

            let elem = e.data().element_coordinates();
            let (cx, cy) = to_canvas(elem.x, elem.y);

            let Some(ref live_prog) = *program.peek() else {
                return;
            };
            let live_map: HashMap<String, Block> = live_prog
                .blocks
                .iter()
                .map(|b| (b.id.clone(), b.clone()))
                .collect();

            // Hit test blocks (reverse order for z-order)
            for block in live_prog.blocks.iter().rev() {
                let (inputs, outputs) = block_ports(&block.block_type);
                let has_detail = has_detail_text(&block.block_type);
                let bh = calc_block_height(inputs.len(), outputs.len());

                // Check output ports first — larger hit area for wire starting
                for (i, port) in outputs.iter().enumerate() {
                    let (px, py) = port_pos_with_detail(block.x, block.y, i, false, has_detail);
                    let dist = ((cx - px).powi(2) + (cy - py).powi(2)).sqrt();
                    if dist < PORT_RADIUS * 4.0 {
                        drag.set(WireDrag::DrawWire(
                            block.id.clone(),
                            port.name.clone(),
                            cx,
                            cy,
                        ));
                        return;
                    }
                }

                // Check input ports — also allow starting wire from input (for disconnecting)
                for (i, _port) in inputs.iter().enumerate() {
                    let (px, py) = port_pos_with_detail(block.x, block.y, i, true, has_detail);
                    let dist = ((cx - px).powi(2) + (cy - py).powi(2)).sqrt();
                    if dist < PORT_RADIUS * 4.0 {
                        // Select the block, don't start wire from input
                        selection.set(WireSelection::Block(block.id.clone()));
                        return;
                    }
                }

                // Check block body
                if cx >= block.x && cx <= block.x + BLOCK_W && cy >= block.y && cy <= block.y + bh {
                    drag.set(WireDrag::MoveBlock(
                        block.id.clone(),
                        cx - block.x,
                        cy - block.y,
                    ));
                    selection.set(WireSelection::Block(block.id.clone()));
                    return;
                }
            }

            // Hit test wires
            for (wi, wire) in live_prog.wires.iter().enumerate() {
                if let (Some(fb), Some(tb)) =
                    (live_map.get(&wire.from_block), live_map.get(&wire.to_block))
                {
                    let fb_detail = has_detail_text(&fb.block_type);
                    let tb_detail = has_detail_text(&tb.block_type);
                    let (_, from_outputs) = block_ports(&fb.block_type);
                    let from_idx = from_outputs
                        .iter()
                        .position(|p| p.name == wire.from_port)
                        .unwrap_or(0);
                    let (to_inputs, _) = block_ports(&tb.block_type);
                    let to_idx = to_inputs
                        .iter()
                        .position(|p| p.name == wire.to_port)
                        .unwrap_or(0);
                    let (sx, sy) = port_pos_with_detail(fb.x, fb.y, from_idx, false, fb_detail);
                    let (tx, ty) = port_pos_with_detail(tb.x, tb.y, to_idx, true, tb_detail);
                    if point_near_bezier(cx, cy, sx, sy, tx, ty, 8.0) {
                        selection.set(WireSelection::Wire(wi));
                        return;
                    }
                }
            }

            selection.set(WireSelection::None);
        }
    };

    let onmousemove_svg = {
        move |e: Event<MouseData>| {
            if *pan_active.peek() {
                let client = e.data().client_coordinates();
                let (sc_x, sc_y) = *pan_start_client.peek();
                let (sv_x, sv_y) = *pan_start_vb.peek();
                let scale = css_scale();
                vb_x.set(sv_x - (client.x - sc_x) / scale);
                vb_y.set(sv_y - (client.y - sc_y) / scale);
                return;
            }

            let current_drag = drag.peek().clone();
            match current_drag {
                WireDrag::MoveBlock(ref bid, ox, oy) => {
                    let elem = e.data().element_coordinates();
                    let (cx, cy) = to_canvas(elem.x, elem.y);
                    let nx = grid_snap(cx - ox);
                    let ny = grid_snap(cy - oy);
                    let snap = { program.peek().clone() };
                    if let Some(mut p) = snap {
                        if let Some(b) = p.blocks.iter_mut().find(|b| b.id == *bid) {
                            b.x = nx.max(0.0);
                            b.y = ny.max(0.0);
                        }
                        program.set(Some(p));
                    }
                }
                WireDrag::DrawWire(ref fb, ref fp, _, _) => {
                    let elem = e.data().element_coordinates();
                    let (cx, cy) = to_canvas(elem.x, elem.y);
                    drag.set(WireDrag::DrawWire(fb.clone(), fp.clone(), cx, cy));

                    // Find nearest input port for snap highlight
                    if let Some(ref live_prog) = *program.peek() {
                        if let Some((bid, pname, px, py, _dist)) =
                            find_nearest_input_port(live_prog, cx, cy, fb)
                        {
                            hover_port.set(Some((bid, pname, px, py)));
                        } else {
                            hover_port.set(None);
                        }
                    }
                }
                WireDrag::PaletteDrop(ref key, _, _) => {
                    let elem = e.data().element_coordinates();
                    let (cx, cy) = to_canvas(elem.x, elem.y);
                    drag.set(WireDrag::PaletteDrop(key.clone(), cx, cy));
                }
                WireDrag::None => {}
            }
        }
    };

    let onmouseup_svg = {
        let ps_up = state.program_store.clone();
        move |e: Event<MouseData>| {
            pan_active.set(false);
            hover_port.set(None);

            let current_drag = drag.peek().clone();
            match current_drag {
                WireDrag::MoveBlock(_, _, _) => {
                    if let Some(p) = program.peek().clone() {
                        let ps = ps_up.clone();
                        spawn(async move {
                            let _ = ps.update(p).await;
                        });
                    }
                }
                WireDrag::DrawWire(ref from_block, ref from_port, _, _) => {
                    let elem = e.data().element_coordinates();
                    let (cx, cy) = to_canvas(elem.x, elem.y);

                    // Snap to nearest input port
                    let live_snap = { program.peek().clone() };
                    if let Some(ref live_prog) = live_snap {
                        if let Some((bid, pname, _, _, _)) =
                            find_nearest_input_port(live_prog, cx, cy, from_block)
                        {
                            // Check we don't already have a wire to this port
                            let already_wired = live_prog
                                .wires
                                .iter()
                                .any(|w| w.to_block == bid && w.to_port == pname);
                            if !already_wired {
                                let wire = Wire {
                                    from_block: from_block.clone(),
                                    from_port: from_port.clone(),
                                    to_block: bid,
                                    to_port: pname,
                                };
                                let ps = ps_up.clone();
                                let mut p = live_prog.clone();
                                spawn(async move {
                                    p.wires.push(wire);
                                    let _ = ps.update(p).await;
                                    {
                                        let v = *refresh_counter.peek();
                                        refresh_counter.set(v + 1);
                                    }
                                });
                            }
                        }
                    }
                }
                WireDrag::PaletteDrop(ref bt_key, cx, cy) => {
                    // Drop from palette onto SVG — always create block immediately
                    let live_snap = { program.peek().clone() };
                    if let Some(ref p) = live_snap {
                        add_block_to_program(
                            bt_key,
                            "",
                            p,
                            &ps_up,
                            *vb_x.peek(),
                            *vb_y.peek(),
                            *vb_w.peek(),
                            *vb_h.peek(),
                            Some((cx, cy)),
                            refresh_counter,
                        );
                    }
                    palette_dragging.set(None);
                    palette_mouse_start.set(None);
                }
                WireDrag::None => {}
            }
            drag.set(WireDrag::None);
        }
    };

    // ── Container mouse handlers (for palette drag across boundary) ──

    let container_onmousemove = {
        move |e: Event<MouseData>| {
            // Only handle if we're doing a palette drag
            let pal_drag = palette_dragging.peek().clone();
            let Some(bt_key) = pal_drag else {
                return;
            };

            let elem = e.data().element_coordinates();
            // Check if we've moved enough to start dragging
            if let Some((start_x, start_y)) = *palette_mouse_start.peek() {
                let dx = (elem.x - start_x).abs();
                let dy = (elem.y - start_y).abs();
                if dx < 5.0 && dy < 5.0 {
                    return; // Not enough movement yet
                }
            }
            // Mouse is in container coords, convert to canvas
            let (cx, cy) = container_to_canvas(elem.x, elem.y);
            drag.set(WireDrag::PaletteDrop(bt_key, cx, cy));
        }
    };

    let container_onmouseup = {
        let ps_cup = state.program_store.clone();
        move |e: Event<MouseData>| {
            // Clone the palette drag data before any mutable borrows
            let pal_drag = palette_dragging.peek().clone();
            let Some(bt_key) = pal_drag else {
                return;
            };

            let elem = e.data().element_coordinates();

            // Check if this was a click (not enough movement)
            let was_click = if let Some((start_x, start_y)) = *palette_mouse_start.peek() {
                let dx = (elem.x - start_x).abs();
                let dy = (elem.y - start_y).abs();
                dx < 5.0 && dy < 5.0
            } else {
                true
            };

            if was_click {
                // Click-to-add at center of view
                let live_snap = { program.peek().clone() };
                if let Some(ref p) = live_snap {
                    add_block_to_program(
                        &bt_key,
                        "",
                        p,
                        &ps_cup,
                        *vb_x.peek(),
                        *vb_y.peek(),
                        *vb_w.peek(),
                        *vb_h.peek(),
                        None,
                        refresh_counter,
                    );
                }
            } else if elem.x > PALETTE_WIDTH {
                // Dropped over SVG area
                let (cx, cy) = container_to_canvas(elem.x, elem.y);
                let live_snap = { program.peek().clone() };
                if let Some(ref p) = live_snap {
                    add_block_to_program(
                        &bt_key,
                        "",
                        p,
                        &ps_cup,
                        *vb_x.peek(),
                        *vb_y.peek(),
                        *vb_w.peek(),
                        *vb_h.peek(),
                        Some((cx, cy)),
                        refresh_counter,
                    );
                }
            }
            // else dropped back on palette — ignore

            palette_dragging.set(None);
            palette_mouse_start.set(None);
            drag.set(WireDrag::None);
        }
    };

    let onwheel = move |e: Event<WheelData>| {
        e.prevent_default();
        let delta_y = e.data().delta().strip_units().y;
        let factor = if delta_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
        let elem = e.data().element_coordinates();
        let (mx, my) = to_canvas(elem.x, elem.y);
        let old_w = *vb_w.peek();
        let old_h = *vb_h.peek();
        let old_x = *vb_x.peek();
        let old_y = *vb_y.peek();
        vb_w.set(old_w * factor);
        vb_h.set(old_h * factor);
        vb_x.set(mx - (mx - old_x) * factor);
        vb_y.set(my - (my - old_y) * factor);
    };

    let onkeydown = {
        let ps_key = state.program_store.clone();
        move |e: Event<KeyboardData>| {
            let key = e.data().key();
            if key == Key::Delete || key == Key::Backspace {
                let sel = selection.peek().clone();
                let live_snap = { program.peek().clone() };
                let Some(live_prog) = live_snap else {
                    return;
                };
                match sel {
                    WireSelection::Block(ref bid) => {
                        let ps = ps_key.clone();
                        let mut p = live_prog;
                        let bid = bid.clone();
                        spawn(async move {
                            p.blocks.retain(|b| b.id != bid);
                            p.wires.retain(|w| w.from_block != bid && w.to_block != bid);
                            let _ = ps.update(p).await;
                            {
                                let v = *refresh_counter.peek();
                                refresh_counter.set(v + 1);
                            }
                        });
                        selection.set(WireSelection::None);
                    }
                    WireSelection::Wire(idx) => {
                        let ps = ps_key.clone();
                        let mut p = live_prog;
                        spawn(async move {
                            if idx < p.wires.len() {
                                p.wires.remove(idx);
                                let _ = ps.update(p).await;
                                {
                                    let v = *refresh_counter.peek();
                                    refresh_counter.set(v + 1);
                                }
                            }
                        });
                        selection.set(WireSelection::None);
                    }
                    WireSelection::None => {}
                }
            }
        }
    };

    // ViewBox string
    let vx = *vb_x.read();
    let vy = *vb_y.read();
    let vw = *vb_w.read();
    let vh = *vb_h.read();
    let view_box = format!("{vx} {vy} {vw} {vh}");

    let sel = selection.read().clone();
    let current_drag = drag.read().clone();
    let hover = hover_port.read().clone();

    // Pre-compute wire paths using consistent port positions
    let wire_paths: Vec<(String, String, bool)> = prog
        .wires
        .iter()
        .enumerate()
        .map(|(wi, wire)| {
            let is_selected = sel == WireSelection::Wire(wi);
            let label = format!("{} -> {}.{}", wire.from_port, wire.to_block, wire.to_port);
            let path = if let (Some(fb), Some(tb)) = (
                block_map.get(&wire.from_block),
                block_map.get(&wire.to_block),
            ) {
                let fb_detail = has_detail_text(&fb.block_type);
                let tb_detail = has_detail_text(&tb.block_type);
                let (_, from_outputs) = block_ports(&fb.block_type);
                let from_idx = from_outputs
                    .iter()
                    .position(|p| p.name == wire.from_port)
                    .unwrap_or(0);
                let (to_inputs, _) = block_ports(&tb.block_type);
                let to_idx = to_inputs
                    .iter()
                    .position(|p| p.name == wire.to_port)
                    .unwrap_or(0);
                let (sx, sy) = port_pos_with_detail(fb.x, fb.y, from_idx, false, fb_detail);
                let (tx, ty) = port_pos_with_detail(tb.x, tb.y, to_idx, true, tb_detail);
                wire_bezier_path(sx, sy, tx, ty)
            } else {
                String::new()
            };
            (path, label, is_selected)
        })
        .collect();

    // In-progress wire — snap to highlighted port if near one
    let (draw_wire_path, draw_wire_snap) =
        if let WireDrag::DrawWire(ref fb, ref fp, mx, my) = current_drag {
            if let Some(from_block) = block_map.get(fb) {
                let fb_detail = has_detail_text(&from_block.block_type);
                let (_, from_outputs) = block_ports(&from_block.block_type);
                let from_idx = from_outputs.iter().position(|p| p.name == *fp).unwrap_or(0);
                let (sx, sy) =
                    port_pos_with_detail(from_block.x, from_block.y, from_idx, false, fb_detail);
                // If hovering near a port, snap the wire end to it
                let (end_x, end_y) = if let Some((_, _, px, py)) = &hover {
                    (*px, *py)
                } else {
                    (mx, my)
                };
                (
                    Some(wire_bezier_path(sx, sy, end_x, end_y)),
                    hover.is_some(),
                )
            } else {
                (None, false)
            }
        } else {
            (None, false)
        };

    // Ghost block preview for palette drag
    let palette_ghost = if let WireDrag::PaletteDrop(ref bt_key, gx, gy) = current_drag {
        Some((bt_key.clone(), gx, gy))
    } else {
        None
    };

    rsx! {
        div {
            class: "wire-sheet-container",
            onmousemove: container_onmousemove,
            onmouseup: container_onmouseup,

            // Block palette
            div { class: "wire-sheet-palette",
                div { class: "wire-sheet-palette-header", "I/O" }
                PaletteItem { label: "Point Read", bt_key: "point_read", color: "#3b82f6", palette_dragging, palette_mouse_start }
                PaletteItem { label: "Point Write", bt_key: "point_write", color: "#3b82f6", palette_dragging, palette_mouse_start }
                PaletteItem { label: "Virtual Pt", bt_key: "virtual_point", color: "#3b82f6", palette_dragging, palette_mouse_start }

                div { class: "wire-sheet-palette-header", "Constants" }
                PaletteItem { label: "Float", bt_key: "constant_float", color: "#64748b", palette_dragging, palette_mouse_start }
                PaletteItem { label: "Bool", bt_key: "constant_bool", color: "#64748b", palette_dragging, palette_mouse_start }
                PaletteItem { label: "Integer", bt_key: "constant_int", color: "#64748b", palette_dragging, palette_mouse_start }

                div { class: "wire-sheet-palette-header", "Math" }
                for (key, label) in [("math_add","Add"),("math_sub","Sub"),("math_mul","Mul"),("math_div","Div"),("math_min","Min"),("math_max","Max"),("math_abs","Abs"),("math_clamp","Clamp")] {
                    PaletteItem { label: label, bt_key: key, color: "#8b5cf6", palette_dragging, palette_mouse_start }
                }

                div { class: "wire-sheet-palette-header", "Compare" }
                for (key, label) in [("compare_gt",">"),("compare_lt","<"),("compare_gte",">="),("compare_lte","<="),("compare_eq","=="),("compare_neq","!=")] {
                    PaletteItem { label: label, bt_key: key, color: "#10b981", palette_dragging, palette_mouse_start }
                }

                div { class: "wire-sheet-palette-header", "Logic" }
                for (key, label) in [("logic_and","AND"),("logic_or","OR"),("logic_not","NOT"),("logic_xor","XOR")] {
                    PaletteItem { label: label, bt_key: key, color: "#f59e0b", palette_dragging, palette_mouse_start }
                }

                div { class: "wire-sheet-palette-header", "Flow" }
                PaletteItem { label: "Select", bt_key: "select", color: "#6366f1", palette_dragging, palette_mouse_start }
                PaletteItem { label: "PID", bt_key: "pid", color: "#ec4899", palette_dragging, palette_mouse_start }

                div { class: "wire-sheet-palette-header", "Timing" }
                for (key, label) in [("timing_delay_on","Delay On"),("timing_delay_off","Delay Off"),("timing_moving_avg","Moving Avg"),("timing_rate_change","Rate/Chg")] {
                    PaletteItem { label: label, bt_key: key, color: "#6366f1", palette_dragging, palette_mouse_start }
                }

                div { class: "wire-sheet-palette-header", "State" }
                PaletteItem { label: "Latch", bt_key: "latch", color: "#f59e0b", palette_dragging, palette_mouse_start }
                PaletteItem { label: "One-Shot", bt_key: "one_shot", color: "#f59e0b", palette_dragging, palette_mouse_start }

                div { class: "wire-sheet-palette-header", "Transform" }
                PaletteItem { label: "Scale", bt_key: "scale", color: "#8b5cf6", palette_dragging, palette_mouse_start }
                PaletteItem { label: "Ramp Limit", bt_key: "ramp_limit", color: "#8b5cf6", palette_dragging, palette_mouse_start }

                div { class: "wire-sheet-palette-header", "Output" }
                PaletteItem { label: "Log", bt_key: "log", color: "#ef4444", palette_dragging, palette_mouse_start }
                PaletteItem { label: "Alarm", bt_key: "alarm_trigger", color: "#ef4444", palette_dragging, palette_mouse_start }

                div { class: "wire-sheet-palette-header", "Script" }
                PaletteItem { label: "Custom", bt_key: "custom_script", color: "#78716c", palette_dragging, palette_mouse_start }

            }

            // SVG Canvas
            svg {
                id: "wire-svg",
                view_box: "{view_box}",
                "preserveAspectRatio": "xMidYMid meet",
                tabindex: "0",
                onmousedown: onmousedown,
                onmousemove: onmousemove_svg,
                onmouseup: onmouseup_svg,
                onwheel: onwheel,
                onkeydown: onkeydown,
                oncontextmenu: move |e: Event<MouseData>| { e.prevent_default(); },

                // Grid dots
                defs {
                    pattern {
                        id: "wire-grid",
                        width: "{GRID_SIZE}",
                        height: "{GRID_SIZE}",
                        "patternUnits": "userSpaceOnUse",
                        circle {
                            cx: "{GRID_SIZE}",
                            cy: "{GRID_SIZE}",
                            r: "0.8",
                            fill: "#2a2a35",
                        }
                    }
                }
                rect {
                    x: "0", y: "0",
                    width: "{CANVAS_W}", height: "{CANVAS_H}",
                    fill: "url(#wire-grid)",
                }

                // Wires
                for (path, _label, is_selected) in wire_paths.iter() {
                    {
                        if path.is_empty() {
                            rsx! {}
                        } else {
                            let stroke = if *is_selected { "#60a5fa" } else { "#64748b" };
                            let width = if *is_selected { "3" } else { "2" };
                            rsx! {
                                path {
                                    d: "{path}",
                                    fill: "none",
                                    stroke: "{stroke}",
                                    stroke_width: "{width}",
                                }
                            }
                        }
                    }
                }

                // In-progress wire
                if let Some(ref dpath) = draw_wire_path {
                    path {
                        d: "{dpath}",
                        fill: "none",
                        stroke: if draw_wire_snap { "#60a5fa" } else { "#3b82f6" },
                        stroke_width: if draw_wire_snap { "3" } else { "2" },
                        stroke_dasharray: if draw_wire_snap { "" } else { "6 4" },
                        opacity: if draw_wire_snap { "1" } else { "0.7" },
                    }
                }

                // Port hover highlight ring
                if let Some((_, _, hx, hy)) = hover {
                    circle {
                        cx: "{hx}",
                        cy: "{hy}",
                        r: "{PORT_RADIUS * 2.5}",
                        fill: "none",
                        stroke: "#60a5fa",
                        stroke_width: "2",
                        opacity: "0.8",
                    }
                }

                // Ghost block preview for palette drag
                if let Some((ref ghost_key, gx, gy)) = palette_ghost {
                    {
                        let ghost_color = palette_key_color(ghost_key);
                        let ghost_label = palette_key_label(ghost_key);
                        rsx! {
                            g {
                                transform: "translate({gx}, {gy})",
                                opacity: "0.5",
                                rect {
                                    width: "{BLOCK_W}",
                                    height: "60",
                                    rx: "6",
                                    fill: "#1a1a24",
                                    stroke: "#60a5fa",
                                    stroke_width: "2",
                                    stroke_dasharray: "4 3",
                                }
                                rect {
                                    width: "{BLOCK_W}",
                                    height: "{TITLE_HEIGHT}",
                                    rx: "6",
                                    fill: "{ghost_color}",
                                }
                                rect {
                                    y: "{TITLE_HEIGHT - 6.0}",
                                    width: "{BLOCK_W}",
                                    height: "6",
                                    fill: "{ghost_color}",
                                }
                                text {
                                    x: "{BLOCK_W / 2.0}",
                                    y: "{TITLE_HEIGHT / 2.0 + 1.0}",
                                    "text-anchor": "middle",
                                    "dominant-baseline": "central",
                                    fill: "#fff",
                                    "font-size": "11",
                                    "font-weight": "600",
                                    "font-family": "system-ui, sans-serif",
                                    "{ghost_label}"
                                }
                            }
                        }
                    }
                }

                // Blocks
                for block in prog.blocks.iter() {
                    {
                        let bid = block.id.clone();
                        let (inputs, outputs) = block_ports(&block.block_type);
                        let has_detail = has_detail_text(&block.block_type);
                        let bh = calc_block_height(inputs.len(), outputs.len());
                        let label = block_type_label(&block.block_type);
                        let detail = block_type_detail(&block.block_type);
                        let detail_short: String = detail.chars().take(20).collect();
                        let color = block_category_color(&block.block_type);
                        let is_selected = sel == WireSelection::Block(bid.clone());
                        let transform = format!("translate({}, {})", block.x, block.y);
                        let opacity = if block.enabled { "1" } else { "0.35" };

                        rsx! {
                            g {
                                key: "{bid}",
                                transform: "{transform}",
                                opacity: "{opacity}",

                                // Block body
                                rect {
                                    width: "{BLOCK_W}",
                                    height: "{bh}",
                                    rx: "6",
                                    fill: "#1a1a24",
                                    stroke: if is_selected { "#60a5fa" } else { "#333" },
                                    stroke_width: if is_selected { "2" } else { "1" },
                                }

                                // Title bar
                                rect {
                                    width: "{BLOCK_W}",
                                    height: "{TITLE_HEIGHT}",
                                    rx: "6",
                                    fill: "{color}",
                                }
                                rect {
                                    y: "{TITLE_HEIGHT - 6.0}",
                                    width: "{BLOCK_W}",
                                    height: "6",
                                    fill: "{color}",
                                }
                                text {
                                    x: "{BLOCK_W / 2.0}",
                                    y: "{TITLE_HEIGHT / 2.0 + 1.0}",
                                    "text-anchor": "middle",
                                    "dominant-baseline": "central",
                                    fill: "#fff",
                                    "font-size": "11",
                                    "font-weight": "600",
                                    "font-family": "system-ui, sans-serif",
                                    "{label}"
                                }

                                // Detail text below title
                                if has_detail {
                                    text {
                                        x: "{BLOCK_W / 2.0}",
                                        y: "{TITLE_HEIGHT + 12.0}",
                                        "text-anchor": "middle",
                                        "dominant-baseline": "central",
                                        fill: "#888",
                                        "font-size": "9",
                                        "font-family": "'SF Mono', monospace",
                                        "{detail_short}"
                                    }
                                }

                                // Input ports (left side)
                                for (i, port) in inputs.iter().enumerate() {
                                    {
                                        let py = port_y_local(i, has_detail);
                                        let pname = port.name.clone();
                                        // Check if this port is the hover target
                                        let is_hover = hover.as_ref().map_or(false, |(hb, hp, _, _)| *hb == bid && *hp == pname);
                                        rsx! {
                                            // Larger invisible hit area
                                            circle {
                                                cx: "0",
                                                cy: "{py}",
                                                r: "{PORT_RADIUS * 3.0}",
                                                fill: "transparent",
                                            }
                                            circle {
                                                cx: "0",
                                                cy: "{py}",
                                                r: if is_hover { "{PORT_RADIUS * 1.6}" } else { "{PORT_RADIUS}" },
                                                fill: if is_hover { "#60a5fa" } else { "#1a1a24" },
                                                stroke: if is_hover { "#93c5fd" } else { "#64748b" },
                                                stroke_width: "1.5",
                                            }
                                            text {
                                                x: "{PORT_RADIUS + 6.0}",
                                                y: "{py}",
                                                "dominant-baseline": "central",
                                                fill: "#999",
                                                "font-size": "9",
                                                "font-family": "system-ui, sans-serif",
                                                "{pname}"
                                            }
                                        }
                                    }
                                }

                                // Output ports (right side)
                                for (i, port) in outputs.iter().enumerate() {
                                    {
                                        let py = port_y_local(i, has_detail);
                                        let pname = port.name.clone();
                                        rsx! {
                                            circle {
                                                cx: "{BLOCK_W}",
                                                cy: "{py}",
                                                r: "{PORT_RADIUS * 3.0}",
                                                fill: "transparent",
                                            }
                                            circle {
                                                cx: "{BLOCK_W}",
                                                cy: "{py}",
                                                r: "{PORT_RADIUS}",
                                                fill: "{color}",
                                                stroke: "{color}",
                                                stroke_width: "1.5",
                                            }
                                            text {
                                                x: "{BLOCK_W - PORT_RADIUS - 6.0}",
                                                y: "{py}",
                                                "text-anchor": "end",
                                                "dominant-baseline": "central",
                                                fill: "#999",
                                                "font-size": "9",
                                                "font-family": "system-ui, sans-serif",
                                                "{pname}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Palette item — mousedown to start drag (or click if no movement)
// ----------------------------------------------------------------

#[component]
fn PaletteItem(
    label: &'static str,
    bt_key: &'static str,
    color: &'static str,
    palette_dragging: Signal<Option<String>>,
    palette_mouse_start: Signal<Option<(f64, f64)>>,
) -> Element {
    rsx! {
        div {
            class: "wire-sheet-palette-item",
            onmousedown: {
                let bt_key = bt_key.to_string();
                move |e: Event<MouseData>| {
                    let elem = e.data().element_coordinates();
                    palette_mouse_start.set(Some((elem.x, elem.y)));
                    palette_dragging.set(Some(bt_key.clone()));
                }
            },
            div {
                class: "wire-sheet-palette-dot",
                style: "background: {color};",
            }
            "{label}"
        }
    }
}

// ----------------------------------------------------------------
// Wire sheet geometry helpers
// ----------------------------------------------------------------

const DETAIL_OFFSET: f64 = 16.0; // extra space when block has detail text

fn calc_block_height(num_inputs: usize, num_outputs: usize) -> f64 {
    let port_count = num_inputs.max(num_outputs).max(1);
    TITLE_HEIGHT + DETAIL_OFFSET + PORT_SPACING * port_count as f64 + 8.0
}

/// Local Y position of a port within the block's g transform.
fn port_y_local(port_idx: usize, has_detail: bool) -> f64 {
    let detail_pad = if has_detail { DETAIL_OFFSET } else { 4.0 };
    TITLE_HEIGHT + detail_pad + PORT_SPACING * (port_idx as f64 + 0.5)
}

/// Whether a block type has detail text below the title.
fn has_detail_text(bt: &BlockType) -> bool {
    !block_type_detail(bt).is_empty()
}

/// Absolute canvas position of a port. `is_input` = true for left side, false for right.
fn port_pos_with_detail(
    bx: f64,
    by: f64,
    port_idx: usize,
    is_input: bool,
    has_detail: bool,
) -> (f64, f64) {
    let py = by + port_y_local(port_idx, has_detail);
    if is_input {
        (bx, py)
    } else {
        (bx + BLOCK_W, py)
    }
}

fn grid_snap(v: f64) -> f64 {
    (v / GRID_SIZE).round() * GRID_SIZE
}

fn wire_bezier_path(sx: f64, sy: f64, tx: f64, ty: f64) -> String {
    let dx = (tx - sx).abs().max(60.0) * 0.5;
    format!("M {sx},{sy} C {},{sy} {},{ty} {tx},{ty}", sx + dx, tx - dx)
}

fn block_category_color(bt: &BlockType) -> &'static str {
    match bt {
        BlockType::PointRead { .. }
        | BlockType::PointWrite { .. }
        | BlockType::VirtualPoint { .. } => "#3b82f6",
        BlockType::Constant { .. } => "#64748b",
        BlockType::Math { .. } => "#8b5cf6",
        BlockType::Logic { .. } => "#f59e0b",
        BlockType::Compare { .. } => "#10b981",
        BlockType::Select | BlockType::Timing { .. } => "#6366f1",
        BlockType::Pid { .. } => "#ec4899",
        BlockType::AlarmTrigger { .. } | BlockType::Log { .. } => "#ef4444",
        BlockType::CustomScript { .. } => "#78716c",
        BlockType::Latch | BlockType::OneShot => "#f59e0b",
        BlockType::Scale { .. } | BlockType::RampLimit { .. } => "#8b5cf6",
    }
}

/// Color for a palette key (for ghost block preview)
fn palette_key_color(key: &str) -> &'static str {
    match key {
        "point_read" | "point_write" | "virtual_point" => "#3b82f6",
        "constant_float" | "constant_bool" | "constant_int" => "#64748b",
        k if k.starts_with("math_") => "#8b5cf6",
        k if k.starts_with("compare_") => "#10b981",
        k if k.starts_with("logic_") => "#f59e0b",
        "select" | "pid" => "#6366f1",
        k if k.starts_with("timing_") => "#6366f1",
        "latch" | "one_shot" => "#f59e0b",
        "scale" | "ramp_limit" => "#8b5cf6",
        "log" | "alarm_trigger" => "#ef4444",
        "custom_script" => "#78716c",
        _ => "#64748b",
    }
}

/// Label for a palette key (for ghost block preview)
fn palette_key_label(key: &str) -> &'static str {
    match key {
        "point_read" => "Read",
        "point_write" => "Write",
        "virtual_point" => "Virtual",
        "constant_float" => "Float",
        "constant_bool" => "Bool",
        "constant_int" => "Integer",
        "math_add" => "Add",
        "math_sub" => "Sub",
        "math_mul" => "Mul",
        "math_div" => "Div",
        "math_min" => "Min",
        "math_max" => "Max",
        "math_abs" => "Abs",
        "math_clamp" => "Clamp",
        "compare_gt" => ">",
        "compare_lt" => "<",
        "compare_gte" => ">=",
        "compare_lte" => "<=",
        "compare_eq" => "==",
        "compare_neq" => "!=",
        "logic_and" => "AND",
        "logic_or" => "OR",
        "logic_not" => "NOT",
        "logic_xor" => "XOR",
        "select" => "Select",
        "pid" => "PID",
        "timing_delay_on" => "Delay On",
        "timing_delay_off" => "Delay Off",
        "timing_moving_avg" => "Moving Avg",
        "timing_rate_change" => "Rate/Chg",
        "latch" => "Latch",
        "one_shot" => "One-Shot",
        "scale" => "Scale",
        "ramp_limit" => "Ramp Limit",
        "log" => "Log",
        "alarm_trigger" => "Alarm",
        "custom_script" => "Script",
        _ => "Block",
    }
}

/// Approximate hit test: is point (px,py) near the cubic bezier from (sx,sy) to (tx,ty)?
fn point_near_bezier(px: f64, py: f64, sx: f64, sy: f64, tx: f64, ty: f64, threshold: f64) -> bool {
    let dx = (tx - sx).abs().max(60.0) * 0.5;
    let c1x = sx + dx;
    let c2x = tx - dx;
    // Sample 10 points along the curve
    for i in 0..=10 {
        let t = i as f64 / 10.0;
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;
        let bx = mt3 * sx + 3.0 * mt2 * t * c1x + 3.0 * mt * t2 * c2x + t3 * tx;
        let by = mt3 * sy + 3.0 * mt2 * t * sy + 3.0 * mt * t2 * ty + t3 * ty;
        let dist = ((px - bx).powi(2) + (py - by).powi(2)).sqrt();
        if dist < threshold {
            return true;
        }
    }
    false
}

fn wire_elem_to_canvas(
    elem_x: f64,
    elem_y: f64,
    svg_w: f64,
    svg_h: f64,
    vb_x: f64,
    vb_y: f64,
    vb_w: f64,
    vb_h: f64,
) -> (f64, f64) {
    if svg_w <= 0.0 || svg_h <= 0.0 {
        return (elem_x, elem_y);
    }
    let scale = (svg_w / vb_w).min(svg_h / vb_h);
    let rendered_w = vb_w * scale;
    let rendered_h = vb_h * scale;
    let offset_x = (svg_w - rendered_w) / 2.0;
    let offset_y = (svg_h - rendered_h) / 2.0;
    (
        vb_x + (elem_x - offset_x) / scale,
        vb_y + (elem_y - offset_y) / scale,
    )
}

fn measure_wire_svg(mut svg_rect: Signal<(f64, f64, f64, f64)>) {
    spawn(async move {
        // Small delay so DOM has rendered
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut eval = document::eval(
            r#"var svg = document.getElementById('wire-svg');
            if (svg) {
                var r = svg.getBoundingClientRect();
                dioxus.send([r.left, r.top, r.width, r.height]);
            } else {
                dioxus.send([0, 0, 0, 0]);
            }"#,
        );
        if let Ok(val) = eval.recv::<serde_json::Value>().await {
            if let Some(arr) = val.as_array() {
                let left = arr[0].as_f64().unwrap_or(0.0);
                let top = arr[1].as_f64().unwrap_or(0.0);
                let width = arr[2].as_f64().unwrap_or(1.0);
                let height = arr[3].as_f64().unwrap_or(1.0);
                svg_rect.set((left, top, width, height));
            }
        }
    });
}

fn add_block_to_program(
    bt_key: &str,
    node: &str,
    prog: &Program,
    ps: &crate::logic::store::ProgramStore,
    vb_x: f64,
    vb_y: f64,
    vb_w: f64,
    vb_h: f64,
    drop_pos: Option<(f64, f64)>,
    mut refresh_counter: Signal<u64>,
) {
    let block_type = match parse_block_type(bt_key, node) {
        Some(bt) => bt,
        None => return,
    };

    // Place at drop position or center of current view
    let (cx, cy) = if let Some((dx, dy)) = drop_pos {
        (grid_snap(dx), grid_snap(dy))
    } else {
        (
            grid_snap(vb_x + vb_w / 2.0 - BLOCK_W / 2.0),
            grid_snap(vb_y + vb_h / 2.0 - 30.0),
        )
    };

    // Use max existing block number + 1 to avoid ID collisions after deletions
    let max_num: u64 = prog
        .blocks
        .iter()
        .filter_map(|b| b.id.strip_prefix('b').and_then(|s| s.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    let block_id = format!("b{}", max_num + 1);

    let block = Block {
        id: block_id,
        block_type,
        x: cx.max(0.0),
        y: cy.max(0.0),
        enabled: true,
    };

    let ps = ps.clone();
    let mut p = prog.clone();
    spawn(async move {
        p.blocks.push(block);
        let _ = ps.update(p).await;
        {
            let v = *refresh_counter.peek();
            refresh_counter.set(v + 1);
        }
    });
}

// ----------------------------------------------------------------
// Code tab — shows compiled Rhai or override editor
// ----------------------------------------------------------------

#[component]
fn CodeTab(program_id: ProgramId, refresh_counter: Signal<u64>) -> Element {
    let state = use_context::<AppState>();
    let mut program = use_signal(|| Option::<Program>::None);
    let mut code_override = use_signal(String::new);
    let mut editing_override = use_signal(|| false);
    let mut compile_error = use_signal(|| Option::<String>::None);

    let prog_store = state.program_store.clone();
    let pid = program_id.clone();
    let _ = use_resource(move || {
        let ps = prog_store.clone();
        let pid = pid.clone();
        let _r = *refresh_counter.read();
        async move {
            if let Ok(p) = ps.get(&pid).await {
                if let Some(ref ovr) = p.rhai_override {
                    code_override.set(ovr.clone());
                }
                program.set(Some(p));
            }
        }
    });

    let Some(prog) = program.read().clone() else {
        return rsx! { div { class: "prog-loading", "Loading..." } };
    };

    // Try to compile
    let compiled_source = if prog.rhai_override.is_some() {
        prog.rhai_override.clone().unwrap_or_default()
    } else {
        match compile_program(&prog) {
            Ok(cp) => {
                if compile_error.read().is_some() {
                    compile_error.set(None);
                }
                cp.rhai_source
            }
            Err(e) => {
                let msg = e.to_string();
                if compile_error.read().as_deref() != Some(&msg) {
                    compile_error.set(Some(msg));
                }
                String::new()
            }
        }
    };

    let is_override = prog.rhai_override.is_some();
    let is_editing = *editing_override.read();

    rsx! {
        div { class: "prog-code-tab",
            div { class: "prog-code-header",
                if is_override {
                    span { class: "prog-code-badge override", "Rhai Override" }
                } else {
                    span { class: "prog-code-badge compiled", "Compiled from Blocks" }
                }

                if !is_editing {
                    button {
                        class: "prog-btn",
                        onclick: {
                            let src = compiled_source.clone();
                            move |_| {
                                code_override.set(src.clone());
                                editing_override.set(true);
                            }
                        },
                        if is_override { "Edit Override" } else { "Switch to Override" }
                    }
                }
                if is_override && !is_editing {
                    button {
                        class: "prog-btn",
                        onclick: {
                            let ps = state.program_store.clone();
                            let prog_clone = prog.clone();
                            move |_| {
                                let ps = ps.clone();
                                let mut p = prog_clone.clone();
                                spawn(async move {
                                    p.rhai_override = None;
                                    let _ = ps.update(p).await;
                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                });
                            }
                        },
                        "Revert to Blocks"
                    }
                }
            }

            if let Some(ref err) = *compile_error.read() {
                div { class: "prog-compile-error",
                    "Compile error: {err}"
                }
            }

            if is_editing {
                div { class: "prog-code-editor",
                    textarea {
                        class: "prog-textarea",
                        value: "{code_override}",
                        oninput: move |e| code_override.set(e.value().clone()),
                        rows: 20,
                    }
                    div { class: "prog-code-actions",
                        button {
                            class: "prog-btn prog-btn-primary",
                            onclick: {
                                let ps = state.program_store.clone();
                                let prog_clone = prog.clone();
                                move |_| {
                                    let code = code_override.read().clone();
                                    let ps = ps.clone();
                                    let mut p = prog_clone.clone();
                                    spawn(async move {
                                        if code.trim().is_empty() {
                                            p.rhai_override = None;
                                            let _ = ps.update(p).await;
                                            editing_override.set(false);
                                            { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                        } else {
                                            // Validate Rhai syntax before saving
                                            let engine = rhai::Engine::new();
                                            match engine.compile(&code) {
                                                Ok(_) => {
                                                    p.rhai_override = Some(code);
                                                    let _ = ps.update(p).await;
                                                    editing_override.set(false);
                                                    compile_error.set(None);
                                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                                }
                                                Err(e) => {
                                                    compile_error.set(Some(format!("Rhai syntax error: {e}")));
                                                }
                                            }
                                        }
                                    });
                                }
                            },
                            "Save"
                        }
                        button {
                            class: "prog-btn",
                            onclick: move |_| editing_override.set(false),
                            "Cancel"
                        }
                    }
                }
            } else {
                pre { class: "prog-code-display",
                    code { "{compiled_source}" }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Log tab — execution history
// ----------------------------------------------------------------

#[component]
fn LogTab(program_id: ProgramId) -> Element {
    let state = use_context::<AppState>();
    let mut entries = use_signal(Vec::<ExecutionLogEntry>::new);

    let prog_store = state.program_store.clone();
    let pid = program_id.clone();
    let _ = use_resource(move || {
        let ps = prog_store.clone();
        let pid = pid.clone();
        async move {
            let log = ps.get_execution_log(&pid, 50).await;
            entries.set(log);
        }
    });

    rsx! {
        div { class: "prog-log-tab",
            h4 { "Execution Log" }
            if entries.read().is_empty() {
                p { class: "prog-hint", "No executions recorded yet." }
            } else {
                table { class: "prog-log-table",
                    thead {
                        tr {
                            th { "Time" }
                            th { "Status" }
                            th { "Duration" }
                            th { "Outputs" }
                            th { "Error" }
                        }
                    }
                    tbody {
                        for entry in entries.read().iter() {
                            {
                                let time_str = format_timestamp(entry.executed_ms);
                                let status_class = if entry.success { "prog-status-ok" } else { "prog-status-err" };
                                let status_text = if entry.success { "OK" } else { "Error" };
                                let duration = format!("{:.1}ms", entry.duration_us as f64 / 1000.0);
                                let error_text = entry.error.clone().unwrap_or_default();
                                rsx! {
                                    tr {
                                        td { "{time_str}" }
                                        td { span { class: "{status_class}", "{status_text}" } }
                                        td { "{duration}" }
                                        td { "{entry.outputs_written}" }
                                        td { class: "prog-error-cell", "{error_text}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Right pane: Program properties + block editing
// ----------------------------------------------------------------

#[component]
fn ProgramPropertiesPanel(
    program_id: ProgramId,
    selected_program: Signal<Option<ProgramId>>,
    refresh_counter: Signal<u64>,
    selected_block: Signal<Option<BlockId>>,
) -> Element {
    let state = use_context::<AppState>();
    let mut program = use_signal(|| Option::<Program>::None);
    let mut edit_name = use_signal(String::new);
    let mut edit_desc = use_signal(String::new);
    let mut edit_interval = use_signal(|| "5000".to_string());
    let mut edit_nodes = use_signal(String::new);

    // Block editing signals
    let mut block_edit_node_id = use_signal(String::new);
    let mut block_edit_priority = use_signal(|| "none".to_string());
    let mut block_edit_value_float = use_signal(|| "0.0".to_string());
    let mut block_edit_value_int = use_signal(|| "0".to_string());
    let mut block_edit_value_bool = use_signal(|| false);
    let mut block_edit_kp = use_signal(|| "1.0".to_string());
    let mut block_edit_ki = use_signal(|| "0.1".to_string());
    let mut block_edit_kd = use_signal(|| "0.01".to_string());
    let mut block_edit_out_min = use_signal(|| "0.0".to_string());
    let mut block_edit_out_max = use_signal(|| "100.0".to_string());
    let mut block_edit_period = use_signal(|| "5000".to_string());
    let mut block_edit_message = use_signal(String::new);
    let mut block_edit_prefix = use_signal(String::new);
    let mut block_edit_code = use_signal(String::new);
    let mut block_edit_scale_in_min = use_signal(|| "0.0".to_string());
    let mut block_edit_scale_in_max = use_signal(|| "100.0".to_string());
    let mut block_edit_scale_out_min = use_signal(|| "0.0".to_string());
    let mut block_edit_scale_out_max = use_signal(|| "100.0".to_string());
    let mut block_edit_max_rate = use_signal(|| "1.0".to_string());

    // Node picker for block editing
    let mut block_node_picker_nodes: Signal<Vec<NodeRecord>> = use_signal(Vec::new);

    // Track which block we last loaded into edit signals
    let mut last_loaded_block: Signal<Option<BlockId>> = use_signal(|| None);

    // Load available nodes for block property picker
    {
        let ns = state.node_store.clone();
        let _ = use_resource(move || {
            let ns = ns.clone();
            async move {
                let mut all = ns.list_nodes(Some("point"), None).await;
                let vps = ns.list_nodes(Some("virtual_point"), None).await;
                all.extend(vps);
                block_node_picker_nodes.set(all);
            }
        });
    }

    let prog_store = state.program_store.clone();
    let pid = program_id.clone();
    let _ = use_resource(move || {
        let ps = prog_store.clone();
        let pid = pid.clone();
        let _r = *refresh_counter.read();
        async move {
            if let Ok(p) = ps.get(&pid).await {
                edit_name.set(p.name.clone());
                edit_desc.set(p.description.clone());
                match &p.trigger {
                    Trigger::Periodic { interval_ms } => {
                        edit_interval.set(interval_ms.to_string());
                    }
                    Trigger::OnChange { node_ids } => {
                        edit_nodes.set(node_ids.join(", "));
                    }
                }
                program.set(Some(p));
            }
        }
    });

    let Some(prog) = program.read().clone() else {
        return rsx! {};
    };

    // Load block edit signals when selected_block changes
    let sel_bid = selected_block.read().clone();
    if sel_bid != *last_loaded_block.read() {
        last_loaded_block.set(sel_bid.clone());
        if let Some(ref bid) = sel_bid {
            if let Some(block) = prog.blocks.iter().find(|b| &b.id == bid) {
                match &block.block_type {
                    BlockType::PointRead { node_id } => {
                        block_edit_node_id.set(node_id.clone());
                    }
                    BlockType::PointWrite { node_id, priority } => {
                        block_edit_node_id.set(node_id.clone());
                        block_edit_priority
                            .set(priority.map_or("none".to_string(), |p| p.to_string()));
                    }
                    BlockType::VirtualPoint { node_id } => {
                        block_edit_node_id.set(node_id.clone());
                    }
                    BlockType::Constant { value } => match value {
                        PointValue::Float(f) => block_edit_value_float.set(f.to_string()),
                        PointValue::Integer(i) => block_edit_value_int.set(i.to_string()),
                        PointValue::Bool(b) => block_edit_value_bool.set(*b),
                    },
                    BlockType::Pid {
                        kp,
                        ki,
                        kd,
                        output_min,
                        output_max,
                    } => {
                        block_edit_kp.set(kp.to_string());
                        block_edit_ki.set(ki.to_string());
                        block_edit_kd.set(kd.to_string());
                        block_edit_out_min.set(output_min.to_string());
                        block_edit_out_max.set(output_max.to_string());
                    }
                    BlockType::Timing { period_ms, .. } => {
                        block_edit_period.set(period_ms.to_string());
                    }
                    BlockType::AlarmTrigger { node_id, message } => {
                        block_edit_node_id.set(node_id.clone());
                        block_edit_message.set(message.clone());
                    }
                    BlockType::Log { prefix } => {
                        block_edit_prefix.set(prefix.clone());
                    }
                    BlockType::CustomScript { code } => {
                        block_edit_code.set(code.clone());
                    }
                    BlockType::Scale {
                        in_min,
                        in_max,
                        out_min,
                        out_max,
                    } => {
                        block_edit_scale_in_min.set(in_min.to_string());
                        block_edit_scale_in_max.set(in_max.to_string());
                        block_edit_scale_out_min.set(out_min.to_string());
                        block_edit_scale_out_max.set(out_max.to_string());
                    }
                    BlockType::RampLimit { max_rate } => {
                        block_edit_max_rate.set(max_rate.to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    let is_periodic = matches!(prog.trigger, Trigger::Periodic { .. });

    // Find the selected block
    let sel_block = sel_bid
        .as_ref()
        .and_then(|bid| prog.blocks.iter().find(|b| &b.id == bid));

    // Build node options for select dropdown
    let node_options: Vec<(String, String)> = block_node_picker_nodes
        .read()
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                if n.dis.is_empty() {
                    n.id.clone()
                } else {
                    format!("{} ({})", n.dis, n.id)
                },
            )
        })
        .collect();

    rsx! {
        div { class: "details-pane",
            div { class: "details-header",
                span { if sel_block.is_some() { "Block Properties" } else { "Properties" } }
            }
            div { class: "prog-props-body",

                // ── Block editing section ──
                if let Some(block) = sel_block {
                    {
                        let block_label = block_type_label(&block.block_type);
                        let bid = block.id.clone();
                        rsx! {
                            div { class: "block-prop-group",
                                label { class: "block-prop-label", "Block: {block_label} ({bid})" }
                            }

                            // Block-specific fields
                            {
                                match &block.block_type {
                                    BlockType::Constant { value } => {
                                        match value {
                                            PointValue::Float(_) => rsx! {
                                                div { class: "block-prop-group",
                                                    label { class: "block-prop-label", "Value (float)" }
                                                    input {
                                                        class: "prog-input",
                                                        r#type: "number",
                                                        step: "any",
                                                        value: "{block_edit_value_float}",
                                                        oninput: move |e| block_edit_value_float.set(e.value().clone()),
                                                    }
                                                }
                                            },
                                            PointValue::Integer(_) => rsx! {
                                                div { class: "block-prop-group",
                                                    label { class: "block-prop-label", "Value (integer)" }
                                                    input {
                                                        class: "prog-input",
                                                        r#type: "number",
                                                        value: "{block_edit_value_int}",
                                                        oninput: move |e| block_edit_value_int.set(e.value().clone()),
                                                    }
                                                }
                                            },
                                            PointValue::Bool(_) => rsx! {
                                                div { class: "block-prop-group",
                                                    label { class: "block-prop-label", "Value (bool)" }
                                                    button {
                                                        class: if *block_edit_value_bool.read() { "prog-toggle on" } else { "prog-toggle off" },
                                                        onclick: move |_| {
                                                            let cur = *block_edit_value_bool.peek();
                                                            block_edit_value_bool.set(!cur);
                                                        },
                                                        if *block_edit_value_bool.read() { "TRUE" } else { "FALSE" }
                                                    }
                                                }
                                            },
                                        }
                                    }

                                    BlockType::PointRead { .. } | BlockType::VirtualPoint { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Node ID" }
                                            select {
                                                class: "prog-select",
                                                value: "{block_edit_node_id}",
                                                onchange: move |e| block_edit_node_id.set(e.value().clone()),
                                                option { value: "(select point)", "(select point)" }
                                                for (nid, display) in node_options.iter() {
                                                    option { value: "{nid}", "{display}" }
                                                }
                                            }
                                        }
                                    },

                                    BlockType::PointWrite { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Node ID" }
                                            select {
                                                class: "prog-select",
                                                value: "{block_edit_node_id}",
                                                onchange: move |e| block_edit_node_id.set(e.value().clone()),
                                                option { value: "(select point)", "(select point)" }
                                                for (nid, display) in node_options.iter() {
                                                    option { value: "{nid}", "{display}" }
                                                }
                                            }
                                        }
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Priority" }
                                            select {
                                                class: "prog-select",
                                                value: "{block_edit_priority}",
                                                onchange: move |e| block_edit_priority.set(e.value().clone()),
                                                option { value: "none", "None" }
                                                option { value: "1", "1 - Manual Life Safety" }
                                                option { value: "2", "2 - Auto Life Safety" }
                                                option { value: "3", "3 - Available" }
                                                option { value: "4", "4 - Available" }
                                                option { value: "5", "5 - Critical Equip" }
                                                option { value: "6", "6 - Minimum On/Off" }
                                                option { value: "7", "7 - Available" }
                                                option { value: "8", "8 - Manual Operator" }
                                                option { value: "9", "9 - Available" }
                                                option { value: "10", "10 - Available" }
                                                option { value: "11", "11 - Available" }
                                                option { value: "12", "12 - Available" }
                                                option { value: "13", "13 - Available" }
                                                option { value: "14", "14 - Available" }
                                                option { value: "15", "15 - Available" }
                                                option { value: "16", "16 - Default" }
                                            }
                                        }
                                    },

                                    BlockType::Pid { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Kp (Proportional)" }
                                            input {
                                                class: "prog-input",
                                                r#type: "number",
                                                step: "any",
                                                value: "{block_edit_kp}",
                                                oninput: move |e| block_edit_kp.set(e.value().clone()),
                                            }
                                        }
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Ki (Integral)" }
                                            input {
                                                class: "prog-input",
                                                r#type: "number",
                                                step: "any",
                                                value: "{block_edit_ki}",
                                                oninput: move |e| block_edit_ki.set(e.value().clone()),
                                            }
                                        }
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Kd (Derivative)" }
                                            input {
                                                class: "prog-input",
                                                r#type: "number",
                                                step: "any",
                                                value: "{block_edit_kd}",
                                                oninput: move |e| block_edit_kd.set(e.value().clone()),
                                            }
                                        }
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Output Min" }
                                            input {
                                                class: "prog-input",
                                                r#type: "number",
                                                step: "any",
                                                value: "{block_edit_out_min}",
                                                oninput: move |e| block_edit_out_min.set(e.value().clone()),
                                            }
                                        }
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Output Max" }
                                            input {
                                                class: "prog-input",
                                                r#type: "number",
                                                step: "any",
                                                value: "{block_edit_out_max}",
                                                oninput: move |e| block_edit_out_max.set(e.value().clone()),
                                            }
                                        }
                                    },

                                    BlockType::Timing { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Period (ms)" }
                                            input {
                                                class: "prog-input",
                                                r#type: "number",
                                                value: "{block_edit_period}",
                                                oninput: move |e| block_edit_period.set(e.value().clone()),
                                            }
                                        }
                                    },

                                    BlockType::AlarmTrigger { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Node ID" }
                                            select {
                                                class: "prog-select",
                                                value: "{block_edit_node_id}",
                                                onchange: move |e| block_edit_node_id.set(e.value().clone()),
                                                option { value: "(select point)", "(select point)" }
                                                for (nid, display) in node_options.iter() {
                                                    option { value: "{nid}", "{display}" }
                                                }
                                            }
                                        }
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Message" }
                                            input {
                                                class: "prog-input",
                                                value: "{block_edit_message}",
                                                oninput: move |e| block_edit_message.set(e.value().clone()),
                                            }
                                        }
                                    },

                                    BlockType::Log { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Prefix" }
                                            input {
                                                class: "prog-input",
                                                value: "{block_edit_prefix}",
                                                oninput: move |e| block_edit_prefix.set(e.value().clone()),
                                            }
                                        }
                                    },

                                    BlockType::CustomScript { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Code" }
                                            textarea {
                                                class: "prog-textarea",
                                                value: "{block_edit_code}",
                                                oninput: move |e| block_edit_code.set(e.value().clone()),
                                                rows: 8,
                                            }
                                        }
                                    },

                                    BlockType::Scale { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Input Range" }
                                            div { style: "display: flex; gap: 4px;",
                                                input {
                                                    class: "prog-input",
                                                    r#type: "number",
                                                    step: "any",
                                                    placeholder: "min",
                                                    value: "{block_edit_scale_in_min}",
                                                    oninput: move |e| block_edit_scale_in_min.set(e.value().clone()),
                                                }
                                                input {
                                                    class: "prog-input",
                                                    r#type: "number",
                                                    step: "any",
                                                    placeholder: "max",
                                                    value: "{block_edit_scale_in_max}",
                                                    oninput: move |e| block_edit_scale_in_max.set(e.value().clone()),
                                                }
                                            }
                                        }
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Output Range" }
                                            div { style: "display: flex; gap: 4px;",
                                                input {
                                                    class: "prog-input",
                                                    r#type: "number",
                                                    step: "any",
                                                    placeholder: "min",
                                                    value: "{block_edit_scale_out_min}",
                                                    oninput: move |e| block_edit_scale_out_min.set(e.value().clone()),
                                                }
                                                input {
                                                    class: "prog-input",
                                                    r#type: "number",
                                                    step: "any",
                                                    placeholder: "max",
                                                    value: "{block_edit_scale_out_max}",
                                                    oninput: move |e| block_edit_scale_out_max.set(e.value().clone()),
                                                }
                                            }
                                        }
                                    },

                                    BlockType::RampLimit { .. } => rsx! {
                                        div { class: "block-prop-group",
                                            label { class: "block-prop-label", "Max Rate (/sec)" }
                                            input {
                                                class: "prog-input",
                                                r#type: "number",
                                                step: "any",
                                                value: "{block_edit_max_rate}",
                                                oninput: move |e| block_edit_max_rate.set(e.value().clone()),
                                            }
                                        }
                                    },

                                    // Math, Logic, Compare, Select, Latch, OneShot — no editable fields
                                    _ => rsx! {},
                                }
                            }

                            // Enable/Disable toggle
                            div { class: "block-prop-group",
                                style: "flex-direction: row; align-items: center; gap: 8px;",
                                label { class: "block-prop-label", "Enabled" }
                                button {
                                    class: if block.enabled { "prog-toggle on" } else { "prog-toggle off" },
                                    onclick: {
                                        let ps_en = state.program_store.clone();
                                        let block_id_en = block.id.clone();
                                        let is_enabled = block.enabled;
                                        move |_| {
                                            let live_snap = { program.peek().clone() };
                                            if let Some(mut p) = live_snap {
                                                if let Some(b) = p.blocks.iter_mut().find(|b| b.id == block_id_en) {
                                                    b.enabled = !is_enabled;
                                                }
                                                let ps = ps_en.clone();
                                                spawn(async move {
                                                    let _ = ps.update(p).await;
                                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                                });
                                            }
                                        }
                                    },
                                    if block.enabled { "ON" } else { "OFF" }
                                }
                            }

                            // Apply button
                            button {
                                class: "prog-btn prog-btn-primary prog-btn-full",
                                onclick: {
                                    let ps = state.program_store.clone();
                                    let block_id = block.id.clone();
                                    let block_type = block.block_type.clone();
                                    move |_| {
                                        let new_bt = build_updated_block_type(
                                            &block_type,
                                            &block_edit_node_id.read(),
                                            &block_edit_priority.read(),
                                            &block_edit_value_float.read(),
                                            &block_edit_value_int.read(),
                                            *block_edit_value_bool.read(),
                                            &block_edit_kp.read(),
                                            &block_edit_ki.read(),
                                            &block_edit_kd.read(),
                                            &block_edit_out_min.read(),
                                            &block_edit_out_max.read(),
                                            &block_edit_period.read(),
                                            &block_edit_message.read(),
                                            &block_edit_prefix.read(),
                                            &block_edit_code.read(),
                                            &block_edit_scale_in_min.read(),
                                            &block_edit_scale_in_max.read(),
                                            &block_edit_scale_out_min.read(),
                                            &block_edit_scale_out_max.read(),
                                            &block_edit_max_rate.read(),
                                        );
                                        let live_snap = { program.peek().clone() };
                                        if let Some(mut p) = live_snap {
                                            if let Some(b) = p.blocks.iter_mut().find(|b| b.id == block_id) {
                                                b.block_type = new_bt;
                                            }
                                            let ps = ps.clone();
                                            spawn(async move {
                                                let _ = ps.update(p).await;
                                                { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                            });
                                        }
                                    }
                                },
                                "Apply"
                            }

                            // Delete Block button
                            button {
                                class: "prog-btn prog-btn-danger prog-btn-full",
                                style: "margin-top: 4px;",
                                onclick: {
                                    let ps = state.program_store.clone();
                                    let block_id = block.id.clone();
                                    move |_| {
                                        let live_snap = { program.peek().clone() };
                                        if let Some(mut p) = live_snap {
                                            let bid = block_id.clone();
                                            p.blocks.retain(|b| b.id != bid);
                                            p.wires.retain(|w| w.from_block != bid && w.to_block != bid);
                                            let ps = ps.clone();
                                            spawn(async move {
                                                let _ = ps.update(p).await;
                                                { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                            });
                                        }
                                        selected_block.set(None);
                                    }
                                },
                                "Delete Block"
                            }

                            // Separator
                            hr { style: "border: none; border-top: 1px solid var(--border); margin: 8px 0;" }
                        }
                    }
                }

                // ── Program-level properties ──

                // Name
                div { class: "prog-prop-group",
                    label { "Name" }
                    input {
                        class: "prog-input",
                        value: "{edit_name}",
                        oninput: move |e| edit_name.set(e.value().clone()),
                    }
                }

                // Description
                div { class: "prog-prop-group",
                    label { "Description" }
                    textarea {
                        class: "prog-textarea prog-textarea-sm",
                        value: "{edit_desc}",
                        oninput: move |e| edit_desc.set(e.value().clone()),
                        rows: 3,
                    }
                }

                // Trigger config
                div { class: "prog-prop-group",
                    label { "Trigger" }
                    if is_periodic {
                        div { class: "prog-trigger-config",
                            label { "Interval (ms)" }
                            input {
                                class: "prog-input",
                                r#type: "number",
                                value: "{edit_interval}",
                                oninput: move |e| edit_interval.set(e.value().clone()),
                            }
                        }
                    } else {
                        div { class: "prog-trigger-config",
                            label { "Trigger nodes (comma-separated)" }
                            input {
                                class: "prog-input",
                                placeholder: "ahu-1/oat, ahu-1/dat",
                                value: "{edit_nodes}",
                                oninput: move |e| edit_nodes.set(e.value().clone()),
                            }
                        }
                    }
                }

                // Save button
                button {
                    class: "prog-btn prog-btn-primary prog-btn-full",
                    onclick: {
                        let ps = state.program_store.clone();
                        let prog_clone = prog.clone();
                        move |_| {
                            let name = edit_name.read().clone();
                            let desc = edit_desc.read().clone();
                            let ps = ps.clone();
                            let mut p = prog_clone.clone();
                            p.name = name;
                            p.description = desc;
                            if is_periodic {
                                if let Ok(ms) = edit_interval.read().parse::<u64>() {
                                    p.trigger = Trigger::Periodic { interval_ms: ms.max(1000) };
                                }
                            } else {
                                let nodes: Vec<String> = edit_nodes.read()
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                p.trigger = Trigger::OnChange { node_ids: nodes };
                            }
                            spawn(async move {
                                let _ = ps.update(p).await;
                                { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                            });
                        }
                    },
                    "Save Properties"
                }

                // Enable/disable toggle
                div { class: "prog-prop-group prog-toggle-group",
                    label { "Enabled" }
                    button {
                        class: if prog.enabled { "prog-toggle on" } else { "prog-toggle off" },
                        onclick: {
                            let ps = state.program_store.clone();
                            let pid = prog.id.clone();
                            let enabled = prog.enabled;
                            let audit_state = state.clone();
                            move |_| {
                                let ps = ps.clone();
                                let pid = pid.clone();
                                let audit_state = audit_state.clone();
                                spawn(async move {
                                    let _ = ps.set_enabled(&pid, !enabled).await;
                                    let action = if !enabled {
                                        bms_store_storage::store::audit_store::AuditAction::EnableProgram
                                    } else {
                                        bms_store_storage::store::audit_store::AuditAction::DisableProgram
                                    };
                                    audit_state.audit(
                                        bms_store_storage::store::audit_store::AuditEntryBuilder::new(action, "program")
                                            .resource_id(&pid),
                                    );
                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                });
                            }
                        },
                        if prog.enabled { "ON" } else { "OFF" }
                    }
                }

                // Delete button
                div { class: "prog-prop-group prog-danger-zone",
                    button {
                        class: "prog-btn prog-btn-danger",
                        onclick: {
                            let ps = state.program_store.clone();
                            let pid = prog.id.clone();
                            let audit_state = state.clone();
                            move |_| {
                                let ps = ps.clone();
                                let pid = pid.clone();
                                let audit_state = audit_state.clone();
                                spawn(async move {
                                    let _ = ps.delete(&pid).await;
                                    audit_state.audit(
                                        bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                            bms_store_storage::store::audit_store::AuditAction::DeleteProgram, "program",
                                        ).resource_id(&pid),
                                    );
                                    selected_program.set(None);
                                    { let v = *refresh_counter.peek(); refresh_counter.set(v + 1); }
                                });
                            }
                        },
                        "Delete Program"
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Build updated block type from edit signals
// ----------------------------------------------------------------

fn build_updated_block_type(
    original: &BlockType,
    node_id: &str,
    priority_str: &str,
    value_float: &str,
    value_int: &str,
    value_bool: bool,
    kp_str: &str,
    ki_str: &str,
    kd_str: &str,
    out_min_str: &str,
    out_max_str: &str,
    period_str: &str,
    message: &str,
    prefix: &str,
    code: &str,
    scale_in_min: &str,
    scale_in_max: &str,
    scale_out_min: &str,
    scale_out_max: &str,
    max_rate: &str,
) -> BlockType {
    match original {
        BlockType::PointRead { .. } => BlockType::PointRead {
            node_id: node_id.to_string(),
        },
        BlockType::PointWrite { .. } => BlockType::PointWrite {
            node_id: node_id.to_string(),
            priority: if priority_str == "none" {
                None
            } else {
                priority_str.parse::<u8>().ok()
            },
        },
        BlockType::VirtualPoint { .. } => BlockType::VirtualPoint {
            node_id: node_id.to_string(),
        },
        BlockType::Constant { value } => {
            let new_value = match value {
                PointValue::Float(_) => PointValue::Float(value_float.parse().unwrap_or(0.0)),
                PointValue::Integer(_) => PointValue::Integer(value_int.parse().unwrap_or(0)),
                PointValue::Bool(_) => PointValue::Bool(value_bool),
            };
            BlockType::Constant { value: new_value }
        }
        BlockType::Pid { .. } => BlockType::Pid {
            kp: kp_str.parse().unwrap_or(1.0),
            ki: ki_str.parse().unwrap_or(0.1),
            kd: kd_str.parse().unwrap_or(0.01),
            output_min: out_min_str.parse().unwrap_or(0.0),
            output_max: out_max_str.parse().unwrap_or(100.0),
        },
        BlockType::Timing { op, .. } => BlockType::Timing {
            op: op.clone(),
            period_ms: period_str.parse().unwrap_or(5000),
        },
        BlockType::AlarmTrigger { .. } => BlockType::AlarmTrigger {
            node_id: node_id.to_string(),
            message: message.to_string(),
        },
        BlockType::Log { .. } => BlockType::Log {
            prefix: prefix.to_string(),
        },
        BlockType::CustomScript { .. } => BlockType::CustomScript {
            code: code.to_string(),
        },
        BlockType::Scale { .. } => BlockType::Scale {
            in_min: scale_in_min.parse().unwrap_or(0.0),
            in_max: scale_in_max.parse().unwrap_or(100.0),
            out_min: scale_out_min.parse().unwrap_or(0.0),
            out_max: scale_out_max.parse().unwrap_or(100.0),
        },
        BlockType::RampLimit { .. } => BlockType::RampLimit {
            max_rate: max_rate.parse().unwrap_or(1.0),
        },
        // Math, Logic, Compare, Select, Latch, OneShot — not editable, return clone
        other => other.clone(),
    }
}

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------

fn block_type_label(bt: &BlockType) -> &'static str {
    match bt {
        BlockType::PointRead { .. } => "Read",
        BlockType::Constant { .. } => "Const",
        BlockType::Math { op } => match op {
            MathOp::Add => "Add",
            MathOp::Sub => "Sub",
            MathOp::Mul => "Mul",
            MathOp::Div => "Div",
            MathOp::Min => "Min",
            MathOp::Max => "Max",
            MathOp::Abs => "Abs",
            MathOp::Clamp => "Clamp",
        },
        BlockType::Logic { op } => match op {
            LogicOp::And => "AND",
            LogicOp::Or => "OR",
            LogicOp::Not => "NOT",
            LogicOp::Xor => "XOR",
        },
        BlockType::Compare { op } => match op {
            CompareOp::Gt => ">",
            CompareOp::Lt => "<",
            CompareOp::Gte => ">=",
            CompareOp::Lte => "<=",
            CompareOp::Eq => "==",
            CompareOp::Neq => "!=",
        },
        BlockType::Select => "Select",
        BlockType::Timing { op, .. } => match op {
            TimingOp::DelayOn => "Delay On",
            TimingOp::DelayOff => "Delay Off",
            TimingOp::MovingAverage => "Moving Avg",
            TimingOp::RateOfChange => "Rate/Chg",
        },
        BlockType::Pid { .. } => "PID",
        BlockType::PointWrite { .. } => "Write",
        BlockType::VirtualPoint { .. } => "Virtual",
        BlockType::AlarmTrigger { .. } => "Alarm",
        BlockType::Log { .. } => "Log",
        BlockType::CustomScript { .. } => "Script",
        BlockType::Latch => "Latch",
        BlockType::OneShot => "One-Shot",
        BlockType::Scale { .. } => "Scale",
        BlockType::RampLimit { .. } => "Ramp Lim",
    }
}

fn block_type_detail(bt: &BlockType) -> String {
    match bt {
        BlockType::PointRead { node_id } => node_id.clone(),
        BlockType::PointWrite { node_id, priority } => {
            if let Some(p) = priority {
                format!("{node_id} @{p}")
            } else {
                node_id.clone()
            }
        }
        BlockType::Constant { value } => match value {
            PointValue::Float(f) => format!("{f}"),
            PointValue::Integer(i) => format!("{i}"),
            PointValue::Bool(b) => format!("{b}"),
        },
        BlockType::Pid {
            kp,
            ki,
            kd,
            output_min,
            output_max,
        } => format!("P={kp} I={ki} D={kd} [{output_min}..{output_max}]"),
        BlockType::Timing { op, period_ms } => format!("{:?} {}ms", op, period_ms),
        BlockType::VirtualPoint { node_id } => node_id.clone(),
        BlockType::AlarmTrigger { node_id, message } => format!("{node_id}: {message}"),
        BlockType::Log { prefix } => prefix.clone(),
        BlockType::CustomScript { code } => {
            let preview: String = code.chars().take(40).collect();
            if code.len() > 40 {
                format!("{preview}...")
            } else {
                preview
            }
        }
        BlockType::Scale {
            in_min,
            in_max,
            out_min,
            out_max,
        } => {
            format!("[{in_min}..{in_max}] → [{out_min}..{out_max}]")
        }
        BlockType::RampLimit { max_rate } => format!("±{max_rate}/s"),
        _ => String::new(),
    }
}

fn parse_block_type(bt_str: &str, node: &str) -> Option<BlockType> {
    Some(match bt_str {
        "point_read" => {
            let nid = if node.is_empty() {
                "(select point)"
            } else {
                node
            };
            BlockType::PointRead {
                node_id: nid.to_string(),
            }
        }
        "point_write" => {
            let nid = if node.is_empty() {
                "(select point)"
            } else {
                node
            };
            BlockType::PointWrite {
                node_id: nid.to_string(),
                priority: None,
            }
        }
        "constant_float" => BlockType::Constant {
            value: PointValue::Float(0.0),
        },
        "constant_bool" => BlockType::Constant {
            value: PointValue::Bool(false),
        },
        "constant_int" => BlockType::Constant {
            value: PointValue::Integer(0),
        },
        "math_add" => BlockType::Math { op: MathOp::Add },
        "math_sub" => BlockType::Math { op: MathOp::Sub },
        "math_mul" => BlockType::Math { op: MathOp::Mul },
        "math_div" => BlockType::Math { op: MathOp::Div },
        "math_min" => BlockType::Math { op: MathOp::Min },
        "math_max" => BlockType::Math { op: MathOp::Max },
        "math_abs" => BlockType::Math { op: MathOp::Abs },
        "math_clamp" => BlockType::Math { op: MathOp::Clamp },
        "compare_gt" => BlockType::Compare { op: CompareOp::Gt },
        "compare_lt" => BlockType::Compare { op: CompareOp::Lt },
        "compare_gte" => BlockType::Compare { op: CompareOp::Gte },
        "compare_lte" => BlockType::Compare { op: CompareOp::Lte },
        "compare_eq" => BlockType::Compare { op: CompareOp::Eq },
        "compare_neq" => BlockType::Compare { op: CompareOp::Neq },
        "logic_and" => BlockType::Logic { op: LogicOp::And },
        "logic_or" => BlockType::Logic { op: LogicOp::Or },
        "logic_not" => BlockType::Logic { op: LogicOp::Not },
        "logic_xor" => BlockType::Logic { op: LogicOp::Xor },
        "select" => BlockType::Select,
        "pid" => BlockType::Pid {
            kp: 1.0,
            ki: 0.1,
            kd: 0.01,
            output_min: 0.0,
            output_max: 100.0,
        },
        "timing_delay_on" => BlockType::Timing {
            op: TimingOp::DelayOn,
            period_ms: 5000,
        },
        "timing_delay_off" => BlockType::Timing {
            op: TimingOp::DelayOff,
            period_ms: 5000,
        },
        "timing_moving_avg" => BlockType::Timing {
            op: TimingOp::MovingAverage,
            period_ms: 60000,
        },
        "timing_rate_change" => BlockType::Timing {
            op: TimingOp::RateOfChange,
            period_ms: 1000,
        },
        "log" => BlockType::Log {
            prefix: "program".to_string(),
        },
        "alarm_trigger" => {
            let nid = if node.is_empty() {
                "(select point)"
            } else {
                node
            };
            BlockType::AlarmTrigger {
                node_id: nid.to_string(),
                message: "alarm".to_string(),
            }
        }
        "virtual_point" => {
            let nid = if node.is_empty() {
                "(select point)"
            } else {
                node
            };
            BlockType::VirtualPoint {
                node_id: nid.to_string(),
            }
        }
        "custom_script" => BlockType::CustomScript {
            code: "// logic here\nout = in1;".to_string(),
        },
        "latch" => BlockType::Latch,
        "one_shot" => BlockType::OneShot,
        "scale" => BlockType::Scale {
            in_min: 0.0,
            in_max: 100.0,
            out_min: 0.0,
            out_max: 100.0,
        },
        "ramp_limit" => BlockType::RampLimit { max_rate: 1.0 },
        _ => return None,
    })
}

fn format_timestamp(ms: i64) -> String {
    let secs = ms / 1000;
    let mins = (secs / 60) % 60;
    let hours = (secs / 3600) % 24;
    let s = secs % 60;
    format!("{hours:02}:{mins:02}:{s:02}")
}
