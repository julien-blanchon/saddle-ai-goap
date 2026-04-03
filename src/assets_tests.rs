use super::*;
use crate::{
    ActionDefinition, ActionId, FactCondition, FactEffect, GoalDefinition, GoalId,
    GoapDomainDefinition, GoapLibrary,
};

#[test]
fn goap_asset_round_trips_through_ron() {
    let mut definition = GoapDomainDefinition::new("asset_roundtrip");
    let done = definition.add_bool_key("done", Some("whether work is complete".into()), Some(false));
    definition.add_goal(
        GoalDefinition::new(GoalId(0), "finish work")
            .with_priority(10)
            .with_desired_state([FactCondition::equals_bool(done, true)]),
    );
    definition.add_action(
        ActionDefinition::new(ActionId(0), "finish work", "finish_work")
            .with_effects([FactEffect::set_bool(done, true)]),
    );

    let asset = GoapDomainAsset::from(definition);
    let serialized = ron::ser::to_string(&asset).unwrap();
    let decoded: GoapDomainAsset = ron::de::from_str(&serialized).unwrap();

    let mut library = GoapLibrary::default();
    let domain_id = decoded.register(&mut library);
    let definition = library.domain(domain_id).unwrap();

    assert_eq!(definition.name, "asset_roundtrip");
    assert_eq!(definition.goals.len(), 1);
    assert_eq!(definition.actions.len(), 1);
}
