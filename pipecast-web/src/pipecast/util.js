import {store} from "@/pipecast/store.js";

export const DeviceType = Object.freeze({
  PhysicalSource: 'PhysicalSource',
  VirtualSource: 'VirtualSource',

  PhysicalTarget: 'PhysicalTarget',
  VirtualTarget: 'VirtualTarget',
});

export function get_devices(type) {
  if (type === DeviceType.PhysicalSource) {
    return store.getProfile().devices.sources.physical_devices;
  }
  if (type === DeviceType.VirtualSource) {
    return store.getProfile().devices.sources.virtual_devices;
  }
  if (type === DeviceType.PhysicalTarget) {
    return store.getProfile().devices.targets.physical_devices;
  }
  if (type === DeviceType.VirtualTarget) {
    return store.getProfile().devices.targets.virtual_devices;
  }
}

export function is_source(type) {
  return (type === DeviceType.PhysicalSource || type === DeviceType.VirtualSource)
}
