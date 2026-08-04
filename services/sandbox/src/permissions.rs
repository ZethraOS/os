#![allow(dead_code)]
// permissions.rs — Permission manifest parser for ZethraOS Apps
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
        toml::from_str(content).context("Failed to parse zethra.permissions.toml")
    }

    pub fn has_permission(&self, perm: &PermissionType) -> bool {
        self.permissions.grants.contains(perm)
    }

    pub fn default_test(grants: Vec<PermissionType>) -> Self {
        Self {
            package: PackageInfo {
                name: "test-app".to_string(),
                version: "1.0".to_string(),
                author: "Zethra".to_string(),
            },
            permissions: Permissions {
                grants: grants.into_iter().collect(),
            },
        }
    }
}

impl PermissionType {
    pub fn from_u32(id: u32) -> Option<Self> {
        match id {
            0 => Some(PermissionType::Network),
            1 => Some(PermissionType::Storage),
            2 => Some(PermissionType::Camera),
            3 => Some(PermissionType::Microphone),
            4 => Some(PermissionType::Contacts),
            5 => Some(PermissionType::Location),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_parsing_and_grants() {
        let toml_str = r#"
            [package]
            name = "camera-app"
            version = "0.1.0"
            author = "Zethra"

            [permissions]
            grants = ["camera", "storage"]
        "#;
        let manifest = AppManifest::from_toml(toml_str).unwrap();
        assert!(manifest.has_permission(&PermissionType::Camera));
        assert!(manifest.has_permission(&PermissionType::Storage));
        assert!(!manifest.has_permission(&PermissionType::Network));
        assert_eq!(PermissionType::from_u32(2), Some(PermissionType::Camera));
    }
}
