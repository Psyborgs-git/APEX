use std::collections::HashMap;

use petgraph::graph::{Graph, NodeIndex};
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Node types
// ---------------------------------------------------------------------------

/// The kind of entity a graph node represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    Instrument,
    Sector,
    MacroVariable,
    NewsEvent,
    CustomVariable,
}

/// Data attached to every node in the relationship graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeData {
    pub id: Uuid,
    pub node_type: NodeType,
    pub label: String,
    pub symbol: Option<String>,
    pub properties: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Edge types
// ---------------------------------------------------------------------------

/// Semantic relationship between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeType {
    CorrelatedWith { coefficient: f64, window: String },
    BelongsTo,
    LeadsBy { lag_hours: f64 },
    PricedIn { currency: String },
    Custom(String),
}

/// Data attached to every edge in the relationship graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeData {
    pub edge_type: EdgeType,
    pub weight: f64,
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// DTO for serialisation to the UI
// ---------------------------------------------------------------------------

/// Lightweight, serialisable snapshot of the graph for the front-end.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphDto {
    pub nodes: Vec<NodeData>,
    pub edges: Vec<EdgeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDto {
    pub source: Uuid,
    pub target: Uuid,
    pub data: EdgeData,
}

// ---------------------------------------------------------------------------
// Correlation result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationResult {
    pub node_a: Uuid,
    pub node_b: Uuid,
    pub coefficient: f64,
}

// ---------------------------------------------------------------------------
// GraphEngine
// ---------------------------------------------------------------------------

/// In-memory relationship graph that tracks instruments, sectors, macro
/// variables, news events and the edges between them.
pub struct GraphEngine {
    graph: Graph<NodeData, EdgeData>,
    /// Fast lookup: node UUID → petgraph NodeIndex
    index_map: HashMap<Uuid, NodeIndex>,
}

impl GraphEngine {
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
            index_map: HashMap::new(),
        }
    }

    // -- Node operations ---------------------------------------------------

    /// Insert a node and return its UUID.
    #[tracing::instrument(skip(self), fields(label = %data.label))]
    pub fn add_node(&mut self, data: NodeData) -> Uuid {
        let id = data.id;
        let idx = self.graph.add_node(data);
        self.index_map.insert(id, idx);
        id
    }

    /// Remove a node (and all its edges) by UUID. Returns `true` if found.
    #[tracing::instrument(skip(self))]
    pub fn remove_node(&mut self, id: &Uuid) -> bool {
        if let Some(idx) = self.index_map.remove(id) {
            self.graph.remove_node(idx);
            // After removal petgraph may swap the last node into the vacated
            // slot — update the index_map for the swapped node if any.
            if let Some(swapped) = self.graph.node_weight(idx) {
                self.index_map.insert(swapped.id, idx);
            }
            true
        } else {
            false
        }
    }

    /// Get a reference to a node by UUID.
    pub fn get_node(&self, id: &Uuid) -> Option<&NodeData> {
        self.index_map.get(id).and_then(|idx| self.graph.node_weight(*idx))
    }

    /// Total number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    // -- Edge operations ---------------------------------------------------

    /// Add a directed edge between two nodes. Returns `true` on success.
    #[tracing::instrument(skip(self, data))]
    pub fn add_edge(&mut self, from: &Uuid, to: &Uuid, data: EdgeData) -> bool {
        if let (Some(&src), Some(&dst)) = (self.index_map.get(from), self.index_map.get(to)) {
            self.graph.add_edge(src, dst, data);
            true
        } else {
            false
        }
    }

    /// Remove **all** edges between `from` and `to`. Returns number removed.
    #[tracing::instrument(skip(self))]
    pub fn remove_edge(&mut self, from: &Uuid, to: &Uuid) -> usize {
        let (Some(&src), Some(&dst)) = (self.index_map.get(from), self.index_map.get(to)) else {
            return 0;
        };
        let mut removed = 0;
        while let Some(edge) = self.graph.find_edge(src, dst) {
            self.graph.remove_edge(edge);
            removed += 1;
        }
        removed
    }

    /// Total number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    // -- Query operations --------------------------------------------------

    /// Return the data of all neighbours of a node (both directions).
    pub fn get_neighbors(&self, id: &Uuid) -> Vec<&NodeData> {
        let Some(&idx) = self.index_map.get(id) else {
            return Vec::new();
        };
        self.graph
            .neighbors_undirected(idx)
            .filter_map(|n| self.graph.node_weight(n))
            .collect()
    }

    /// Compute pair-wise correlations between all `Instrument` nodes using the
    /// edge weights already stored in the graph.
    ///
    /// In a production system this would pull live time-series data; here we
    /// expose the stored `CorrelatedWith` coefficients as a convenience.
    pub fn compute_correlations(&self) -> Vec<CorrelationResult> {
        let mut results = Vec::new();
        for edge_idx in self.graph.edge_indices() {
            let edge = &self.graph[edge_idx];
            if let EdgeType::CorrelatedWith { coefficient, .. } = &edge.edge_type {
                if let Some((src, dst)) = self.graph.edge_endpoints(edge_idx) {
                    let src_data = &self.graph[src];
                    let dst_data = &self.graph[dst];
                    if src_data.node_type == NodeType::Instrument
                        && dst_data.node_type == NodeType::Instrument
                    {
                        results.push(CorrelationResult {
                            node_a: src_data.id,
                            node_b: dst_data.id,
                            coefficient: *coefficient,
                        });
                    }
                }
            }
        }
        results
    }

    /// Produce a lightweight DTO for the front-end.
    pub fn to_dto(&self) -> GraphDto {
        let nodes: Vec<NodeData> = self
            .graph
            .node_indices()
            .filter_map(|idx| self.graph.node_weight(idx).cloned())
            .collect();

        let edges: Vec<EdgeDto> = self
            .graph
            .edge_indices()
            .filter_map(|eidx| {
                let (src, dst) = self.graph.edge_endpoints(eidx)?;
                let src_data = self.graph.node_weight(src)?;
                let dst_data = self.graph.node_weight(dst)?;
                let data = self.graph.edge_weight(eidx)?.clone();
                Some(EdgeDto {
                    source: src_data.id,
                    target: dst_data.id,
                    data,
                })
            })
            .collect();

        GraphDto { nodes, edges }
    }

    /// Return nodes filtered by incoming/outgoing direction from a given node.
    pub fn get_directed_neighbors(&self, id: &Uuid, direction: Direction) -> Vec<&NodeData> {
        let Some(&idx) = self.index_map.get(id) else {
            return Vec::new();
        };
        self.graph
            .neighbors_directed(idx, direction)
            .filter_map(|n| self.graph.node_weight(n))
            .collect()
    }
}

impl Default for GraphEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn instrument_node(label: &str) -> NodeData {
        NodeData {
            id: Uuid::new_v4(),
            node_type: NodeType::Instrument,
            label: label.to_string(),
            symbol: Some(label.to_string()),
            properties: HashMap::new(),
        }
    }

    fn sector_node(label: &str) -> NodeData {
        NodeData {
            id: Uuid::new_v4(),
            node_type: NodeType::Sector,
            label: label.to_string(),
            symbol: None,
            properties: HashMap::new(),
        }
    }

    #[test]
    fn test_add_and_get_node() {
        let mut engine = GraphEngine::new();
        let node = instrument_node("AAPL");
        let id = node.id;
        engine.add_node(node);

        assert_eq!(engine.node_count(), 1);
        let fetched = engine.get_node(&id).unwrap();
        assert_eq!(fetched.label, "AAPL");
    }

    #[test]
    fn test_remove_node() {
        let mut engine = GraphEngine::new();
        let n1 = instrument_node("AAPL");
        let n2 = instrument_node("MSFT");
        let id1 = n1.id;
        let id2 = n2.id;
        engine.add_node(n1);
        engine.add_node(n2);

        assert!(engine.remove_node(&id1));
        assert_eq!(engine.node_count(), 1);
        assert!(engine.get_node(&id1).is_none());
        // The other node must still be reachable.
        assert!(engine.get_node(&id2).is_some());
    }

    #[test]
    fn test_remove_nonexistent_node() {
        let mut engine = GraphEngine::new();
        assert!(!engine.remove_node(&Uuid::new_v4()));
    }

    #[test]
    fn test_add_and_remove_edge() {
        let mut engine = GraphEngine::new();
        let n1 = instrument_node("AAPL");
        let n2 = instrument_node("MSFT");
        let id1 = n1.id;
        let id2 = n2.id;
        engine.add_node(n1);
        engine.add_node(n2);

        let ok = engine.add_edge(
            &id1,
            &id2,
            EdgeData {
                edge_type: EdgeType::CorrelatedWith {
                    coefficient: 0.85,
                    window: "30d".into(),
                },
                weight: 0.85,
                metadata: HashMap::new(),
            },
        );
        assert!(ok);
        assert_eq!(engine.edge_count(), 1);

        let removed = engine.remove_edge(&id1, &id2);
        assert_eq!(removed, 1);
        assert_eq!(engine.edge_count(), 0);
    }

    #[test]
    fn test_add_edge_missing_node() {
        let mut engine = GraphEngine::new();
        let n = instrument_node("AAPL");
        let id = n.id;
        engine.add_node(n);

        let ok = engine.add_edge(
            &id,
            &Uuid::new_v4(),
            EdgeData {
                edge_type: EdgeType::BelongsTo,
                weight: 1.0,
                metadata: HashMap::new(),
            },
        );
        assert!(!ok);
    }

    #[test]
    fn test_get_neighbors() {
        let mut engine = GraphEngine::new();
        let n1 = instrument_node("AAPL");
        let n2 = instrument_node("MSFT");
        let n3 = sector_node("Tech");
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        engine.add_node(n1);
        engine.add_node(n2);
        engine.add_node(n3);

        engine.add_edge(
            &id1,
            &id3,
            EdgeData {
                edge_type: EdgeType::BelongsTo,
                weight: 1.0,
                metadata: HashMap::new(),
            },
        );
        engine.add_edge(
            &id2,
            &id3,
            EdgeData {
                edge_type: EdgeType::BelongsTo,
                weight: 1.0,
                metadata: HashMap::new(),
            },
        );

        let neighbors = engine.get_neighbors(&id3);
        assert_eq!(neighbors.len(), 2);

        let labels: Vec<&str> = neighbors.iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains(&"AAPL"));
        assert!(labels.contains(&"MSFT"));
    }

    #[test]
    fn test_compute_correlations() {
        let mut engine = GraphEngine::new();
        let n1 = instrument_node("AAPL");
        let n2 = instrument_node("MSFT");
        let id1 = n1.id;
        let id2 = n2.id;
        engine.add_node(n1);
        engine.add_node(n2);

        engine.add_edge(
            &id1,
            &id2,
            EdgeData {
                edge_type: EdgeType::CorrelatedWith {
                    coefficient: 0.92,
                    window: "90d".into(),
                },
                weight: 0.92,
                metadata: HashMap::new(),
            },
        );

        let corrs = engine.compute_correlations();
        assert_eq!(corrs.len(), 1);
        assert!((corrs[0].coefficient - 0.92).abs() < f64::EPSILON);
        assert_eq!(corrs[0].node_a, id1);
        assert_eq!(corrs[0].node_b, id2);
    }

    #[test]
    fn test_correlations_skip_non_instruments() {
        let mut engine = GraphEngine::new();
        let n1 = instrument_node("AAPL");
        let n2 = sector_node("Tech");
        let id1 = n1.id;
        let id2 = n2.id;
        engine.add_node(n1);
        engine.add_node(n2);

        engine.add_edge(
            &id1,
            &id2,
            EdgeData {
                edge_type: EdgeType::CorrelatedWith {
                    coefficient: 0.5,
                    window: "30d".into(),
                },
                weight: 0.5,
                metadata: HashMap::new(),
            },
        );

        let corrs = engine.compute_correlations();
        assert!(corrs.is_empty());
    }

    #[test]
    fn test_to_dto() {
        let mut engine = GraphEngine::new();
        let n1 = instrument_node("AAPL");
        let n2 = instrument_node("MSFT");
        let id1 = n1.id;
        let id2 = n2.id;
        engine.add_node(n1);
        engine.add_node(n2);

        engine.add_edge(
            &id1,
            &id2,
            EdgeData {
                edge_type: EdgeType::BelongsTo,
                weight: 1.0,
                metadata: HashMap::new(),
            },
        );

        let dto = engine.to_dto();
        assert_eq!(dto.nodes.len(), 2);
        assert_eq!(dto.edges.len(), 1);
        assert_eq!(dto.edges[0].source, id1);
        assert_eq!(dto.edges[0].target, id2);
    }

    #[test]
    fn test_remove_node_with_edges() {
        let mut engine = GraphEngine::new();
        let n1 = instrument_node("AAPL");
        let n2 = instrument_node("MSFT");
        let n3 = sector_node("Tech");
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        engine.add_node(n1);
        engine.add_node(n2);
        engine.add_node(n3);

        engine.add_edge(
            &id1,
            &id2,
            EdgeData {
                edge_type: EdgeType::BelongsTo,
                weight: 1.0,
                metadata: HashMap::new(),
            },
        );
        engine.add_edge(
            &id1,
            &id3,
            EdgeData {
                edge_type: EdgeType::BelongsTo,
                weight: 1.0,
                metadata: HashMap::new(),
            },
        );

        // Removing n1 should also remove its edges.
        engine.remove_node(&id1);
        assert_eq!(engine.node_count(), 2);
        assert_eq!(engine.edge_count(), 0);
        // Remaining nodes still valid.
        assert!(engine.get_node(&id2).is_some());
        assert!(engine.get_node(&id3).is_some());
    }

    #[test]
    fn test_default_trait() {
        let engine = GraphEngine::default();
        assert_eq!(engine.node_count(), 0);
        assert_eq!(engine.edge_count(), 0);
    }

    #[test]
    fn test_directed_neighbors() {
        let mut engine = GraphEngine::new();
        let n1 = instrument_node("AAPL");
        let n2 = instrument_node("MSFT");
        let n3 = sector_node("Tech");
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        engine.add_node(n1);
        engine.add_node(n2);
        engine.add_node(n3);

        // n1 -> n3, n2 -> n3
        engine.add_edge(
            &id1,
            &id3,
            EdgeData {
                edge_type: EdgeType::BelongsTo,
                weight: 1.0,
                metadata: HashMap::new(),
            },
        );
        engine.add_edge(
            &id2,
            &id3,
            EdgeData {
                edge_type: EdgeType::BelongsTo,
                weight: 1.0,
                metadata: HashMap::new(),
            },
        );

        // Outgoing from n1 should be [n3]
        let out = engine.get_directed_neighbors(&id1, petgraph::Direction::Outgoing);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].label, "Tech");

        // Incoming to n3 should be [n1, n2]
        let inc = engine.get_directed_neighbors(&id3, petgraph::Direction::Incoming);
        assert_eq!(inc.len(), 2);
    }

    #[test]
    fn test_edge_types_serialize() {
        let edge = EdgeData {
            edge_type: EdgeType::LeadsBy { lag_hours: 2.5 },
            weight: 0.7,
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&edge).unwrap();
        let deserialized: EdgeData = serde_json::from_str(&json).unwrap();
        assert!((deserialized.weight - 0.7).abs() < f64::EPSILON);
    }
}
