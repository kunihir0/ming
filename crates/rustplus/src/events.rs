use crate::proto::AppMarker;
use std::time::{Duration, Instant};

/// The single output type of the whole monitoring system.
#[derive(Debug, Clone)]
pub enum RustEvent {
    CargoSpawned,
    CargoDespawned { was_out_for: Duration },
    CargoEgress { spawned_at: Instant },
    HeliSpawned,
    HeliDespawned { was_out_for: Duration },
    HeliTakenDown { last_position: (f32, f32) },
    OilRigCrateDropped { unlock_at: Instant },
    OilRigCrateLooted,
    Ch47Entered,
    Ch47Left,
    VendingMachineNew { position: (f32, f32), id: u32 },
    MarkerSnapshot(Vec<AppMarker>),
}

/// Interface for entity modules to implement.
pub trait EventMonitor: Send + Sync {
    fn on_tick(&mut self, markers: &[AppMarker], emit: &mut dyn FnMut(RustEvent));
}
