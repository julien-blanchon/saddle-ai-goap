use std::fmt::{Display, Formatter};

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use serde::{Deserialize, Serialize};

use crate::definitions::{GoapDomainDefinition, GoapDomainId};
use crate::resources::GoapLibrary;

#[derive(Asset, Clone, Debug, PartialEq, Reflect, Serialize, Deserialize)]
pub struct GoapDomainAsset {
    pub definition: GoapDomainDefinition,
}

impl GoapDomainAsset {
    pub fn register(&self, library: &mut GoapLibrary) -> GoapDomainId {
        library.register(self.definition.clone())
    }
}

impl From<GoapDomainDefinition> for GoapDomainAsset {
    fn from(definition: GoapDomainDefinition) -> Self {
        Self { definition }
    }
}

#[derive(Default, TypePath)]
pub struct GoapDomainAssetLoader;

#[derive(Debug)]
pub enum GoapDomainAssetLoaderError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
}

impl Display for GoapDomainAssetLoaderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "failed to read GOAP asset: {error}"),
            Self::Ron(error) => write!(f, "failed to parse GOAP RON asset: {error}"),
        }
    }
}

impl std::error::Error for GoapDomainAssetLoaderError {}

impl From<std::io::Error> for GoapDomainAssetLoaderError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ron::error::SpannedError> for GoapDomainAssetLoaderError {
    fn from(value: ron::error::SpannedError) -> Self {
        Self::Ron(value)
    }
}

impl AssetLoader for GoapDomainAssetLoader {
    type Asset = GoapDomainAsset;
    type Settings = ();
    type Error = GoapDomainAssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(ron::de::from_bytes::<GoapDomainAsset>(&bytes)?)
    }

    fn extensions(&self) -> &[&str] {
        &["goap.ron"]
    }
}

#[cfg(test)]
#[path = "assets_tests.rs"]
mod tests;
