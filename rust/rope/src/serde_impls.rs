// Copyright 2019 The xi-editor Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fmt;
use std::str::FromStr;

use serde::de::{
    self, Deserialize, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor,
};
use serde::ser::{Serialize, SerializeSeq, SerializeStruct, SerializeTupleVariant, Serializer};

use crate::tree::Node;
use crate::{Delta, DeltaElement, Rope, RopeInfo};

const DELTA_ELEMENT_VARIANTS: &[&str] = &["copy", "insert"];
const DELTA_FIELDS: &[&str] = &["els", "base_len"];

impl Serialize for Rope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&String::from(self))
    }
}

impl<'de> Deserialize<'de> for Rope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(RopeVisitor)
    }
}

struct RopeVisitor;

impl<'de> Visitor<'de> for RopeVisitor {
    type Value = Rope;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a string")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Rope::from_str(s).map_err(|_| de::Error::invalid_value(de::Unexpected::Str(s), &self))
    }
}

impl Serialize for DeltaElement<RopeInfo, String> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            DeltaElement::Copy(ref start, ref end) => {
                let mut el = serializer.serialize_tuple_variant("DeltaElement", 0, "copy", 2)?;
                el.serialize_field(start)?;
                el.serialize_field(end)?;
                el.end()
            }
            DeltaElement::Insert(ref node) => {
                serializer.serialize_newtype_variant("DeltaElement", 1, "insert", node)
            }
        }
    }
}

#[derive(Debug)]
enum DeltaElementVariant {
    Copy,
    Insert,
}

impl<'de> Deserialize<'de> for DeltaElementVariant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VariantVisitor;

        impl<'de> Visitor<'de> for VariantVisitor {
            type Value = DeltaElementVariant;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("`copy` or `insert`")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    "copy" => Ok(DeltaElementVariant::Copy),
                    "insert" => Ok(DeltaElementVariant::Insert),
                    _ => Err(de::Error::unknown_variant(value, DELTA_ELEMENT_VARIANTS)),
                }
            }
        }

        deserializer.deserialize_identifier(VariantVisitor)
    }
}

struct CopyRangeVisitor;

impl<'de> Visitor<'de> for CopyRangeVisitor {
    type Value = (usize, usize);

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a two-element tuple containing `start` and `end`")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let start: usize =
            seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let end: usize =
            seq.next_element()?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
        Ok((start, end))
    }
}

struct DeltaElementVisitor;

impl<'de> Visitor<'de> for DeltaElementVisitor {
    type Value = DeltaElement<RopeInfo, String>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a DeltaElement variant")
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: EnumAccess<'de>,
    {
        let (variant, variant_access) = data.variant::<DeltaElementVariant>()?;
        match variant {
            DeltaElementVariant::Copy => {
                let (start, end) = variant_access.tuple_variant(2, CopyRangeVisitor)?;
                Ok(DeltaElement::Copy(start, end))
            }
            DeltaElementVariant::Insert => {
                let node = variant_access.newtype_variant::<Node<RopeInfo, String>>()?;
                Ok(DeltaElement::Insert(node))
            }
        }
    }
}

impl<'de> Deserialize<'de> for DeltaElement<RopeInfo, String> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_enum("DeltaElement", DELTA_ELEMENT_VARIANTS, DeltaElementVisitor)
    }
}

struct DeltaElementsSerialize<'a> {
    delta: &'a Delta<RopeInfo, String>,
}

impl Serialize for DeltaElementsSerialize<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.delta.element_count()))?;
        for element in self.delta.iter_elements() {
            seq.serialize_element(element)?;
        }
        seq.end()
    }
}

impl Serialize for Delta<RopeInfo, String> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut delta = serializer.serialize_struct("Delta", 2)?;
        delta.serialize_field("els", &DeltaElementsSerialize { delta: self })?;
        delta.serialize_field("base_len", &self.base_len())?;
        delta.end()
    }
}

enum DeltaField {
    Els,
    BaseLen,
}

impl<'de> Deserialize<'de> for DeltaField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FieldVisitor;

        impl<'de> Visitor<'de> for FieldVisitor {
            type Value = DeltaField;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("`els` or `base_len`")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    "els" => Ok(DeltaField::Els),
                    "base_len" => Ok(DeltaField::BaseLen),
                    _ => Err(de::Error::unknown_field(value, DELTA_FIELDS)),
                }
            }
        }

        deserializer.deserialize_identifier(FieldVisitor)
    }
}

struct DeltaVisitor;

impl<'de> Visitor<'de> for DeltaVisitor {
    type Value = Delta<RopeInfo, String>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("struct Delta")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut elements: Option<Vec<DeltaElement<RopeInfo, String>>> = None;
        let mut base_len: Option<usize> = None;

        while let Some(key) = map.next_key::<DeltaField>()? {
            match key {
                DeltaField::Els => {
                    if elements.is_some() {
                        return Err(de::Error::duplicate_field("els"));
                    }
                    elements = Some(map.next_value()?);
                }
                DeltaField::BaseLen => {
                    if base_len.is_some() {
                        return Err(de::Error::duplicate_field("base_len"));
                    }
                    base_len = Some(map.next_value()?);
                }
            }
        }

        let elements = elements.ok_or_else(|| de::Error::missing_field("els"))?;
        let base_len = base_len.ok_or_else(|| de::Error::missing_field("base_len"))?;
        Ok(Delta::from_element_vec(base_len, elements))
    }
}

impl<'de> Deserialize<'de> for Delta<RopeInfo, String> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct("Delta", DELTA_FIELDS, DeltaVisitor)
    }
}
