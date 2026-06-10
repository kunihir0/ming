pub mod detector;
pub mod error;
pub mod models;
pub mod services;

pub use detector::{TeamDetector, TeamDetectorBuilder, TeamDetectorConfig};
pub use error::{Result, TeamDetectorError};
pub use models::{GraphData, GraphEdge, GraphNode, Player};
