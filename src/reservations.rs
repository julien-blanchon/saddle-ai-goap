use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::definitions::{ActionId, GoapDomainId};
use crate::world_state::TargetToken;

/// Policy controlling how target reservations influence planning.
#[derive(Debug, Clone, PartialEq, Reflect, Serialize, Deserialize)]
pub struct ReservationPolicy {
    /// Extra cost added to a target candidate when another agent already reserves it.
    pub cost_penalty: u32,
    /// If true, reserved targets are excluded entirely from planning rather than
    /// just penalized.
    pub hard_block: bool,
    /// Optional time-to-live in seconds. Reservations older than this are
    /// automatically reaped during cleanup. `None` means reservations persist
    /// until explicitly released.
    pub ttl_seconds: Option<f32>,
}

impl Default for ReservationPolicy {
    fn default() -> Self {
        Self {
            cost_penalty: 100,
            hard_block: false,
            ttl_seconds: Some(10.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Reflect)]
pub struct ReservationEntry {
    pub entity: Entity,
    pub action_id: ActionId,
    pub reserved_at: f32,
}

/// Shared resource tracking which targets are reserved by which agents, per domain.
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct GoapReservationMap {
    #[reflect(ignore)]
    reservations: HashMap<GoapDomainId, HashMap<TargetToken, ReservationEntry>>,
}

impl GoapReservationMap {
    pub fn reserve(
        &mut self,
        domain: GoapDomainId,
        token: TargetToken,
        entry: ReservationEntry,
    ) {
        self.reservations.entry(domain).or_default().insert(token, entry);
    }

    pub fn release(&mut self, domain: GoapDomainId, token: TargetToken) {
        if let Some(domain_map) = self.reservations.get_mut(&domain) {
            domain_map.remove(&token);
        }
    }

    pub fn release_all_for_entity(&mut self, domain: GoapDomainId, entity: Entity) {
        if let Some(domain_map) = self.reservations.get_mut(&domain) {
            domain_map.retain(|_, entry| entry.entity != entity);
        }
    }

    pub fn is_reserved_by_other(
        &self,
        domain: GoapDomainId,
        token: &TargetToken,
        entity: Entity,
    ) -> bool {
        self.reservations
            .get(&domain)
            .and_then(|m| m.get(token))
            .is_some_and(|entry| entry.entity != entity)
    }

    pub fn get(
        &self,
        domain: GoapDomainId,
        token: &TargetToken,
    ) -> Option<&ReservationEntry> {
        self.reservations.get(&domain).and_then(|m| m.get(token))
    }

    pub fn cleanup_stale(&mut self, domain: GoapDomainId, policy: &ReservationPolicy, now: f32) {
        let Some(ttl) = policy.ttl_seconds else {
            return;
        };
        if let Some(domain_map) = self.reservations.get_mut(&domain) {
            domain_map.retain(|_, entry| now - entry.reserved_at < ttl);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(entity: Entity) -> ReservationEntry {
        ReservationEntry {
            entity,
            action_id: ActionId(0),
            reserved_at: 0.0,
        }
    }

    #[test]
    fn reserve_and_query() {
        let mut map = GoapReservationMap::default();
        let domain = GoapDomainId(0);
        let token = TargetToken(42);
        let e1 = Entity::from_raw_u32(1).unwrap();
        let e2 = Entity::from_raw_u32(2).unwrap();

        map.reserve(domain, token, entry(e1));
        assert!(!map.is_reserved_by_other(domain, &token, e1));
        assert!(map.is_reserved_by_other(domain, &token, e2));
    }

    #[test]
    fn release_clears_reservation() {
        let mut map = GoapReservationMap::default();
        let domain = GoapDomainId(0);
        let token = TargetToken(42);
        let e1 = Entity::from_raw_u32(1).unwrap();

        map.reserve(domain, token, entry(e1));
        map.release(domain, token);
        assert!(!map.is_reserved_by_other(domain, &token, Entity::from_raw_u32(99).unwrap()));
    }

    #[test]
    fn release_all_for_entity_clears_only_that_entity() {
        let mut map = GoapReservationMap::default();
        let domain = GoapDomainId(0);
        let e1 = Entity::from_raw_u32(1).unwrap();
        let e2 = Entity::from_raw_u32(2).unwrap();

        map.reserve(domain, TargetToken(1), entry(e1));
        map.reserve(domain, TargetToken(2), entry(e2));
        map.release_all_for_entity(domain, e1);

        assert!(map.get(domain, &TargetToken(1)).is_none());
        assert!(map.get(domain, &TargetToken(2)).is_some());
    }

    #[test]
    fn ttl_cleanup_removes_stale_entries() {
        let mut map = GoapReservationMap::default();
        let domain = GoapDomainId(0);
        let e = Entity::from_raw_u32(1).unwrap();

        map.reserve(
            domain,
            TargetToken(1),
            ReservationEntry {
                entity: e,
                action_id: ActionId(0),
                reserved_at: 0.0,
            },
        );
        map.reserve(
            domain,
            TargetToken(2),
            ReservationEntry {
                entity: e,
                action_id: ActionId(0),
                reserved_at: 9.0,
            },
        );

        let policy = ReservationPolicy {
            ttl_seconds: Some(5.0),
            ..Default::default()
        };
        map.cleanup_stale(domain, &policy, 10.0);

        assert!(map.get(domain, &TargetToken(1)).is_none()); // 10 - 0 = 10 >= 5
        assert!(map.get(domain, &TargetToken(2)).is_some()); // 10 - 9 = 1 < 5
    }
}
