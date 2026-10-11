use std::sync::Arc;

use nih_plug::prelude::Editor;

use crate::params::AirZoneParams;
use crate::shared::Shared;

pub fn create(_params: Arc<AirZoneParams>, _shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    None
}
