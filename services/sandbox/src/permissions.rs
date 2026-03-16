#![allow(dead_code)]
// permissions.rs — Permission manifest parser for AetherOS Apps
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    pub package: PackageInfo,
    pub permissions: Permissions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: String,
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub grants: HashSet<PermissionType>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionType {
    Network,
    Storage,
    Camera,
    Microphone,
    Contacts,
    Location,
}

impl AppManifest {
    pub fn from_toml(content: &str) -> Result<Self> {
        toml::from_str(content).context("Failed to parse aether.permissions.toml")
    }

    pub fn has_permission(&self, perm: PermissionType) -> bool {
        self.permissions.grants.contains(&perm)
    }
}
