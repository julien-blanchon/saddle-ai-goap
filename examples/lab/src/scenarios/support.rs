use bevy::prelude::*;

use crate::LabOverlay;

pub(super) fn overlay_text(world: &mut World) -> Option<String> {
    let mut query = world.query_filtered::<&Text, With<LabOverlay>>();
    query.iter(world).next().map(|text| text.0.clone())
}
