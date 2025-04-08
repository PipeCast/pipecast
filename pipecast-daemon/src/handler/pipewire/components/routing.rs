use crate::handler::pipewire::components::links::LinkManagement;
use crate::handler::pipewire::components::mute::MuteManager;
use crate::handler::pipewire::components::node::NodeManagement;
use crate::handler::pipewire::components::profile::ProfileManagement;
use crate::handler::pipewire::manager::PipewireManager;
use anyhow::{anyhow, bail, Result};
use log::{debug, warn};
use pipecast_shared::{Mix, NodeType};
use ulid::Ulid;

pub(crate) trait RoutingManagement {
    async fn routing_load(&mut self) -> Result<()>;

    async fn routing_set_route(&mut self, source: Ulid, target: Ulid, enabled: bool) -> Result<()>;
    async fn routing_route_exists(&self, source: Ulid, target: Ulid) -> Result<bool>;
}

impl RoutingManagement for PipewireManager {
    async fn routing_load(&mut self) -> Result<()> {
        // This should be called after all the nodes are set up, we need to check our routing table
        // and establish links between the sources and targets
        debug!("Loading Routing..");

        let routing = &self.profile.routes;
        for (source, targets) in routing {
            for target in targets {
                let target_node = self.get_target_filter_node(*target)?;
                if !self.is_source_muted_to_some(*source, *target).await? {
                    if let Some(map) = self.source_map.get(source).copied() {
                        // Grab the Mix to Route From
                        let mix = self.get_target_mix(target).await?;
                        self.link_create_filter_to_filter(map[mix], target_node).await?;
                    }
                }
            }
        }

        Ok(())
    }


    async fn routing_set_route(&mut self, source: Ulid, target: Ulid, enabled: bool) -> Result<()> {
        // This is actually more complicated that it sounds, first lets find some stuff out..
        let source_type = self.get_node_type(source).ok_or(anyhow!("Source Not Found"))?;
        let target_type = self.get_node_type(target).ok_or(anyhow!("Target Not Found"))?;

        // Make sure the user is being sane
        if !matches!(source_type, NodeType::PhysicalSource | NodeType::VirtualSource) {
            bail!("Provided Source is a Target Node");
        }
        if !matches!(target_type, NodeType::PhysicalTarget | NodeType::VirtualTarget) {
            bail!("Provided Target is a Source Node");
        }

        // This should already be here, but it's not, so create it
        let target_id = self.get_target_filter_node(target)?;
        if self.profile.routes.get(&source).is_none() {
            warn!("[Routing] Table Missing for Source {}, Creating", source);
            self.profile.routes.insert(source, Default::default());
        }

        // This unwrap is safe, so just grab the Set and check what we're doing
        let route = self.profile.routes.get_mut(&source).unwrap();
        if enabled == route.contains(&target) {
            bail!("Requested route change already set");
        }
        if enabled { route.insert(target); } else { route.remove(&target); }

        // Next, we need to get the A/B IDs for the Source
        if let Some(map) = self.source_map.get(&source).copied() {
            // Set up the Pipewire Links
            if enabled {
                // Only create the route if it's not currently muted
                if !self.is_source_muted_to_some(source, target).await? {
                    let mix = self.get_target_mix(&target).await?;
                    self.link_create_filter_to_filter(map[mix], target_id).await?;
                }
            } else {
                let mix = self.get_target_mix(&target).await?;
                self.link_remove_filter_to_filter(map[mix], target_id).await?;
            }
        } else {
            bail!("Unable to obtain volume map for Source");
        }


        Ok(())
    }

    async fn routing_route_exists(&self, source: Ulid, target: Ulid) -> Result<bool> {
        let source_type = self.get_node_type(source).ok_or(anyhow!("Source Not Found"))?;
        let target_type = self.get_node_type(target).ok_or(anyhow!("Target Not Found"))?;

        // Make sure the user is being sane
        if !matches!(source_type, NodeType::PhysicalSource | NodeType::VirtualSource) {
            bail!("Provided Source is a Target Node");
        }
        if !matches!(target_type, NodeType::PhysicalTarget | NodeType::VirtualTarget) {
            bail!("Provided Target is a Source Node");
        }

        if !self.profile.routes.contains_key(&source) {
            return Ok(false);
        }

        Ok(self.profile.routes.get(&source).unwrap().contains(&target))
    }
}

trait RoutingManagementLocal {
    async fn get_target_mix(&self, id: &Ulid) -> Result<Mix>;
}

impl RoutingManagementLocal for PipewireManager {
    async fn get_target_mix(&self, id: &Ulid) -> Result<Mix> {
        let error = anyhow!("Cannot Locate Node");
        let node_type = self.get_node_type(*id).ok_or(error)?;
        if !matches!(node_type, NodeType::PhysicalTarget | NodeType::VirtualTarget) {
            bail!("Provided Target is a Source Node");
        }

        let err = anyhow!("Failed to Locate Target");
        let mix = if node_type == NodeType::PhysicalTarget {
            self.get_physical_target(*id).ok_or(err)?.mix
        } else {
            self.get_virtual_target(*id).ok_or(err)?.mix
        };
        Ok(mix)
    }
}