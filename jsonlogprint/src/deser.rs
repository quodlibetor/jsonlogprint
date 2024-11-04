use std::borrow::Cow;
use std::fmt;

use serde::de::DeserializeSeed;
use serde::de::Visitor;
use serde::Deserialize;
use serde::Serialize;

use crate::FnvIndexMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum JsonValue<'a> {
    #[serde(borrow)]
    String(Cow<'a, str>),
    Number(serde_json::Number),
    Bool(bool),
    Null,
    #[serde(borrow)]
    Object(FnvIndexMap<&'a str, JsonValue<'a>>),
    #[serde(borrow)]
    Array(Vec<JsonValue<'a>>),
    Removed,
}

// Custom DeserializeSeed and Visitor
pub(crate) struct IndexMapSeed<'a, 'b> {
    pub(crate) map: &'b mut FnvIndexMap<&'a str, JsonValue<'a>>,
}

impl<'de, 'a, 'b> DeserializeSeed<'de> for IndexMapSeed<'a, 'b>
where
    'de: 'a,
{
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de, 'a, 'b> Visitor<'de> for IndexMapSeed<'a, 'b>
where
    'de: 'a,
{
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<M>(self, mut access: M) -> Result<(), M::Error>
    where
        M: serde::de::MapAccess<'de>,
    {
        while let Some((key, value)) = access.next_entry::<&'a str, JsonValue<'a>>()? {
            self.map.insert(key, value);
        }
        Ok(())
    }
}
