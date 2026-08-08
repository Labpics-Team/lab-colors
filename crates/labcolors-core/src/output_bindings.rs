//! Static output namespace compiled from executable declarations.
//!
//! This module owns identifier grammar, ordered key materialisation, alias
//! shape resolution, and collision detection. It deliberately knows neither
//! configuration syntax nor semantic recipes; those layers map declarations
//! to the closed [`OutputBindingShape`] vocabulary.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Namespace prefix for every CSS custom property compiled by this module.
const OUTPUT_KEY_PREFIX: &str = "--lab-";

/// Immutable, fully compiled set of CSS output bindings.
///
/// Keys preserve declaration order: role primary/satellites first, then alias
/// primary/satellites. The set has no mutable API after construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputBindingSet {
    keys: Arc<Vec<String>>,
}

impl OutputBindingSet {
    /// Exact CSS custom-property keys in canonical compilation order.
    pub fn keys(&self) -> &[String] {
        self.keys.as_slice()
    }

    /// Return whether this static output contract owns `key`.
    pub fn contains(&self, key: &str) -> bool {
        self.keys.iter().any(|candidate| candidate == key)
    }

    /// Compile declarations and aliases into one exact output namespace.
    pub(crate) fn compile<'a>(
        declarations: impl IntoIterator<Item = (&'a str, OutputBindingShape)>,
        aliases: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self, OutputBindingCompileError> {
        let mut keys = Vec::new();
        let mut seen = HashSet::new();
        let mut shapes = HashMap::new();

        for (name, shape) in declarations {
            if !is_valid_contract_name(name) {
                return Err(OutputBindingCompileError::InvalidName {
                    kind: OutputBindingNameKind::Role,
                    value: name.to_string(),
                });
            }
            append_output_binding_shape(&mut keys, &mut seen, name, shape.suffixes())?;
            shapes.insert(name, shape);
        }

        for (alias, target) in aliases {
            if !is_valid_contract_name(alias) {
                return Err(OutputBindingCompileError::InvalidName {
                    kind: OutputBindingNameKind::Alias,
                    value: alias.to_string(),
                });
            }
            let shape = shapes.get(target).copied().ok_or_else(|| {
                OutputBindingCompileError::UnknownAliasTarget {
                    alias: alias.to_string(),
                    target: target.to_string(),
                }
            })?;
            append_output_binding_shape(&mut keys, &mut seen, alias, shape.suffixes())?;
        }

        Ok(Self {
            keys: Arc::new(keys),
        })
    }
}

/// Closed output shape selected exhaustively by the semantic recipe layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputBindingShape {
    /// Primary custom property only.
    Primary,
    /// Primary plus `-core` and `-alpha`.
    Glow,
    /// Primary plus `-01` and `-02`.
    Material,
}

impl OutputBindingShape {
    fn suffixes(self) -> &'static [&'static str] {
        match self {
            Self::Primary => &[""],
            Self::Glow => &["", "-core", "-alpha"],
            Self::Material => &["", "-01", "-02"],
        }
    }
}

/// Which public declaration supplied an invalid output name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputBindingNameKind {
    /// Executable role declaration.
    Role,
    /// Alias declaration.
    Alias,
}

/// Typed failure produced before an output manifest can exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OutputBindingCompileError {
    /// A role or alias name violates the stable contract grammar.
    InvalidName {
        /// Declaration category used for precise boundary error mapping.
        kind: OutputBindingNameKind,
        /// Rejected name.
        value: String,
    },
    /// An alias points outside the executable role declarations.
    UnknownAliasTarget {
        /// Alias whose target cannot be resolved.
        alias: String,
        /// Missing executable role name.
        target: String,
    },
    /// Two declaration shapes reserve the same exact CSS custom property.
    DuplicateBinding {
        /// Colliding CSS custom property.
        key: String,
    },
}

impl core::fmt::Display for OutputBindingCompileError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidName { kind, value } => {
                let kind = match kind {
                    OutputBindingNameKind::Role => "role",
                    OutputBindingNameKind::Alias => "alias",
                };
                write!(
                    formatter,
                    "invalid {kind} name {value:?}: expected non-empty [a-z0-9-]+"
                )
            }
            Self::UnknownAliasTarget { alias, target } => write!(
                formatter,
                "alias {alias:?} targets unknown executable role {target:?}"
            ),
            Self::DuplicateBinding { key } => {
                write!(formatter, "duplicate output binding {key:?}")
            }
        }
    }
}

/// Stable contract-name grammar shared by configuration and executable output.
pub(crate) fn is_valid_contract_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

/// Append one shape while proving exact namespace uniqueness.
fn append_output_binding_shape(
    keys: &mut Vec<String>,
    seen: &mut HashSet<String>,
    name: &str,
    suffixes: &[&str],
) -> Result<(), OutputBindingCompileError> {
    for suffix in suffixes {
        let mut key = String::with_capacity(OUTPUT_KEY_PREFIX.len() + name.len() + suffix.len());
        key.push_str(OUTPUT_KEY_PREFIX);
        key.push_str(name);
        key.push_str(suffix);
        if !seen.insert(key.clone()) {
            return Err(OutputBindingCompileError::DuplicateBinding { key });
        }
        keys.push(key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn compiled_output_contracts_remain_send_sync() {
        assert_send_sync::<OutputBindingSet>();
        assert_send_sync::<crate::semantic::NamedRoleTable>();
    }

    #[test]
    fn primitive_rejects_derived_to_derived_collision() {
        let mut keys = Vec::new();
        let mut seen = HashSet::new();
        append_output_binding_shape(&mut keys, &mut seen, "probe", &["-outer-inner"])
            .expect("first satellite is free");
        let error = append_output_binding_shape(&mut keys, &mut seen, "probe-outer", &["-inner"])
            .expect_err("derived bindings must share the same collision gate");

        assert_eq!(
            error,
            OutputBindingCompileError::DuplicateBinding {
                key: "--lab-probe-outer-inner".to_string(),
            }
        );
    }
}
