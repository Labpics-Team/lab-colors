//! Ограниченная схема внешнего запроса. Никаких вычислений цвета или AUTH.
use serde::{
    Deserialize, Deserializer,
    de::{Error, SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData};

pub(crate) const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_PROGRAM_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_SCENARIOS: usize = 64;
pub(crate) const MAX_SURFACES: usize = 4096;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    pub format_version: u32,
    pub kind: String,
    pub program_wire_hex: String,
    pub binding: Binding,
    pub observation: Observation,
    pub profile: Profile,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Binding {
    pub stream_id: u32,
    pub output_slot: u32,
    pub sink_output: u32,
    pub presentation_root: u32,
    pub occurrence: u32,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Observation {
    pub revision: u64,
    pub scenarios: Bounded<Scenario, MAX_SCENARIOS>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Scenario {
    pub id: u32,
    pub surfaces: Bounded<[u8; 3], MAX_SURFACES>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Profile {
    pub convention_release_sha256: String,
    pub scope: String,
    pub admission: String,
    pub human: String,
}

/// Счётчик проверяется при разборе последовательности, без доверия size_hint.
#[derive(Debug)]
pub(crate) struct Bounded<T, const N: usize>(pub Vec<T>);
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Bounded<T, N> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct Items<T, const N: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Items<T, N> {
            type Value = Bounded<T, N>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a bounded sequence of at most {N} items")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    if items.len() == N {
                        return Err(A::Error::custom("too_many_items"));
                    }
                    items
                        .try_reserve(1)
                        .map_err(|_| A::Error::custom("allocation_refused"))?;
                    items.push(item);
                }
                Ok(Bounded(items))
            }
        }
        de.deserialize_seq(Items::<T, N>(PhantomData))
    }
}
