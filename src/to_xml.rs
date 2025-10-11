use crate::common::bpmn_node::ActivityType;
use crate::common::bpmn_node::BpmnNode;
use crate::common::bpmn_node::EventVisual;
use crate::common::bpmn_node::InterruptKind;
use crate::common::bpmn_node::TaskType;
use crate::common::edge::Edge;
use crate::common::edge::EdgeType;
use crate::common::edge::FlowType;
use crate::common::edge::RegularEdgeBendPoints;
use crate::common::graph::EdgeId;
use crate::common::graph::Graph;
use crate::common::node::Node;
use crate::common::node::NodeType;
use crate::lexer::DataType;
use crate::lexer::EventType;
use std::fmt::Display;

struct IncomingOutgoing<'a> {
    incoming: &'a [EdgeId],
    outgoing: &'a [EdgeId],
    graph: &'a Graph,
}

impl Display for IncomingOutgoing<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for EdgeId(edge_idx) in self.incoming.iter().cloned() {
            let edge = &self.graph.edges[edge_idx];
            match edge.flow_type {
                FlowType::DataFlow(_) => {
                    write!(
                        f,
                        r#"      <bpmn:dataInputAssociation id="Flow_{}">
        <bpmn:sourceRef>Node_{}</bpmn:sourceRef>
        <bpmn:targetRef>Node_{}</bpmn:targetRef>
      </bpmn:dataInputAssociation>
"#,
                        edge_idx, edge.from.0, edge.to.0
                    )?;
                }
                FlowType::SequenceFlow => {
                    writeln!(f, "      <bpmn:incoming>Flow_{}</bpmn:incoming>", edge_idx)?
                }
                FlowType::MessageFlow(_) => {
                    // void:
                    // Message flows are just part of the `collaboration` and `BPMNDiagram`
                    // sections, not of the process nodes (at least it seems like that).
                }
            }
        }
        for EdgeId(edge_idx) in self.outgoing.iter().cloned() {
            let edge = &self.graph.edges[edge_idx];
            match edge.flow_type {
                FlowType::DataFlow(_) => {
                    write!(
                        f,
                        r#"      <bpmn:dataOutputAssociation id="Flow_{}">
        <bpmn:targetRef>Node_{}</bpmn:targetRef>
      </bpmn:dataOutputAssociation>
"#,
                        edge_idx, edge.to.0
                    )?;
                }
                FlowType::SequenceFlow => {
                    writeln!(f, "      <bpmn:outgoing>Flow_{}</bpmn:outgoing>", edge_idx)?
                }
                FlowType::MessageFlow(_) => {
                    // void:
                    // Message flows are just part of the `collaboration` and `BPMNDiagram`
                    // sections, not of the process nodes (at least it seems like that).
                }
            }
        }
        Ok(())
    }
}

struct EventDefinition<'a>(&'a EventType);

impl Display for EventDefinition<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO only a handful of them are actually tested.
        let text = match *self.0 {
            EventType::Blank => return Ok(()),
            EventType::Timer => "<bpmn:timerEventDefinition />",
            EventType::Message => "<bpmn:messageEventDefinition />",
            EventType::Conditional => "<bpmn:conditionalEventDefinition />",
            EventType::Link => "<bpmn:conditionalEventDefinition />",
            EventType::Signal => "<bpmn:signalEventDefinition />",
            EventType::Error => "<bpmn:errorEventDefinition />",
            EventType::Escalation => "<bpmn:escalationEventDefinition />",
            EventType::Termination => "<bpmn:terminationEventDefinition />",
            EventType::Compensation => "<bpmn:compensationEventDefinition />",
            EventType::Cancel => "<bpmn:cancelEventDefinition />",
            EventType::Multiple => "<bpmn:multipleEventDefinition />",
            EventType::MultipleParallel => "<bpmn:multipleParallelEventDefinition />",
        };
        writeln!(f, "      {text}")
    }
}

pub fn generate_bpmn(graph: &Graph) -> String {
    let mut bpmn = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL"
    xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI"
    xmlns:dc="http://www.omg.org/spec/DD/20100524/DC"
    xmlns:di="http://www.omg.org/spec/DD/20100524/DI"
    xmlns:bioc="http://bpmn.io/schema/bpmn/biocolor/1.0"
    xmlns:color="http://www.omg.org/spec/BPMN/non-normative/color/1.0"
    xmlns:modeler="http://camunda.org/schema/modeler/1.0" id="Definitions_1"
    targetNamespace="http://bpmn.io/schema/bpmn" exporter="Camunda Modeler"
    exporterVersion="5.17.0">
"#,
    );

    let has_pools = !matches!(&graph.pools[..], [pool] if pool.name.is_none() && matches!(&pool.lanes[..], [lane] if lane.name.is_none()));

    if has_pools {
        bpmn.push_str("  <bpmn:collaboration id=\"Collaboration_1\">\n");

        for (id, pool) in graph.pools.iter().enumerate() {
            bpmn.push_str(&format!(
                "    <bpmn:participant id=\"Participant_{id}\" name=\"{}\" processRef=\"Process_{id}\" />\n", pool.name.as_ref().map_or("", AsRef::as_ref)
            ));
        }
        for (id, edge) in graph
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| Edge::is_message_flow(&e))
        {
            bpmn.push_str(&format!(
                "    <bpmn:messageFlow id=\"Flow_{id}\" name=\"{}\" sourceRef=\"Node_{}\" targetRef=\"Node_{}\" />\n", edge.text().unwrap_or(""),edge.from.0, edge.to.0
            ));
        }
        bpmn.push_str("  </bpmn:collaboration>\n");
    }

    for (pool_id, pool) in graph.pools.iter().enumerate() {
        bpmn.push_str(&format!(
            "  <bpmn:process id=\"Process_{pool_id}\" isExecutable=\"true\">\n"
        ));
        if !pool.lanes.is_empty() && pool.lanes[0].name.is_some() {
            bpmn.push_str(&format!("    <bpmn:laneSet id=\"LaneSet_{}\">\n", pool_id));

            for (lane_id, lane) in pool.lanes.iter().enumerate() {
                bpmn.push_str(&format!(
                    "      <bpmn:lane id=\"Lane_{pool_id}_{lane_id}\" name=\"{}\">\n",
                    lane.name.clone().unwrap_or_default()
                ));

                for node_id in &lane.nodes {
                    bpmn.push_str(&format!(
                        "        <bpmn:flowNodeRef>Node_{node_id}</bpmn:flowNodeRef>\n"
                    ));
                }

                bpmn.push_str("      </bpmn:lane>\n");
            }

            bpmn.push_str("    </bpmn:laneSet>\n");
        }

        pool.lanes
            .iter()
            .flat_map(|lane| lane.nodes.iter())
            .map(|node_id| (node_id.0, &graph.nodes[node_id.0]))
            .filter(|(_, node)| node.is_some_sequence_flow_box())
            .for_each(|(node_idx, node)| {
                write_process_node(&mut bpmn, graph, node_idx, node);
            });

        pool.lanes
            .iter()
            .flat_map(|lane| lane.nodes.iter())
            .map(|node_id| (node_id.0, &graph.nodes[node_id.0]))
            .for_each(|(node_idx, node)| match &node.node_type {
                NodeType::RealNode {
                    event: BpmnNode::Data(DataType::Store, _),
                    display_text,
                    ..
                } => bpmn.push_str(&format!(
                    "        <bpmn:dataStoreReference id=\"Node_{node_idx}\" name=\"{display_text}\" />\n",

                )),
                NodeType::RealNode {
                    event: BpmnNode::Data(DataType::Object, _),
                    display_text,
                    ..
                } => bpmn.push_str(&format!(
                    "        <bpmn:dataObjectReference id=\"Node_{node_idx}\" name=\"{display_text}\" />\n",
                )),
                _ => { /* skip - we are only interested in data objects right now */ }
            });

        // Generate sequence flows
        pool.lanes.iter()
            .flat_map(|lane| lane.nodes.iter())
            .flat_map(|n| graph.nodes[n.0].outgoing.iter())
            .map(|edge_id| (edge_id.0, &graph.edges[edge_id.0]))
            .filter(|(_, edge)| edge.flow_type == FlowType::SequenceFlow)
            .for_each(|(edge_idx, e)|  {
                match &e.edge_type {
                    EdgeType::Regular { text: None, .. } =>
                    // _display_text will be used at some point to display the text label.
                    bpmn.push_str(&format!(
                    "    <bpmn:sequenceFlow id=\"Flow_{edge_idx}\" sourceRef=\"Node_{}\" targetRef=\"Node_{}\" />\n",
                    e.from.0, e.to.0
                )),
                    EdgeType::Regular { text: Some(_text), .. } =>
                    // _display_text will be used at some point to display the text label.
                    bpmn.push_str(&format!(
                    "    <bpmn:sequenceFlow id=\"Flow_{edge_idx}\" sourceRef=\"Node_{}\" targetRef=\"Node_{}\" />\n",
                    e.from.0, e.to.0
                )),
                    _ => (),
                }
        });

        bpmn.push_str("  </bpmn:process>\n");
    }

    // Add BPMN diagram elements (BPMNPlane and BPMNShape)
    if has_pools {
        bpmn.push_str(
            r#"  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="Collaboration_1">
"#,
        );
    } else {
        bpmn.push_str(
            r#"  <bpmndi:BPMNDiagram id="BPMNDiagram_1">
    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="Process_0">
"#,
        );
    }

    for (pool_id, pool) in graph.pools.iter().enumerate() {
        bpmn.push_str(&format!(
            r#"      <bpmndi:BPMNShape id="Participant_{pool_id}_di" bpmnElement="Participant_{pool_id}" isHorizontal="true" isExpanded="true" color:background-color="{}" color:border-color="{}">
        <dc:Bounds x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" />
      </bpmndi:BPMNShape>
"#,
            /* fill color */ pool.fill_color.as_deref().unwrap_or("#ffffff"),
            /* stroke color */ pool.stroke_color.as_deref().unwrap_or("#000000"),
            /* x */ pool.x,
            /* y */ pool.y,
            /* width */ pool.width,
            /* height */ pool.height,
        ));

        for (lane_id, lane) in pool.lanes.iter().enumerate() {
            if lane.name.is_none() {
                // Must be skipped for anonymous lanes. But this can only be if there is just one
                // lane in this pool.
                assert_eq!(pool.lanes.len(), 1);
                continue;
            }
            bpmn.push_str(&format!(
                r#"      <bpmndi:BPMNShape id="Lane_{pool_id}_{lane_id}_di" bpmnElement="Lane_{pool_id}_{lane_id}" isHorizontal="true" color:background-color="{}" color:border-color="{}">
        <dc:Bounds x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" />
      </bpmndi:BPMNShape>
"#,
                /* fill color */ lane.fill_color.as_deref().unwrap_or("#ffffff"),
                /* stroke color */ lane.stroke_color.as_deref().unwrap_or("#000000"),
                /* x */ lane.x,
                /* y */ lane.y,
                /* width */ lane.width,
                /* height */ lane.height,
            ));
        }
    }

    graph.nodes.iter().enumerate().for_each(|(node_id, node)| {
        let (width, height) = node.size();

        bpmn.push_str(&format!(
            r#"      <bpmndi:BPMNShape id="Node_{node_id}_di" bpmnElement="Node_{node_id}" {}>
        <dc:Bounds x="{:.2}" y="{:.2}" width="{}" height="{}" />
      </bpmndi:BPMNShape>
"#,
            AdditionalShapeInfo(node),
            node.x,
            node.y,
            width,
            height,
        ));
    });

    // Add BPMNEdge elements
    for (edge_id, edge) in graph.edges.iter().enumerate() {
        let EdgeType::Regular {
            bend_points: RegularEdgeBendPoints::FullyRouted(bend_points),
            ..
        } = &edge.edge_type
        else {
            dbg!("This should never be the case?");
            continue;
        };

        bpmn.push_str(&format!(
            "      <bpmndi:BPMNEdge id=\"Flow_{edge_id}_di\" bpmnElement=\"Flow_{edge_id}\" {}>\n",
            AdditionalEdgeShapeInfo(edge),
        ));

        bend_points.iter().for_each(|(x, y)| {
            bpmn.push_str(&format!(
                "        <di:waypoint x=\"{x:.2}\" y=\"{y:.2}\" />\n"
            ))
        });

        bpmn.push_str("      </bpmndi:BPMNEdge>\n");
    }

    //    for data_edge in graph.data_edges.iter() {
    //        let from = data_edge.from;
    //        let to = data_edge.to;
    //        if data_edge.is_reversed {
    //            bpmn.push_str(&format!(
    //                "      <bpmndi:BPMNEdge id=\"DataOutputAssociation_{from}_{to}_di\" bpmnElement=\"DataOutputAssociation_{from}_{to}\">"
    //            ));
    //        } else {
    //            bpmn.push_str(&format!(
    //                "      <bpmndi:BPMNEdge id=\"DataInputAssociation_{from}_{to}_di\" bpmnElement=\"DataInputAssociation_{from}_{to}\">"
    //            ));
    //        }
    //
    //        data_edge.bend_points.iter().flatten().for_each(|(x, y)| {
    //            bpmn.push_str(&format!(
    //                "        <di:waypoint x=\"{x:.2}\" y=\"{y:.2}\" />\n"
    //            ))
    //        });
    //
    //        bpmn.push_str("      </bpmndi:BPMNEdge>\n");
    //    }

    bpmn.push_str(
        r#"    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</bpmn:definitions>
"#,
    );

    bpmn
}

fn write_process_node(bpmn: &mut String, graph: &Graph, node_id: usize, node: &Node) {
    let NodeType::RealNode {
        event,
        display_text,
        ..
    } = &node.node_type
    else {
        return;
    };
    let incomingoutgoing = IncomingOutgoing {
        incoming: &node.incoming,
        outgoing: &node.outgoing,
        graph,
    };
    match event {
        BpmnNode::Data(_, _) => {
            unreachable!("This function is not called for data nodes, the caller guarantees this.")
        }
        // Gateways
        BpmnNode::Gateway(gt) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:{0}Gateway id="Node_{node_id}">
{incomingoutgoing}
    </bpmn:{0}Gateway>
"#,
                match gt {
                    crate::lexer::GatewayType::Exclusive => "exclusive",
                    crate::lexer::GatewayType::Parallel => "parallel",
                    crate::lexer::GatewayType::Inclusive => "inclusive",
                    crate::lexer::GatewayType::Event => "eventBased",
                }
            ));
        }

        // Activities
        BpmnNode::Activity(ActivityType::Task(TaskType::User)) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:userTask id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    </bpmn:userTask>
"#,
            ));
        }
        BpmnNode::Activity(ActivityType::Task(TaskType::Service)) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:serviceTask id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    </bpmn:serviceTask>
"#,
            ));
        }
        BpmnNode::Activity(ActivityType::Task(TaskType::Businessrule)) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:businessRuleTask id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    </bpmn:businessRuleTask>
"#,
            ));
        }
        BpmnNode::Activity(ActivityType::Task(TaskType::Script)) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:scriptTask id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    </bpmn:scriptTask>
"#,
            ));
        }
        BpmnNode::Activity(ActivityType::Task(TaskType::None)) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:task id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    </bpmn:task>
"#,
            ));
        }
        BpmnNode::Activity(ActivityType::Task(_)) => todo!(),

        // Tasks
        BpmnNode::Activity(ActivityType::Subprocess) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:subProcess id="Node_{node_id}" name="{display_text}" triggeredByEvent="false">
{incomingoutgoing}
    </bpmn:subProcess>
"#,
            ));
        }
        BpmnNode::Activity(ActivityType::CallActivity) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:callActivity id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    </bpmn:callActivity>
"#,
            ));
        }
        BpmnNode::Activity(ActivityType::EventSubprocess) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:subProcess id="Node_{node_id}" name="{display_text}" triggeredByEvent="true">
{incomingoutgoing}
    </bpmn:subProcess>
"#,
            ));
        }
        BpmnNode::Activity(ActivityType::Transaction) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:transaction id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    </bpmn:transaction>
"#,
            ));
        }

        // Start Events
        BpmnNode::Event(event_type, EventVisual::Start(_)) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:startEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
{}    </bpmn:startEvent>
"#,
                EventDefinition(event_type),
            ));
        }

        // Intermediate Events
        BpmnNode::Event(event_type, EventVisual::Throw) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:intermediateThrowEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
{}    </bpmn:intermediateThrowEvent>
"#,
                EventDefinition(event_type),
            ));
        }
        BpmnNode::Event(event_type, EventVisual::Catch(InterruptKind::Interrupting)) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:intermediateCatchEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
{}    </bpmn:intermediateCatchEvent>
"#,
                EventDefinition(event_type),
            ));
        }
        BpmnNode::Event(_, EventVisual::Catch(InterruptKind::NonInterrupting)) => todo!(),

        BpmnNode::Event(EventType::Blank, EventVisual::End) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:endEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    </bpmn:endEvent>
"#,
            ));
        }
        BpmnNode::Event(EventType::Error, EventVisual::End) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:endEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    <bpmn:errorEventDefinition />
    </bpmn:endEvent>
"#,
            ));
        }
        BpmnNode::Event(EventType::Cancel, EventVisual::End) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:endEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    <bpmn:cancelEventDefinition />
    </bpmn:endEvent>
"#,
            ));
        }
        BpmnNode::Event(EventType::Signal, EventVisual::End) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:endEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    <bpmn:signalEventDefinition />
    </bpmn:endEvent>
"#,
            ));
        }
        BpmnNode::Event(EventType::Message, EventVisual::End) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:endEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    <bpmn:messageEventDefinition />
    </bpmn:endEvent>
"#,
            ));
        }
        BpmnNode::Event(EventType::Termination, EventVisual::End) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:endEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    <bpmn:terminateEventDefinition />
    </bpmn:endEvent>
"#,
            ));
        }
        BpmnNode::Event(EventType::Escalation, EventVisual::End) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:endEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    <bpmn:escalationEventDefinition />
    </bpmn:endEvent>
"#,
            ));
        }
        BpmnNode::Event(EventType::Compensation, EventVisual::End) => {
            bpmn.push_str(&format!(
                r#"    <bpmn:endEvent id="Node_{node_id}" name="{display_text}">
{incomingoutgoing}
    <bpmn:compensateEventDefinition />
    </bpmn:endEvent>
"#,
            ));
        }
        /*
                // Boundary Events
                BpmnNode::BoundaryEvent(meta, attached_to, cancel_activity) => {
                    bpmn.push_str(&format!(
                                r#"    <bpmn:boundaryEvent id="Node_{node_id}" name="{display_text}" attachedToRef="Node_{attached_to}" cancelActivity="{}">
        {incomingoutgoing}
            </bpmn:boundaryEvent>
        "#,
                                meta,
                                if *cancel_activity { "true" } else { "false" },
                            ));
                }
                BpmnNode::BoundaryErrorEvent(meta, attached_to, cancel_activity) => {
                    bpmn.push_str(&format!(
                                r#"    <bpmn:boundaryEvent id="Node_{node_id}" name="{display_text}" attachedToRef="Node_{attached_to}" cancelActivity="{}">
        {incomingoutgoing}
            <bpmn:errorEventDefinition />
            </bpmn:boundaryEvent>
        "#,
                                meta,
                                if *cancel_activity { "true" } else { "false" },
                            ));
                }
                BpmnNode::BoundaryTimerEvent(meta, attached_to, cancel_activity) => {
                    bpmn.push_str(&format!(
                                r#"    <bpmn:boundaryEvent id="Node_{node_id}" name="{display_text}" attachedToRef="Node_{attached_to}" cancelActivity="{}">
        {incomingoutgoing}
            <bpmn:timerEventDefinition />
            </bpmn:boundaryEvent>
        "#,
                                meta,
                                if *cancel_activity { "true" } else { "false" },
                            ));
                }
                BpmnNode::BoundarySignalEvent(meta, attached_to, cancel_activity) => {
                    bpmn.push_str(&format!(
                                r#"    <bpmn:boundaryEvent id="Node_{node_id}" name="{display_text}" attachedToRef="Node_{attached_to}" cancelActivity="{}">
        {incomingoutgoing}
            <bpmn:signalEventDefinition />
            </bpmn:boundaryEvent>
        "#,
                                meta,
                                if *cancel_activity { "true" } else { "false" },
                            ));
                }
                BpmnNode::BoundaryMessageEvent(meta, attached_to, cancel_activity) => {
                    bpmn.push_str(&format!(
                                r#"    <bpmn:boundaryEvent id="Node_{node_id}" name="{display_text}" attachedToRef="Node_{attached_to}" cancelActivity="{}">
        {incomingoutgoing}
            <bpmn:messageEventDefinition />
            </bpmn:boundaryEvent>
        "#,
                                meta,
                                if *cancel_activity { "true" } else { "false" },
                            ));
                }
                BpmnNode::BoundaryEscalationEvent(meta, attached_to, cancel_activity) => {
                    bpmn.push_str(&format!(
                                r#"    <bpmn:boundaryEvent id="Node_{node_id}" name="{display_text}" attachedToRef="Node_{attached_to}" cancelActivity="{}">
        {incomingoutgoing}
            <bpmn:escalationEventDefinition />
            </bpmn:boundaryEvent>
        "#,
                                meta,
                                if *cancel_activity { "true" } else { "false" },
                            ));
                }
                BpmnNode::BoundaryConditionalEvent(meta, attached_to, cancel_activity) => {
                    bpmn.push_str(&format!(
                                r#"    <bpmn:boundaryEvent id="Node_{node_id}" name="{display_text}" attachedToRef="Node_{attached_to}" cancelActivity="{}">
        {incomingoutgoing}
            <bpmn:conditionalEventDefinition>
              <bpmn:condition xsi:type="bpmn:tFormalExpression">/* Your condition here */</bpmn:condition>
            </bpmn:conditionalEventDefinition>
            </bpmn:boundaryEvent>
"#,
                        meta,
                        if *cancel_activity { "true" } else { "false" },
                    ));
        }
        BpmnNode::BoundaryCompensationEvent(meta, attached_to) => {
            // Compensation boundary events are always non-interrupting
            bpmn.push_str(&format!(
                        r#"    <bpmn:boundaryEvent id="Node_{node_id}" name="{display_text}" attachedToRef="Node_{attached_to}" cancelActivity="false">
        {incomingoutgoing}
            <bpmn:compensateEventDefinition />
            </bpmn:boundaryEvent>
"#,
                        meta
                    ));
        }
*/

        // Default case
        _ => {}
    }
}

struct AdditionalShapeInfo<'a>(&'a Node);

impl Display for AdditionalShapeInfo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_gateway() {
            write!(f, " isMarkerVisible=\"true\"")?;
        }

        if let Some(stroke_color) = &self.0.stroke_color {
            write!(
                f,
                " bioc:stroke=\"{stroke_color}\" color:border-color=\"{stroke_color}\""
            )?;
        }
        if let Some(fill_color) = &self.0.fill_color {
            write!(
                f,
                "bioc:fill=\"{fill_color}\" color:background-color=\"{fill_color}\""
            )?;
        }

        Ok(())
    }
}

struct AdditionalEdgeShapeInfo<'a>(&'a Edge);

impl Display for AdditionalEdgeShapeInfo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(stroke_color) = &self.0.stroke_color {
            write!(
                f,
                " bioc:stroke=\"{stroke_color}\" color:border-color=\"{stroke_color}\""
            )?;
        }

        Ok(())
    }
}
