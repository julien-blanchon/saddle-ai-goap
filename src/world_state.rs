use std::hash::{Hash, Hasher};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub struct WorldKeyId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub enum FactValueType {
    Bool,
    Int,
    Target,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub struct TargetToken(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub enum FactValue {
    Bool(bool),
    Int(i32),
    Target(TargetToken),
}

impl std::fmt::Display for FactValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Int(value) => write!(f, "{value}"),
            Self::Target(value) => write!(f, "target({})", value.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub struct WorldKeyDefinition {
    pub id: WorldKeyId,
    pub name: String,
    pub value_type: FactValueType,
    pub description: Option<String>,
    pub default_value: Option<FactValue>,
}

#[derive(Debug, Clone, Default, PartialEq, Reflect, Serialize, Deserialize)]
pub struct WorldStateSchema {
    pub keys: Vec<WorldKeyDefinition>,
}

impl WorldStateSchema {
    pub fn add_bool_key(
        &mut self,
        name: impl Into<String>,
        description: impl Into<Option<String>>,
        default_value: Option<bool>,
    ) -> WorldKeyId {
        self.add_key(
            name,
            FactValueType::Bool,
            description,
            default_value.map(FactValue::Bool),
        )
    }

    pub fn add_int_key(
        &mut self,
        name: impl Into<String>,
        description: impl Into<Option<String>>,
        default_value: Option<i32>,
    ) -> WorldKeyId {
        self.add_key(
            name,
            FactValueType::Int,
            description,
            default_value.map(FactValue::Int),
        )
    }

    pub fn add_target_key(
        &mut self,
        name: impl Into<String>,
        description: impl Into<Option<String>>,
        default_value: Option<TargetToken>,
    ) -> WorldKeyId {
        self.add_key(
            name,
            FactValueType::Target,
            description,
            default_value.map(FactValue::Target),
        )
    }

    pub fn key(&self, id: WorldKeyId) -> Option<&WorldKeyDefinition> {
        self.keys.get(id.0)
    }

    pub fn key_name(&self, id: WorldKeyId) -> &str {
        self.key(id)
            .map(|definition| definition.name.as_str())
            .unwrap_or("<unknown>")
    }

    pub fn default_state(&self) -> GoapWorldState {
        let mut state = GoapWorldState::with_capacity(self.keys.len());
        for definition in &self.keys {
            state.set_raw(definition.id, definition.default_value.clone());
        }
        state
    }

    fn add_key(
        &mut self,
        name: impl Into<String>,
        value_type: FactValueType,
        description: impl Into<Option<String>>,
        default_value: Option<FactValue>,
    ) -> WorldKeyId {
        let id = WorldKeyId(self.keys.len());
        self.keys.push(WorldKeyDefinition {
            id,
            name: name.into(),
            value_type,
            description: description.into(),
            default_value,
        });
        id
    }
}

// ---------------------------------------------------------------------------
// Zobrist hashing helpers
// ---------------------------------------------------------------------------

/// Fast integer hash finalizer from SplitMix64.
#[inline]
const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

const ZOBRIST_NONE: u64 = 0xd5a8_a733_9988_4177;
const ZOBRIST_BOOL_FALSE: u64 = 0x3c6e_f372_fe94_f82a;
const ZOBRIST_BOOL_TRUE: u64 = 0xa54f_f53a_5f1d_36f1;
const ZOBRIST_TARGET_MIX: u64 = 0x8bb2_4c37_e9a0_d1ed;

#[inline]
fn zobrist_slot_hash(index: usize, value: &Option<FactValue>) -> u64 {
    let key_mix = splitmix64(index as u64);
    let value_mix = match value {
        None => ZOBRIST_NONE,
        Some(FactValue::Bool(false)) => ZOBRIST_BOOL_FALSE,
        Some(FactValue::Bool(true)) => ZOBRIST_BOOL_TRUE,
        Some(FactValue::Int(v)) => splitmix64(*v as u64),
        Some(FactValue::Target(t)) => splitmix64(t.0 ^ ZOBRIST_TARGET_MIX),
    };
    key_mix ^ value_mix
}

fn compute_full_hash(values: &[Option<FactValue>]) -> u64 {
    values
        .iter()
        .enumerate()
        .fold(0u64, |acc, (i, v)| acc ^ zobrist_slot_hash(i, v))
}

// ---------------------------------------------------------------------------
// Serde helper for GoapWorldState round-trip
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GoapWorldStateRaw {
    values: Vec<Option<FactValue>>,
}

impl From<GoapWorldStateRaw> for GoapWorldState {
    fn from(raw: GoapWorldStateRaw) -> Self {
        let cached_hash = compute_full_hash(&raw.values);
        Self {
            values: raw.values,
            cached_hash,
        }
    }
}

// ---------------------------------------------------------------------------
// GoapWorldState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Reflect, Serialize)]
#[serde(into = "GoapWorldStateSerdeOut")]
#[reflect(PartialEq, Hash)]
pub struct GoapWorldState {
    pub values: Vec<Option<FactValue>>,
    #[serde(skip)]
    #[reflect(ignore)]
    cached_hash: u64,
}

// Serialize outputs only the values field, matching the original wire format.
#[derive(Serialize)]
struct GoapWorldStateSerdeOut {
    values: Vec<Option<FactValue>>,
}

impl From<GoapWorldState> for GoapWorldStateSerdeOut {
    fn from(state: GoapWorldState) -> Self {
        Self {
            values: state.values,
        }
    }
}

impl<'de> Deserialize<'de> for GoapWorldState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = GoapWorldStateRaw::deserialize(deserializer)?;
        Ok(Self::from(raw))
    }
}

impl PartialEq for GoapWorldState {
    fn eq(&self, other: &Self) -> bool {
        if self.cached_hash != other.cached_hash {
            return false;
        }
        self.values == other.values
    }
}

impl Eq for GoapWorldState {}

impl Hash for GoapWorldState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.cached_hash.hash(state);
    }
}

impl GoapWorldState {
    pub fn with_capacity(keys: usize) -> Self {
        let values = vec![None; keys];
        let cached_hash = compute_full_hash(&values);
        Self {
            values,
            cached_hash,
        }
    }

    pub fn ensure_len(&mut self, len: usize) {
        if self.values.len() < len {
            let old_len = self.values.len();
            self.values.resize(len, None);
            for i in old_len..len {
                self.cached_hash ^= zobrist_slot_hash(i, &None);
            }
        }
    }

    pub fn set_raw(&mut self, key: WorldKeyId, value: Option<FactValue>) {
        self.ensure_len(key.0 + 1);
        let old = &self.values[key.0];
        self.cached_hash ^= zobrist_slot_hash(key.0, old);
        self.values[key.0] = value;
        self.cached_hash ^= zobrist_slot_hash(key.0, &self.values[key.0]);
    }

    pub fn clear(&mut self, key: WorldKeyId) {
        self.set_raw(key, None);
    }

    pub fn set_bool(&mut self, key: WorldKeyId, value: bool) {
        self.set_raw(key, Some(FactValue::Bool(value)));
    }

    pub fn set_int(&mut self, key: WorldKeyId, value: i32) {
        self.set_raw(key, Some(FactValue::Int(value)));
    }

    pub fn set_target(&mut self, key: WorldKeyId, value: TargetToken) {
        self.set_raw(key, Some(FactValue::Target(value)));
    }

    pub fn get(&self, key: WorldKeyId) -> Option<&FactValue> {
        self.values.get(key.0).and_then(Option::as_ref)
    }

    pub fn get_bool(&self, key: WorldKeyId) -> Option<bool> {
        match self.get(key) {
            Some(FactValue::Bool(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn get_int(&self, key: WorldKeyId) -> Option<i32> {
        match self.get(key) {
            Some(FactValue::Int(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn get_target(&self, key: WorldKeyId) -> Option<TargetToken> {
        match self.get(key) {
            Some(FactValue::Target(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn overlay(&mut self, other: &GoapWorldState) {
        self.ensure_len(other.values.len());
        for (index, value) in other.values.iter().enumerate() {
            if value.is_some() {
                self.cached_hash ^= zobrist_slot_hash(index, &self.values[index]);
                self.values[index] = value.clone();
                self.cached_hash ^= zobrist_slot_hash(index, &self.values[index]);
            }
        }
    }

    pub fn describe(&self, schema: &WorldStateSchema) -> Vec<(String, String)> {
        schema
            .keys
            .iter()
            .filter_map(|definition| {
                self.get(definition.id)
                    .map(|value| (definition.name.clone(), value.to_string()))
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub enum FactComparison {
    Equals(FactValue),
    NotEquals(FactValue),
    GreaterOrEqual(i32),
    LessOrEqual(i32),
    IsSet,
    IsUnset,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub struct FactCondition {
    pub key: WorldKeyId,
    pub comparison: FactComparison,
}

impl FactCondition {
    pub fn equals_bool(key: WorldKeyId, value: bool) -> Self {
        Self {
            key,
            comparison: FactComparison::Equals(FactValue::Bool(value)),
        }
    }

    pub fn equals_int(key: WorldKeyId, value: i32) -> Self {
        Self {
            key,
            comparison: FactComparison::Equals(FactValue::Int(value)),
        }
    }

    pub fn equals_target(key: WorldKeyId, value: TargetToken) -> Self {
        Self {
            key,
            comparison: FactComparison::Equals(FactValue::Target(value)),
        }
    }

    pub fn int_at_least(key: WorldKeyId, value: i32) -> Self {
        Self {
            key,
            comparison: FactComparison::GreaterOrEqual(value),
        }
    }

    pub fn int_at_most(key: WorldKeyId, value: i32) -> Self {
        Self {
            key,
            comparison: FactComparison::LessOrEqual(value),
        }
    }

    pub fn is_set(key: WorldKeyId) -> Self {
        Self {
            key,
            comparison: FactComparison::IsSet,
        }
    }

    pub fn is_unset(key: WorldKeyId) -> Self {
        Self {
            key,
            comparison: FactComparison::IsUnset,
        }
    }

    pub fn matches(&self, state: &GoapWorldState) -> bool {
        match &self.comparison {
            FactComparison::Equals(expected) => state.get(self.key) == Some(expected),
            FactComparison::NotEquals(expected) => state.get(self.key) != Some(expected),
            FactComparison::GreaterOrEqual(expected) => state
                .get_int(self.key)
                .is_some_and(|value| value >= *expected),
            FactComparison::LessOrEqual(expected) => state
                .get_int(self.key)
                .is_some_and(|value| value <= *expected),
            FactComparison::IsSet => state.get(self.key).is_some(),
            FactComparison::IsUnset => state.get(self.key).is_none(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub enum FactEffect {
    Set(WorldKeyId, FactValue),
    AddInt(WorldKeyId, i32),
    Clear(WorldKeyId),
}

impl FactEffect {
    pub fn set_bool(key: WorldKeyId, value: bool) -> Self {
        Self::Set(key, FactValue::Bool(value))
    }

    pub fn set_int(key: WorldKeyId, value: i32) -> Self {
        Self::Set(key, FactValue::Int(value))
    }

    pub fn set_target(key: WorldKeyId, value: TargetToken) -> Self {
        Self::Set(key, FactValue::Target(value))
    }

    pub fn add_int(key: WorldKeyId, value: i32) -> Self {
        Self::AddInt(key, value)
    }

    pub fn clear(key: WorldKeyId) -> Self {
        Self::Clear(key)
    }

    pub fn apply(&self, state: &mut GoapWorldState) {
        match self {
            Self::Set(key, value) => state.set_raw(*key, Some(value.clone())),
            Self::AddInt(key, delta) => {
                let current = state.get_int(*key).unwrap_or_default();
                state.set_int(*key, current + *delta);
            }
            Self::Clear(key) => state.clear(*key),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize)]
pub struct FactPatch {
    pub key: WorldKeyId,
    pub value: Option<FactValue>,
}

impl FactPatch {
    pub fn set_bool(key: WorldKeyId, value: bool) -> Self {
        Self {
            key,
            value: Some(FactValue::Bool(value)),
        }
    }

    pub fn set_int(key: WorldKeyId, value: i32) -> Self {
        Self {
            key,
            value: Some(FactValue::Int(value)),
        }
    }

    pub fn set_target(key: WorldKeyId, value: TargetToken) -> Self {
        Self {
            key,
            value: Some(FactValue::Target(value)),
        }
    }

    pub fn clear(key: WorldKeyId) -> Self {
        Self { key, value: None }
    }

    pub fn apply(&self, state: &mut GoapWorldState) {
        state.set_raw(self.key, self.value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_consistent_for_identical_states() {
        let mut a = GoapWorldState::with_capacity(3);
        a.set_bool(WorldKeyId(0), true);
        a.set_int(WorldKeyId(1), 42);

        let mut b = GoapWorldState::with_capacity(3);
        b.set_bool(WorldKeyId(0), true);
        b.set_int(WorldKeyId(1), 42);

        assert_eq!(a, b);
        assert_eq!(a.cached_hash, b.cached_hash);
    }

    #[test]
    fn hash_diverges_for_different_states() {
        let mut a = GoapWorldState::with_capacity(2);
        a.set_bool(WorldKeyId(0), true);

        let mut b = GoapWorldState::with_capacity(2);
        b.set_bool(WorldKeyId(0), false);

        assert_ne!(a, b);
        assert_ne!(a.cached_hash, b.cached_hash);
    }

    #[test]
    fn incremental_hash_matches_full_recompute() {
        let mut state = GoapWorldState::with_capacity(4);
        state.set_bool(WorldKeyId(0), true);
        state.set_int(WorldKeyId(1), 10);
        state.set_bool(WorldKeyId(0), false);
        state.set_int(WorldKeyId(2), -5);

        let expected = compute_full_hash(&state.values);
        assert_eq!(state.cached_hash, expected);
    }

    #[test]
    fn overlay_maintains_hash() {
        let mut base = GoapWorldState::with_capacity(3);
        base.set_bool(WorldKeyId(0), false);
        base.set_int(WorldKeyId(1), 10);

        let mut overlay = GoapWorldState::with_capacity(2);
        overlay.set_bool(WorldKeyId(0), true);

        base.overlay(&overlay);

        let expected = compute_full_hash(&base.values);
        assert_eq!(base.cached_hash, expected);
        assert_eq!(base.get_bool(WorldKeyId(0)), Some(true));
        assert_eq!(base.get_int(WorldKeyId(1)), Some(10));
    }

    #[test]
    fn serde_round_trip_preserves_equality() {
        let mut state = GoapWorldState::with_capacity(3);
        state.set_bool(WorldKeyId(0), true);
        state.set_int(WorldKeyId(1), 42);
        state.set_target(WorldKeyId(2), TargetToken(999));

        let serialized = ron::to_string(&state).unwrap();
        let deserialized: GoapWorldState = ron::from_str(&serialized).unwrap();

        assert_eq!(state, deserialized);
        assert_eq!(state.cached_hash, deserialized.cached_hash);
    }

    #[test]
    fn default_state_has_consistent_hash() {
        let state = GoapWorldState::default();
        assert_eq!(state.cached_hash, 0);
        assert_eq!(state.cached_hash, compute_full_hash(&state.values));
    }

    #[test]
    fn ensure_len_maintains_hash() {
        let mut state = GoapWorldState::with_capacity(2);
        state.set_bool(WorldKeyId(0), true);

        state.ensure_len(5);
        let expected = compute_full_hash(&state.values);
        assert_eq!(state.cached_hash, expected);
    }

    #[test]
    fn hashmap_lookup_works_correctly() {
        use std::collections::HashMap;

        let mut a = GoapWorldState::with_capacity(2);
        a.set_bool(WorldKeyId(0), true);
        a.set_int(WorldKeyId(1), 5);

        let mut map = HashMap::new();
        map.insert(a.clone(), 42u32);

        let mut lookup = GoapWorldState::with_capacity(2);
        lookup.set_bool(WorldKeyId(0), true);
        lookup.set_int(WorldKeyId(1), 5);

        assert_eq!(map.get(&lookup), Some(&42));

        let mut miss = GoapWorldState::with_capacity(2);
        miss.set_bool(WorldKeyId(0), false);
        miss.set_int(WorldKeyId(1), 5);

        assert_eq!(map.get(&miss), None);
    }
}
