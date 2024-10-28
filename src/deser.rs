use std::fmt;

use serde::de::Visitor;
use serde::de::DeserializeSeed;
use serde::Deserialize;
use serde::Serialize;

use indexmap::IndexMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum JsonValue {
    String(String),
    Number(serde_json::Number),
    Bool(bool),
    Null,
    Object(IndexMap<String, JsonValue>),
    Array(Vec<JsonValue>),
    Removed,
}

// Custom DeserializeSeed and Visitor
pub(crate) struct IndexMapSeed<'a> {
    pub(crate) map: &'a mut IndexMap<String, JsonValue>,
}

impl<'de, 'a> DeserializeSeed<'de> for IndexMapSeed<'a> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de, 'a> Visitor<'de> for IndexMapSeed<'a> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<M>(self, mut access: M) -> Result<(), M::Error>
    where
        M: serde::de::MapAccess<'de>,
    {
        // Clear the map to reuse
        self.map.clear();

        while let Some((key, value)) = access.next_entry::<String, JsonValue>()? {
            self.map.insert(key, value);
        }
        Ok(())
    }
}
