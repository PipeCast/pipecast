use crate::store::Store;
use anyhow::{anyhow, bail};
use enum_map::{Enum, EnumMap};
use pipewire::keys::{ACCESS, APP_NAME, AUDIO_CHANNEL, CLIENT_ID, DEVICE_DESCRIPTION, DEVICE_ID, DEVICE_NAME, DEVICE_NICK, FACTORY_NAME, FACTORY_TYPE_NAME, FACTORY_TYPE_VERSION, LINK_INPUT_NODE, LINK_INPUT_PORT, LINK_OUTPUT_NODE, LINK_OUTPUT_PORT, MODULE_ID, NODE_DESCRIPTION, NODE_ID, NODE_NAME, NODE_NICK, PORT_DIRECTION, PORT_ID, PORT_MONITOR, PORT_NAME, PROTOCOL, SEC_GID, SEC_PID, SEC_UID};
use pipewire::registry::Listener;
use pipewire::registry::Registry;
use pipewire::spa::utils::dict::DictRef;
use pipewire::types::ObjectType;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub(crate) struct PipewireRegistry {
    registry: Registry,
    store: Rc<RefCell<Store>>,

    // These two need to exist, if the Listeners are dropped they simply stop working.
    registry_listener: Option<Listener>,
    registry_removal_listener: Option<Listener>,
}

impl PipewireRegistry {
    pub fn new(registry: Registry, store: Rc<RefCell<Store>>) -> Self {
        let mut registry = Self {
            registry,
            store,
            registry_listener: None,
            registry_removal_listener: None,
        };

        registry.registry_listener = Some(registry.register_listener());
        registry.registry_removal_listener = Some(registry.registry_removal_listener());

        registry
    }

    pub fn register_listener(&self) -> Listener {
        let store = self.store.clone();
        self.registry
            .add_listener_local()
            .global(
                move |global| {
                    let id = global.id;

                    let mut store = store.borrow_mut();
                    match global.type_ {
                        ObjectType::Device => {
                            if let Some(props) = global.props {
                                // Create the Device
                                let device = RegistryDevice::from(props);
                                store.unmanaged_device_add(id, device);
                            }
                        }
                        ObjectType::Node => {
                            if let Some(props) = global.props {
                                if let Ok(node) = RegistryDeviceNode::try_from(props) {
                                    if let Some(device) = store.unmanaged_device_get(node.parent_id) {
                                        device.add_node(id);
                                        store.unmanaged_device_node_add(id, node);
                                    }
                                }
                                if let Ok(node) = RegistryClientNode::try_from(props) {
                                    if let Some(client) = store.unmanaged_client_get(node.parent_id) {
                                        client.add_node(id);
                                        store.unmanaged_client_node_add(id, node);
                                    }
                                }
                            }
                        }

                        ObjectType::Port => {
                            if let Some(props) = global.props {
                                let node_id = props.get(*NODE_ID);
                                let pid = props.get(*PORT_ID);
                                let name = props.get(*PORT_NAME);
                                let channel = props.get(*AUDIO_CHANNEL);
                                let direction = props.get(*PORT_DIRECTION);
                                let is_monitor = props.get(*PORT_MONITOR);

                                // Realistically, the only field that can be missing which we can infer
                                // a default from would be 'is_monitor'
                                if node_id.is_none() || pid.is_none() || name.is_none() || channel.is_none() || direction.is_none() {
                                    return;
                                }

                                // Ok, we can unwrap these vars
                                let name = name.unwrap();
                                let channel = channel.unwrap();

                                // Unwrap the Port Direction. Pipewire also supports 'notify' and
                                // 'control' ports, if we run into either of those, they're not
                                // useful here, so we'll ignore the port entirely
                                let direction = match direction.unwrap() {
                                    "in" => Direction::In,
                                    "out" => Direction::Out,
                                    _ => return
                                };

                                // Unwrap the Monitor boolean. This should be set, but if it's not
                                // we'll assume it's NOT a monitor port.
                                let is_monitor = if let Some(monitor) = is_monitor {
                                    monitor.parse::<bool>().unwrap_or_default()
                                } else {
                                    false
                                };

                                let port = RegistryPort::new(id, name, channel, is_monitor);

                                // We need to extract the NodeID and PortID from the data..
                                if let Some(node_id) = node_id.and_then(|s| s.parse::<u32>().ok()) {
                                    if let Some(port_id) = pid.and_then(|s| s.parse::<u32>().ok()) {
                                        if let Some(node) = store.unmanaged_device_node_get(node_id) {
                                            node.add_port(id, direction, port);

                                            store.unmanaged_node_check(node_id);
                                            return;
                                        }
                                        if let Some(node) = store.unmanaged_client_node_get(node_id) {
                                            node.add_port(port_id, direction, port);
                                        }
                                    }
                                }
                            }
                        }

                        ObjectType::Link => {
                            // We need to track links, to allow callbacks when links are created.
                            if let Some(props) = global.props {
                                if let Ok(link) = RegistryLink::try_from(props) {
                                    let input_node = link.input_node;
                                    let output_node = link.output_node;

                                    store.unmanaged_link_add(id, link);
                                    store.unmanaged_client_node_check(input_node);
                                    store.unmanaged_client_node_check(output_node);
                                }
                            }
                        }
                        ObjectType::Factory => {
                            if let Some(props) = global.props {
                                if let Ok(factory) = RegistryFactory::try_from(props) {
                                    store.factory_add(id, factory);
                                }
                            }
                        }
                        ObjectType::Client => {
                            if let Some(props) = global.props {
                                if let Ok(client) = RegistryClient::try_from(props) {
                                    store.unmanaged_client_add(id, client);
                                }
                            }
                        }
                        // ObjectType::ClientEndpoint => {}
                        // ObjectType::ClientNode => {}
                        // ObjectType::ClientSession => {}
                        // ObjectType::Core => {}
                        // ObjectType::Endpoint => {}
                        // ObjectType::EndpointLink => {}
                        // ObjectType::EndpointStream => {}
                        // ObjectType::Metadata => {}
                        // ObjectType::Module => {}
                        // ObjectType::Profiler => {}
                        // ObjectType::Registry => {}
                        // ObjectType::Session => {}
                        // ObjectType::Other(_) => {}

                        _ => {
                            //debug!("Unmonitored Global Type: {} - {}", global.type_, global.id);
                        }
                    }
                }
            )
            .register()
    }

    pub fn registry_removal_listener(&self) -> Listener {
        let store = self.store.clone();
        self.registry
            .add_listener_local()
            .global_remove(move |id| {
                store.borrow_mut().remove_by_id(id);
            })
            .register()
    }

    pub fn destroy_global(&self, id: u32) {
        self.registry.destroy_global(id);
    }
}

#[derive(Debug)]
pub(crate) struct RegistryFactory {
    pub(crate) module_id: u32,

    pub(crate) name: String,
    pub(crate) factory_type: ObjectType,
    pub(crate) version: u32,
}

impl TryFrom<&DictRef> for RegistryFactory {
    type Error = anyhow::Error;

    fn try_from(value: &DictRef) -> Result<Self, Self::Error> {
        let module_id = value.get(*MODULE_ID).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("MODULE_ID"))?;
        let name = value.get(*FACTORY_NAME).map(|s| s.to_string()).ok_or_else(|| anyhow!("FACTORY_NAME"))?;
        let factory_type = value.get(*FACTORY_TYPE_NAME).ok_or_else(|| anyhow!("FACTORY_TYPE_NAME"))?;
        let version = value.get(*FACTORY_TYPE_VERSION).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("FACTORY_VERSION"))?;

        Ok(RegistryFactory {
            module_id,
            name,
            factory_type: to_object_type(factory_type),
            version,
        })
    }
}

#[derive(Debug)]
pub(crate) struct RegistryDevice {
    nickname: Option<String>,
    description: Option<String>,
    name: Option<String>,

    pub(crate) nodes: Vec<u32>,
}

impl From<&DictRef> for RegistryDevice {
    fn from(value: &DictRef) -> Self {
        let nickname = value.get(*DEVICE_NICK).map(|s| s.to_string());
        let description = value.get(*DEVICE_DESCRIPTION).map(|s| s.to_string());
        let name = value.get(*DEVICE_NAME).map(|s| s.to_string());

        Self {
            nickname,
            description,
            name,
            nodes: vec![],
        }
    }
}

impl RegistryDevice {
    pub fn add_node(&mut self, id: u32) {
        self.nodes.push(id);
    }
}

#[derive(Debug, Enum)]
pub(crate) enum Direction {
    In,
    Out,
}

#[derive(Debug)]
pub(crate) struct RegistryDeviceNode {
    pub parent_id: u32,

    pub nickname: Option<String>,
    pub description: Option<String>,
    pub name: Option<String>,

    pub ports: EnumMap<Direction, HashMap<u32, RegistryPort>>,
}

impl TryFrom<&DictRef> for RegistryDeviceNode {
    type Error = anyhow::Error;

    fn try_from(value: &DictRef) -> Result<Self, Self::Error> {
        let device = value.get(*DEVICE_ID);
        let nickname = value.get(*NODE_NICK).map(|s| s.to_string());
        let description = value.get(*NODE_DESCRIPTION).map(|s| s.to_string());
        let name = value.get(*NODE_NAME).map(|s| s.to_string());

        if let Some(device_id) = device.and_then(|s| s.parse::<u32>().ok()) {
            return Ok(Self {
                parent_id: device_id,
                nickname,
                description,
                name,
                ports: Default::default(),
            });
        }
        bail!("Device ID Missing");
    }
}

impl RegistryDeviceNode {
    pub(crate) fn add_port(&mut self, id: u32, direction: Direction, port: RegistryPort) {
        self.ports[direction].insert(id, port);
    }
}

#[derive(Debug)]
pub(crate) struct RegistryPort {
    pub global_id: u32,
    pub name: String,
    pub channel: String,
    pub is_monitor: bool,
}

impl RegistryPort {
    pub fn new(id: u32, name: &str, channel: &str, is_monitor: bool) -> Self {
        let name = name.to_string();
        let channel = channel.to_string();

        Self {
            global_id: id,
            name,
            channel,
            is_monitor,
        }
    }
}


pub(crate) struct RegistryClient {
    module_id: u32,
    protocol: String,
    process_id: u32,
    user_id: u32,
    group_id: u32,
    access: String,
    application_name: String,

    pub(crate) nodes: Vec<u32>,
}

impl RegistryClient {
    pub fn add_node(&mut self, id: u32) {
        self.nodes.push(id);
    }
}

impl TryFrom<&DictRef> for RegistryClient {
    type Error = anyhow::Error;

    fn try_from(value: &DictRef) -> Result<Self, Self::Error> {
        // I currently expect all these fields to be present for general usage
        let module_id = value.get(*MODULE_ID).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("MODULE_ID"))?;
        let protocol = value.get(*PROTOCOL).map(|s| s.to_string()).ok_or_else(|| anyhow!("PROTOCOL"))?;
        let process_id = value.get(*SEC_PID).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("SEC_PID"))?;
        let user_id = value.get(*SEC_UID).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("SEC_UID"))?;
        let group_id = value.get(*SEC_GID).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("SEC_GID"))?;
        let access = value.get(*ACCESS).map(|s| s.to_string()).ok_or_else(|| anyhow!("ACCESS"))?;
        let application_name = value.get(*APP_NAME).map(|s| s.to_string()).ok_or_else(|| anyhow!("APP_NAME"))?;

        Ok(Self {
            module_id,
            protocol,
            process_id,
            user_id,
            group_id,
            access,
            application_name,
            nodes: vec![],
        })
    }
}

#[derive(Debug)]
pub(crate) struct RegistryClientNode {
    pub(crate) parent_id: u32,

    pub(crate) application_name: String,
    pub(crate) node_name: String,

    pub ports: EnumMap<Direction, HashMap<u32, RegistryPort>>,
}

impl TryFrom<&DictRef> for RegistryClientNode {
    type Error = anyhow::Error;

    fn try_from(value: &DictRef) -> Result<Self, Self::Error> {
        let parent_id = value.get(*CLIENT_ID).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("CLIENT_ID"))?;
        let node_name = value.get(*NODE_NAME).map(|s| s.to_string()).ok_or_else(|| anyhow!("NODE_NAME"))?;
        let application_name = value.get("application.name").map(|s| s.to_string()).ok_or_else(|| anyhow!("APPLICATION_NAME"))?;

        Ok(Self {
            parent_id,
            application_name,
            node_name,

            ports: Default::default(),
        })
    }
}

impl RegistryClientNode {
    pub(crate) fn add_port(&mut self, id: u32, direction: Direction, port: RegistryPort) {
        self.ports[direction].insert(id, port);
    }
}


#[derive(Debug, PartialEq)]
pub(crate) struct RegistryLink {
    pub input_node: u32,
    pub input_port: u32,
    pub output_node: u32,
    pub output_port: u32,
}

impl TryFrom<&DictRef> for RegistryLink {
    type Error = anyhow::Error;

    fn try_from(value: &DictRef) -> Result<Self, Self::Error> {
        let input_node = value.get(*LINK_INPUT_NODE).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("LINK_INPUT_NODE"))?;
        let input_port = value.get(*LINK_INPUT_PORT).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("LINK_INPUT_PORT"))?;
        let output_node = value.get(*LINK_OUTPUT_NODE).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("LINK_OUTPUT_NODE"))?;
        let output_port = value.get(*LINK_OUTPUT_PORT).and_then(|s| s.parse::<u32>().ok()).ok_or_else(|| anyhow!("LINK_OUTPUT_PORT"))?;

        Ok(RegistryLink {
            input_node,
            input_port,
            output_node,
            output_port,
        })
    }
}

// pipewire-rs doesn't seem to provide one of these, it does have from_str and to_str, but they're
// crate public, so we can't use them, and they're only looking for the last chunk.
fn to_object_type(input: &str) -> ObjectType {
    match input {
        "PipeWire:Interface:Client" => ObjectType::Client,
        "PipeWire:Interface:ClientEndpoint" => ObjectType::ClientEndpoint,
        "PipeWire:Interface:ClientNode" => ObjectType::ClientNode,
        "PipeWire:Interface:ClientSession" => ObjectType::ClientSession,
        "PipeWire:Interface:Core" => ObjectType::Core,
        "PipeWire:Interface:Device" => ObjectType::Device,
        "PipeWire:Interface:Endpoint" => ObjectType::Endpoint,
        "PipeWire:Interface:EndpointLink" => ObjectType::EndpointLink,
        "PipeWire:Interface:EndpointStream" => ObjectType::EndpointStream,
        "PipeWire:Interface:Factory" => ObjectType::Factory,
        "PipeWire:Interface:Link" => ObjectType::Link,
        "PipeWire:Interface:Metadata" => ObjectType::Metadata,
        "PipeWire:Interface:Module" => ObjectType::Module,
        "PipeWire:Interface:Node" => ObjectType::Node,
        "PipeWire:Interface:Port" => ObjectType::Port,
        "PipeWire:Interface:Profiler" => ObjectType::Profiler,
        "PipeWire:Interface:Registry" => ObjectType::Registry,
        "PipeWire:Interface:Session" => ObjectType::Session,
        _ => ObjectType::Other(input.to_string())
    }
}