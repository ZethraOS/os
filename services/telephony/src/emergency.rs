// emergency.rs — Emergency call handler for ZethraOS Telephony
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::collections::HashSet;

pub struct EmergencyHandler {
    emergency_numbers: HashSet<String>,
}

impl EmergencyHandler {
    pub fn new() -> Self {
        let mut set = HashSet::new();
        for num in ["112", "911", "999", "100", "101", "108"] {
            set.insert(num.to_string());
        }
        Self {
            emergency_numbers: set,
        }
    }

    pub fn is_emergency(&self, number: &str) -> bool {
        self.emergency_numbers.contains(number)
    }

    pub fn allow_unauthenticated(&self, number: &str) -> bool {
        self.is_emergency(number)
    }
}

impl Default for EmergencyHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emergency_numbers() {
        let handler = EmergencyHandler::new();
        assert!(handler.is_emergency("112"));
        assert!(handler.is_emergency("911"));
        assert!(handler.is_emergency("108"));
        assert!(!handler.is_emergency("18005551234"));
    }

    #[test]
    fn test_unauthenticated_routing() {
        let handler = EmergencyHandler::new();
        assert!(handler.allow_unauthenticated("911"));
        assert!(!handler.allow_unauthenticated("5550199"));
    }
}
