use std::collections::HashMap;

use crate::state::AppState;
use crate::validation;
use apex_core::application::graph_engine::{EdgeData, EdgeType, GraphDto, NodeData, NodeType};
use apex_core::domain::models::{OHLCVQuery, Symbol, Timeframe};
use chrono::{Duration, Utc};
use tauri::State;
use uuid::Uuid;

/// Return the current relationship graph snapshot.
#[tauri::command]
pub async fn get_graph(state: State<'_, AppState>) -> Result<GraphDto, String> {
    Ok(state.graph.read().await.to_dto())
}

/// Compute pairwise daily-return correlations across `symbols` over
/// `window_days`, upsert instrument nodes + correlation edges into the graph
/// engine, and return the full graph snapshot for rendering.
#[tauri::command]
pub async fn compute_correlations(
    symbols: Vec<String>,
    window_days: Option<u32>,
    state: State<'_, AppState>,
) -> Result<GraphDto, String> {
    if symbols.len() < 2 {
        return Err("Provide at least two symbols".to_string());
    }
    if symbols.len() > 40 {
        return Err("Too many symbols (max 40)".to_string());
    }
    for s in &symbols {
        validation::validate_symbol(s)?;
    }
    let window = window_days.unwrap_or(90).clamp(5, 1825);

    // Pull daily closes for each symbol from storage.
    let to = Utc::now();
    let from = to - Duration::days(window as i64);
    let mut closes: HashMap<String, Vec<(chrono::DateTime<Utc>, f64)>> = HashMap::new();

    for s in &symbols {
        let bars = state
            .storage
            .query_ohlcv(OHLCVQuery {
                symbol: Symbol(s.clone()),
                timeframe: Timeframe::D1,
                from,
                to,
                limit: Some(window as usize + 10),
            })
            .await
            .map_err(|e| format!("Failed to load history for {}: {}", s, e))?;
        if bars.len() >= 3 {
            closes.insert(s.clone(), bars.iter().map(|b| (b.time, b.close)).collect());
        }
    }

    // Daily log/simple returns per symbol, aligned on common dates.
    let mut returns: HashMap<String, HashMap<i64, f64>> = HashMap::new();
    for (sym, series) in &closes {
        let mut by_day: HashMap<i64, f64> = HashMap::new();
        let mut sorted = series.clone();
        sorted.sort_by_key(|(t, _)| *t);
        for w in sorted.windows(2) {
            let (t0, c0) = w[0];
            let (t1, c1) = w[1];
            if c0 != 0.0 {
                by_day.insert(t1.timestamp() / 86_400, (c1 - c0) / c0);
            }
            let _ = t0;
        }
        returns.insert(sym.clone(), by_day);
    }

    let mut graph = state.graph.write().await;

    // Upsert instrument nodes (keyed by symbol via properties scan).
    let existing: HashMap<String, Uuid> = graph
        .to_dto()
        .nodes
        .into_iter()
        .filter(|n| n.node_type == NodeType::Instrument)
        .filter_map(|n| n.symbol.clone().map(|s| (s, n.id)))
        .collect();

    let mut ids: HashMap<String, Uuid> = HashMap::new();
    for s in &symbols {
        let id = existing.get(s).copied().unwrap_or_else(|| {
            graph.add_node(NodeData {
                id: Uuid::new_v4(),
                node_type: NodeType::Instrument,
                label: s.clone(),
                symbol: Some(s.clone()),
                properties: HashMap::new(),
            })
        });
        ids.insert(s.clone(), id);
    }

    // Clear previous correlation edges for the requested pairs first — a pair
    // that can no longer be computed (not enough overlap) must drop its stale
    // edge rather than keep last window's coefficient.
    let syms: Vec<&String> = symbols.iter().collect();
    for i in 0..syms.len() {
        for j in (i + 1)..syms.len() {
            let (a, b) = (ids[syms[i]], ids[syms[j]]);
            graph.remove_edge(&a, &b);
            graph.remove_edge(&b, &a);
        }
    }

    // Pearson correlation on overlapping return dates; add computed edges.
    for i in 0..syms.len() {
        for j in (i + 1)..syms.len() {
            let (a, b) = (syms[i], syms[j]);
            let (ra, rb) = match (returns.get(a), returns.get(b)) {
                (Some(ra), Some(rb)) => (ra, rb),
                _ => continue,
            };
            let common: Vec<(f64, f64)> = ra
                .iter()
                .filter_map(|(day, r)| rb.get(day).map(|r2| (*r, *r2)))
                .collect();
            if common.len() < 5 {
                continue;
            }
            let n = common.len() as f64;
            let (sx, sy) = common.iter().fold((0.0, 0.0), |(ax, ay), (x, y)| (ax + x, ay + y));
            let (sxx, syy, sxy) = common.iter().fold((0.0, 0.0, 0.0), |(ax, ay, az), (x, y)| {
                (ax + x * x, ay + y * y, az + x * y)
            });
            let denom = ((n * sxx - sx * sx) * (n * syy - sy * sy)).sqrt();
            if denom <= f64::EPSILON {
                continue;
            }
            let coeff = ((n * sxy - sx * sy) / denom).clamp(-1.0, 1.0);

            let (from_id, to_id) = (ids[a], ids[b]);
            graph.add_edge(
                &from_id,
                &to_id,
                EdgeData {
                    edge_type: EdgeType::CorrelatedWith {
                        coefficient: coeff,
                        window: format!("{}d", window),
                    },
                    weight: coeff.abs(),
                    metadata: HashMap::new(),
                },
            );
        }
    }

    Ok(graph.to_dto())
}
