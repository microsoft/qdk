// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use arbitrary::Unstructured;
use rustc_hash::FxHashMap;

use crate::{
    model::{Attribute, AttributeGroup, MetadataNode, MetadataValue, Module, NamedMetadata, Type},
    qir,
};

use crate::fuzz::qir_smith::generator::ShellCounts;

const SUPPORTED_FLOAT_COMPUTATIONS: [&str; 3] = ["half", "float", "double"];

pub(super) fn build_qdk_attribute_groups(
    profile: qir::QirProfile,
    shell_counts: ShellCounts,
) -> Vec<AttributeGroup> {
    vec![
        AttributeGroup {
            id: qir::ENTRY_POINT_ATTR_GROUP_ID,
            attributes: vec![
                Attribute::StringAttr(qir::ENTRY_POINT_ATTR.to_string()),
                Attribute::StringAttr(qir::OUTPUT_LABELING_SCHEMA_ATTR.to_string()),
                Attribute::KeyValue(
                    qir::QIR_PROFILES_ATTR.to_string(),
                    profile.profile_name().to_string(),
                ),
                Attribute::KeyValue(
                    qir::REQUIRED_NUM_QUBITS_ATTR.to_string(),
                    shell_counts.required_num_qubits.to_string(),
                ),
                Attribute::KeyValue(
                    qir::REQUIRED_NUM_RESULTS_ATTR.to_string(),
                    shell_counts.required_num_results.to_string(),
                ),
            ],
        },
        AttributeGroup {
            id: qir::IRREVERSIBLE_ATTR_GROUP_ID,
            attributes: vec![Attribute::StringAttr(qir::IRREVERSIBLE_ATTR.to_string())],
        },
    ]
}

pub(super) fn build_qdk_metadata(
    profile: qir::QirProfile,
    _bytes: &mut Unstructured<'_>,
) -> (Vec<NamedMetadata>, Vec<MetadataNode>) {
    let mut metadata_nodes = vec![
        MetadataNode {
            id: 0,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_ERROR),
                MetadataValue::String(qir::QIR_MAJOR_VERSION_KEY.to_string()),
                MetadataValue::Int(Type::Integer(32), profile.major_version()),
            ],
        },
        MetadataNode {
            id: 1,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_MAX),
                MetadataValue::String(qir::QIR_MINOR_VERSION_KEY.to_string()),
                MetadataValue::Int(Type::Integer(32), profile.minor_version()),
            ],
        },
        MetadataNode {
            id: 2,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_ERROR),
                MetadataValue::String(qir::DYNAMIC_QUBIT_MGMT_KEY.to_string()),
                MetadataValue::Int(Type::Integer(1), 0),
            ],
        },
        MetadataNode {
            id: 3,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_ERROR),
                MetadataValue::String(qir::DYNAMIC_RESULT_MGMT_KEY.to_string()),
                MetadataValue::Int(Type::Integer(1), 0),
            ],
        },
    ];

    if matches!(profile, qir::QirProfile::AdaptiveV2) {
        metadata_nodes.push(MetadataNode {
            id: 4,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_APPEND),
                MetadataValue::String(qir::INT_COMPUTATIONS_KEY.to_string()),
                MetadataValue::SubList(vec![MetadataValue::String("i64".to_string())]),
            ],
        });
        metadata_nodes.push(MetadataNode {
            id: 5,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_APPEND),
                MetadataValue::String(qir::FLOAT_COMPUTATIONS_KEY.to_string()),
                supported_float_computation_metadata(),
            ],
        });
        metadata_nodes.push(MetadataNode {
            id: 6,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_MAX),
                MetadataValue::String(qir::BACKWARDS_BRANCHING_KEY.to_string()),
                MetadataValue::Int(Type::Integer(2), 3),
            ],
        });
        metadata_nodes.push(MetadataNode {
            id: 7,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_ERROR),
                MetadataValue::String(qir::ARRAYS_KEY.to_string()),
                MetadataValue::Int(Type::Integer(1), 1),
            ],
        });
    }

    if profile == qir::QirProfile::AdaptiveV1 {
        let next_id =
            u32::try_from(metadata_nodes.len()).expect("metadata node count should fit in u32");
        metadata_nodes.push(MetadataNode {
            id: next_id,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_APPEND),
                MetadataValue::String(qir::INT_COMPUTATIONS_KEY.to_string()),
                MetadataValue::SubList(vec![MetadataValue::String("i64".to_string())]),
            ],
        });

        let next_id =
            u32::try_from(metadata_nodes.len()).expect("metadata node count should fit in u32");
        metadata_nodes.push(MetadataNode {
            id: next_id,
            values: vec![
                MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_APPEND),
                MetadataValue::String(qir::FLOAT_COMPUTATIONS_KEY.to_string()),
                supported_float_computation_metadata(),
            ],
        });
    }

    let node_refs: Vec<u32> = metadata_nodes.iter().map(|n| n.id).collect();

    (
        vec![NamedMetadata {
            name: qir::MODULE_FLAGS_NAME.to_string(),
            node_refs,
        }],
        metadata_nodes,
    )
}

fn supported_float_computation_metadata() -> MetadataValue {
    MetadataValue::SubList(
        SUPPORTED_FLOAT_COMPUTATIONS
            .iter()
            .map(|width| MetadataValue::String((*width).to_string()))
            .collect(),
    )
}

pub(super) fn finalize_float_computations(module: &mut Module) {
    let analysis = qir::inspect::analyze_float_surface(module);
    let float_flag_node_id = find_module_flag_node_id(module, qir::FLOAT_COMPUTATIONS_KEY);

    if !analysis.has_float_op {
        if let Some(node_id) = float_flag_node_id {
            remove_module_flag_node(module, node_id);
        }
        return;
    }

    let metadata_value = MetadataValue::SubList(
        analysis
            .surface_width_names()
            .into_iter()
            .map(|width| MetadataValue::String(width.to_string()))
            .collect(),
    );

    if let Some(node_id) = float_flag_node_id
        && let Some(node) = module
            .metadata_nodes
            .iter_mut()
            .find(|candidate| candidate.id == node_id)
    {
        let behavior = node.values.first().cloned().unwrap_or(MetadataValue::Int(
            Type::Integer(32),
            qir::FLAG_BEHAVIOR_APPEND,
        ));
        node.values = vec![
            behavior,
            MetadataValue::String(qir::FLOAT_COMPUTATIONS_KEY.to_string()),
            metadata_value,
        ];
        return;
    }

    if !module
        .named_metadata
        .iter()
        .any(|metadata| metadata.name == qir::MODULE_FLAGS_NAME)
    {
        return;
    }

    let node_id = next_metadata_node_id(module);
    module.metadata_nodes.push(MetadataNode {
        id: node_id,
        values: vec![
            MetadataValue::Int(Type::Integer(32), qir::FLAG_BEHAVIOR_APPEND),
            MetadataValue::String(qir::FLOAT_COMPUTATIONS_KEY.to_string()),
            metadata_value,
        ],
    });

    if let Some(module_flags) = module
        .named_metadata
        .iter_mut()
        .find(|metadata| metadata.name == qir::MODULE_FLAGS_NAME)
    {
        module_flags.node_refs.push(node_id);
    }
}

fn find_module_flag_node_id(module: &Module, key: &str) -> Option<u32> {
    let module_flags = module
        .named_metadata
        .iter()
        .find(|metadata| metadata.name == qir::MODULE_FLAGS_NAME)?;

    for &node_ref in &module_flags.node_refs {
        let Some(node) = module
            .metadata_nodes
            .iter()
            .find(|candidate| candidate.id == node_ref)
        else {
            continue;
        };
        if node.values.len() >= 2
            && let MetadataValue::String(flag_name) = &node.values[1]
            && flag_name == key
        {
            return Some(node_ref);
        }
    }

    None
}

fn next_metadata_node_id(module: &Module) -> u32 {
    module
        .metadata_nodes
        .iter()
        .map(|node| node.id)
        .max()
        .map_or(0, |id| id.saturating_add(1))
}

fn remap_metadata_value_node_refs(value: &mut MetadataValue, id_remap: &FxHashMap<u32, u32>) {
    match value {
        MetadataValue::NodeRef(node_id) => {
            if let Some(remapped_id) = id_remap.get(node_id).copied() {
                *node_id = remapped_id;
            }
        }
        MetadataValue::SubList(values) => {
            for child in values {
                remap_metadata_value_node_refs(child, id_remap);
            }
        }
        MetadataValue::Int(_, _) | MetadataValue::String(_) => {}
    }
}

fn renumber_metadata_nodes(module: &mut Module) {
    let old_ids: Vec<u32> = module.metadata_nodes.iter().map(|node| node.id).collect();
    if old_ids
        .iter()
        .enumerate()
        .all(|(index, &node_id)| node_id == u32::try_from(index).expect("invalid index value"))
    {
        return;
    }

    let id_remap: FxHashMap<u32, u32> = old_ids
        .iter()
        .enumerate()
        .map(|(index, &old_id)| (old_id, u32::try_from(index).expect("invalid index value")))
        .collect();

    for metadata in &mut module.named_metadata {
        metadata.node_refs = metadata
            .node_refs
            .iter()
            .filter_map(|node_id| id_remap.get(node_id).copied())
            .collect();
    }

    for node in &mut module.metadata_nodes {
        node.id = id_remap[&node.id];
        for value in &mut node.values {
            remap_metadata_value_node_refs(value, &id_remap);
        }
    }
}

fn remove_module_flag_node(module: &mut Module, node_id: u32) {
    if let Some(module_flags) = module
        .named_metadata
        .iter_mut()
        .find(|metadata| metadata.name == qir::MODULE_FLAGS_NAME)
    {
        module_flags
            .node_refs
            .retain(|&candidate| candidate != node_id);
    }

    module.metadata_nodes.retain(|node| node.id != node_id);
    renumber_metadata_nodes(module);
}
