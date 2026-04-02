use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub struct WorldKeyId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum FactValueType {
    Bool,
    Int,
    Target,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub struct TargetToken(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect)]
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

#[derive(Debug, Clone, PartialEq, Eq, Reflect)]
pub struct WorldKeyDefinition {
    pub id: WorldKeyId,
    pub name: String,
    pub value_type: FactValueType,
    pub description: Option<String>,
    pub default_value: Option<FactValue>,
}

#[derive(Debug, Clone, Default, PartialEq, Reflect)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Reflect)]
pub struct GoapWorldState {
    pub values: Vec<Option<FactValue>>,
}

impl GoapWorldState {
    pub fn with_capacity(keys: usize) -> Self {
        Self {
            values: vec![None; keys],
        }
    }

    pub fn ensure_len(&mut self, len: usize) {
        if self.values.len() < len {
            self.values.resize(len, None);
        }
    }

    pub fn set_raw(&mut self, key: WorldKeyId, value: Option<FactValue>) {
        self.ensure_len(key.0 + 1);
        self.values[key.0] = value;
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
                self.values[index] = value.clone();
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect)]
pub enum FactComparison {
    Equals(FactValue),
    NotEquals(FactValue),
    GreaterOrEqual(i32),
    LessOrEqual(i32),
    IsSet,
    IsUnset,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Reflect)]
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
