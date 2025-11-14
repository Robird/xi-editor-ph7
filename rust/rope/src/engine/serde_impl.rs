use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

use super::{
    default_session, initial_revision_counter, Contents, EditContentsOwned, EditContentsRef,
    Engine, RevId, Revision, RevisionContentsOwned, RevisionContentsRef, RevisionOwned,
    RevisionRef, SessionId, UndoContentsOwned, UndoContentsRef,
};
use crate::multiset::Subset;
use crate::rope::Rope;

#[derive(Serialize)]
struct EngineSerialize<'a> {
    text: &'a Rope,
    tombstones: &'a Rope,
    deletes_from_union: &'a Subset,
    undone_groups: &'a BTreeSet<usize>,
    revs: Vec<RevisionSerialize<'a>>,
}

impl<'a> From<&'a Engine> for EngineSerialize<'a> {
    fn from(engine: &'a Engine) -> Self {
        let revs = engine.revision_log().map(RevisionSerialize::from).collect();
        let _session = engine.session_components();
        let _rev_counter = engine.revision_counter();
        EngineSerialize {
            text: engine.text_snapshot(),
            tombstones: engine.tombstones_snapshot(),
            deletes_from_union: engine.deletes_from_union_snapshot(),
            undone_groups: engine.undone_groups_snapshot(),
            revs,
        }
    }
}

#[derive(Serialize)]
struct RevisionSerialize<'a> {
    rev_id: RevId,
    max_undo_so_far: usize,
    edit: RevisionContentsSerialize<'a>,
}

#[derive(Serialize)]
enum RevisionContentsSerialize<'a> {
    Edit {
        priority: usize,
        undo_group: usize,
        #[serde(borrow)]
        inserts: &'a Subset,
        #[serde(borrow)]
        deletes: &'a Subset,
    },
    Undo {
        #[serde(borrow)]
        toggled_groups: &'a BTreeSet<usize>,
        #[serde(borrow)]
        deletes_bitxor: &'a Subset,
    },
}

impl<'a> From<RevisionRef<'a>> for RevisionSerialize<'a> {
    fn from(revision: RevisionRef<'a>) -> Self {
        let edit = RevisionContentsSerialize::from(revision.contents);
        RevisionSerialize {
            rev_id: revision.rev_id,
            max_undo_so_far: revision.max_undo_so_far,
            edit,
        }
    }
}

impl<'a> From<RevisionContentsRef<'a>> for RevisionContentsSerialize<'a> {
    fn from(contents: RevisionContentsRef<'a>) -> Self {
        match contents {
            RevisionContentsRef::Edit(EditContentsRef {
                priority,
                undo_group,
                inserts,
                deletes,
            }) => RevisionContentsSerialize::Edit { priority, undo_group, inserts, deletes },
            RevisionContentsRef::Undo(UndoContentsRef { toggled_groups, deletes_bitxor }) => {
                RevisionContentsSerialize::Undo { toggled_groups, deletes_bitxor }
            }
        }
    }
}

#[derive(Deserialize)]
struct EngineDeserialize {
    #[serde(default = "default_session")]
    session: SessionId,
    #[serde(default = "initial_revision_counter")]
    rev_id_counter: u32,
    text: Rope,
    tombstones: Rope,
    deletes_from_union: Subset,
    undone_groups: BTreeSet<usize>,
    revs: Vec<RevisionDeserialize>,
}

#[derive(Deserialize)]
struct RevisionDeserialize {
    rev_id: RevId,
    max_undo_so_far: usize,
    edit: RevisionContentsDeserialize,
}

#[derive(Deserialize)]
enum RevisionContentsDeserialize {
    Edit { priority: usize, undo_group: usize, inserts: Subset, deletes: Subset },
    Undo { toggled_groups: BTreeSet<usize>, deletes_bitxor: Subset },
}

impl From<RevisionContentsDeserialize> for RevisionContentsOwned {
    fn from(contents: RevisionContentsDeserialize) -> Self {
        match contents {
            RevisionContentsDeserialize::Edit { priority, undo_group, inserts, deletes } => {
                RevisionContentsOwned::Edit(EditContentsOwned {
                    priority,
                    undo_group,
                    inserts,
                    deletes,
                })
            }
            RevisionContentsDeserialize::Undo { toggled_groups, deletes_bitxor } => {
                RevisionContentsOwned::Undo(UndoContentsOwned { toggled_groups, deletes_bitxor })
            }
        }
    }
}

impl From<RevisionDeserialize> for RevisionOwned {
    fn from(revision: RevisionDeserialize) -> Self {
        RevisionOwned::new(
            revision.rev_id,
            revision.max_undo_so_far,
            RevisionContentsOwned::from(revision.edit),
        )
    }
}

#[derive(Serialize, Deserialize)]
struct RevIdParts {
    session1: u64,
    session2: u32,
    num: u32,
}

impl Serialize for RevId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let (session1, session2, num) = self.raw_parts();
        RevIdParts { session1, session2, num }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RevId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parts = RevIdParts::deserialize(deserializer)?;
        Ok(RevId::from_raw_parts(parts.session1, parts.session2, parts.num))
    }
}

impl Serialize for Revision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RevisionSerialize::from(self.as_ref()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Revision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let revision = RevisionDeserialize::deserialize(deserializer)?;
        Ok(Revision::from_owned(revision.into()))
    }
}

impl Serialize for Contents {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RevisionContentsSerialize::from(self.as_ref()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Contents {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let contents = RevisionContentsDeserialize::deserialize(deserializer)?;
        Ok(Contents::from_owned(contents.into()))
    }
}

impl Serialize for Engine {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        EngineSerialize::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Engine {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let engine = EngineDeserialize::deserialize(deserializer)?;
        let revs = engine.revs.into_iter().map(RevisionOwned::from).collect();
        Ok(Engine::from_serialized_state(
            engine.session,
            engine.rev_id_counter,
            engine.text,
            engine.tombstones,
            engine.deletes_from_union,
            engine.undone_groups,
            revs,
        ))
    }
}
