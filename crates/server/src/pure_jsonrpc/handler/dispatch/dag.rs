// SPDX-License-Identifier: AGPL-3.0-or-later
//! DAG validation and topological sorting for pipeline dispatch.
//!
//! Extracted from `pipeline.rs` (S203) so the graph algorithms are
//! independently testable and reusable.

use std::collections::HashMap;

use super::types::PipelineStageRequest;
use crate::pure_jsonrpc::types::{JsonRpcError, JsonRpcErrorExt};

/// Parse `"edges"` from the request as `Vec<(String, String)>`.
pub(super) fn parse_edges(p: &serde_json::Value) -> Result<Vec<(String, String)>, JsonRpcError> {
    let Some(edges_val) = p.get("edges") else {
        return Ok(Vec::new());
    };

    let arr = edges_val
        .as_array()
        .ok_or_else(|| JsonRpcError::invalid_params("'edges' must be an array of [from, to]"))?;

    arr.iter()
        .map(|edge| {
            let pair = edge.as_array().ok_or_else(|| {
                JsonRpcError::invalid_params("Each edge must be [from_id, to_id]")
            })?;
            if pair.len() != 2 {
                return Err(JsonRpcError::invalid_params(
                    "Each edge must have exactly 2 elements",
                ));
            }
            let from = pair[0]
                .as_str()
                .ok_or_else(|| JsonRpcError::invalid_params("Edge 'from' must be a string"))?
                .to_string();
            let to = pair[1]
                .as_str()
                .ok_or_else(|| JsonRpcError::invalid_params("Edge 'to' must be a string"))?
                .to_string();
            Ok((from, to))
        })
        .collect()
}

/// Kahn's algorithm: topological sort of stages by edges.
///
/// Returns `Err` if the graph contains a cycle or references unknown stages.
pub(super) fn topological_sort(
    stages: &[PipelineStageRequest],
    edges: &[(String, String)],
) -> Result<Vec<String>, JsonRpcError> {
    let ids: Vec<&str> = stages.iter().map(|s| s.id.as_str()).collect();
    let id_set: std::collections::HashSet<&str> = ids.iter().copied().collect();

    for (from, to) in edges {
        if !id_set.contains(from.as_str()) {
            return Err(JsonRpcError::invalid_params(format!(
                "Edge references unknown stage: {from}"
            )));
        }
        if !id_set.contains(to.as_str()) {
            return Err(JsonRpcError::invalid_params(format!(
                "Edge references unknown stage: {to}"
            )));
        }
    }

    let mut in_degree: HashMap<&str, usize> = ids.iter().map(|id| (*id, 0)).collect();
    let mut adjacency: HashMap<&str, Vec<&str>> = ids.iter().map(|id| (*id, Vec::new())).collect();

    for (from, to) in edges {
        if let Some(neighbors) = adjacency.get_mut(from.as_str()) {
            neighbors.push(to.as_str());
        }
        if let Some(deg) = in_degree.get_mut(to.as_str()) {
            *deg += 1;
        }
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|&(_, deg)| *deg == 0)
        .map(|(&id, _)| id)
        .collect();
    queue.sort_unstable();

    let mut order = Vec::with_capacity(stages.len());

    while let Some(node) = queue.pop() {
        order.push(node.to_string());
        if let Some(neighbors) = adjacency.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(neighbor);
                        queue.sort_unstable();
                    }
                }
            }
        }
    }

    if order.len() == stages.len() {
        Ok(order)
    } else {
        Err(JsonRpcError::invalid_params(
            "Pipeline graph contains a cycle — stages must form a DAG",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::PipelineSubstrate;
    use super::*;

    #[test]
    fn topological_sort_linear_chain() {
        let stages = vec![
            PipelineStageRequest {
                id: "a".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
            PipelineStageRequest {
                id: "b".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
            PipelineStageRequest {
                id: "c".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
        ];
        let edges = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "c".to_string()),
        ];
        let order = topological_sort(&stages, &edges).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn topological_sort_diamond_dag() {
        let stages = vec![
            PipelineStageRequest {
                id: "root".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
            PipelineStageRequest {
                id: "left".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::GpuOnly,
            },
            PipelineStageRequest {
                id: "right".into(),
                method: "shader.dispatch".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::GpuPreferred,
            },
            PipelineStageRequest {
                id: "join".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
        ];
        let edges = vec![
            ("root".to_string(), "left".to_string()),
            ("root".to_string(), "right".to_string()),
            ("left".to_string(), "join".to_string()),
            ("right".to_string(), "join".to_string()),
        ];
        let order = topological_sort(&stages, &edges).unwrap();
        assert_eq!(order[0], "root");
        assert_eq!(*order.last().unwrap(), "join");
    }

    #[test]
    fn topological_sort_cycle_rejected() {
        let stages = vec![
            PipelineStageRequest {
                id: "a".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
            PipelineStageRequest {
                id: "b".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
        ];
        let edges = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
        ];
        let err = topological_sort(&stages, &edges).unwrap_err();
        assert!(err.message.contains("cycle"));
    }

    #[test]
    fn topological_sort_unknown_stage_rejected() {
        let stages = vec![PipelineStageRequest {
            id: "a".into(),
            method: "compute.dispatch.submit".into(),
            params: serde_json::json!({}),
            substrate: PipelineSubstrate::Any,
        }];
        let edges = vec![("a".to_string(), "nonexistent".to_string())];
        let err = topological_sort(&stages, &edges).unwrap_err();
        assert!(err.message.contains("nonexistent"));
    }

    #[test]
    fn topological_sort_no_edges() {
        let stages = vec![
            PipelineStageRequest {
                id: "x".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
            PipelineStageRequest {
                id: "y".into(),
                method: "compute.dispatch.submit".into(),
                params: serde_json::json!({}),
                substrate: PipelineSubstrate::Any,
            },
        ];
        let order = topological_sort(&stages, &[]).unwrap();
        assert_eq!(order.len(), 2);
    }

    #[test]
    fn parse_edges_empty() {
        let p = serde_json::json!({"stages": []});
        let edges = parse_edges(&p).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn parse_edges_valid() {
        let p = serde_json::json!({
            "edges": [["a", "b"], ["b", "c"]]
        });
        let edges = parse_edges(&p).unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0], ("a".to_string(), "b".to_string()));
    }

    #[test]
    fn parse_edges_invalid_shape() {
        let p = serde_json::json!({"edges": [["a"]]});
        assert!(parse_edges(&p).is_err());
    }
}
