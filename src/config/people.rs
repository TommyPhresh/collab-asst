//! Every person the scheduler considers.
//!
//! Closeness helps resolve conflicts, and will be used later.
//!
//! # File Format
//!
//! `data/people.yaml` has up to 3 top-level lists, one per closeness bucket.
//! Each entry is in form `{id, role, birthday}`
//!
//! ```yaml
//! immediate:
//!   - {id: keaton, role: brother, birthday: "Oct 17"}
//! close:
//!   - {id: peter, role: grandfather, birthday: "Jan 2"}
//! extended:
//!   - {id: tom, role: uncle}
//! ```
//!
//! Rules are enforced at load time.
//!
//! - Ids are unique across **all** buckets.
//! - `role` is one of `Role` variants in `snake_case`.
//! - `birthday` follows [`MonthDay`] syntax.
//! - No other keys allowed, missing treated as empty.
//!
//! Ids are matched exactly and expected to be lowercase.
//!
//! # Example
//!
//! ```
//! use collab_asst::config::people::{Closeness, People};
//!
//! let people = People::from_yaml(
//!     "immediate:\n  - {id: keaton, role: brother}\n\
//!      extended:\n  - {id: tom, role: uncle}\n",
//! ).unwrap();
//! assert_eq!(people.closeness("keaton"), Some(Closeness::Immediate));
//! assert_eq!(people.closeness("tom"), Some(Closeness::Extended));
//! assert_eq!(people.closeness("nemo"), None);
//! ```

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use serde::Deserialize;
use thiserror::Error;

use super::units::MonthDay;

/// How close a person is.
///
/// Ordered closest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Closeness {
    Immediate,
    Close,
    Extended,
}

impl fmt::Display for Closeness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Closeness::Immediate => "immediate",
            Closeness::Close => "close",
            Closeness::Extended => "extended",
        })
    }
}

/// Person's family relationship.
///
/// Informational, since closeness is independent of role.
///
/// Spelled in `snake_case` in YAML. Must be one of the defined variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Mother,
    Father,
    Brother,
    Sister,
    Grandmother,
    Grandfather,
    Aunt,
    Uncle,
    GreatAunt,
    GreatUncle,
    Cousin,
}

/// One entry in `data/people.yaml`.
///
/// Doesn't store its own `Closeness`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: String,
    pub role: Role,
    #[serde(default)]
    pub birthday: Option<MonthDay>,
}

/// `people.yaml` on-disk shape before indexing.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPeople {
    #[serde(default)]
    immediate: Vec<Person>,
    #[serde(default)]
    close: Vec<Person>,
    #[serde(default)]
    extended: Vec<Person>,
}

/// Person and their closeness bucket.
#[derive(Debug, Clone)]
struct Entry {
    closeness: Closeness,
    person: Person,
}

/// Index of all people by `id`.
///
/// Built with [`People::load`] or [`People::from_yaml`].
/// A successful build guarantees unique IDs and bucketed people.
///
/// Lookups are `O(1)` but order is random since `HashMap`.
#[derive(Debug, Clone)]
pub struct People {
    by_id: HashMap<String, Entry>,
}

/// Why `data/people.yaml` didn't load.
#[derive(Debug, Error)]
pub enum PeopleError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parsing people: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error("duplicate id {id:?}: first in `{first}`, again in `{second}`")]
    DuplicateId {
        id: String,
        first: Closeness,
        second: Closeness,
    },
}

impl People {
    /// Reads and indexes `data/people.yaml` from file.
    ///
    /// `path` is used as given.
    /// # Errors
    ///
    /// [`PeopleError::Io`] if file can't be read.
    pub fn load(path: &Path) -> Result<Self, PeopleError> {
        let text = std::fs::read_to_string(path).map_err(|source| PeopleError::Io {
            path: path.to_owned(),
            source,
        })?;
        Self::from_yaml(&text)
    }

    /// Parses and indexes `people.yaml` from string.
    ///
    /// # Errors
    ///
    /// [`PeopleError::Yaml`] if text doesn't match file format,
    /// [`PeopleError::DuplicateId`] if id repeats.
    pub fn from_yaml(s: &str) -> Result<Self, PeopleError> {
        let raw: RawPeople = serde_yaml_ng::from_str(s)?;
        Self::from_raw(raw)
    }

    /// Builds index from parsed buckets in closeness order.
    ///
    /// Since buckets are processed closest first, `DuplicateId`
    /// errors short-circuit in closeness order.
    fn from_raw(raw: RawPeople) -> Result<Self, PeopleError> {
        let buckets = [
            (Closeness::Immediate, raw.immediate),
            (Closeness::Close, raw.close),
            (Closeness::Extended, raw.extended),
        ];
        let mut by_id: HashMap<String, Entry> = HashMap::new();
        for (closeness,people) in buckets {
            for person in people {
                if let Some(prev) = by_id.get(&person.id) {
                    return Err(PeopleError::DuplicateId {
                        id: person.id,
                        first: prev.closeness,
                        second: closeness,
                    });
                }
                by_id.insert(person.id.clone(), Entry { closeness, person });
            }
        }
        Ok(People { by_id })
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&Person> {
        self.by_id.get(id).map(|e| &e.person)
    }

    pub fn closeness(&self, id: &str) -> Option<Closeness> {
        self.by_id.get(id).map(|e| e.closeness)
    }

    /// Retrieves all people whose birthday falls in defined range in birthday order.
    ///
    /// # Example
    ///
    /// ```
    /// use collab_asst::config::people::People;
    ///
    /// let people = People::from_yaml(
    ///    "close:\n
    ///       - {id: peter, role: grandfather, birthday: \"Jan 2\"}\n\
    ///       - [id: ben, role: uncle, birthday: \"Dec 21\"}\n",
    /// ).unwrap();
    ///
    /// let soon = people.birthdays_between("Dec 20".parse().unwrap(), "Jan 5".parse().unwrap());
    /// let ids: Vec<_> = soon.iter().map(|p| p.id.as_str()).collect();
    /// assert_eq!(ids, ["ben", "peter"]);
    /// ```
    pub fn birthdays_between(&self, from: MonthDay, to: MonthDay) -> Vec<&Person> {
        let mut hits: Vec<(&Person, MonthDay)> = self
            .by_id
            .values()
            .filter_map(|e| e.person.birthday.map(|b| (&e.person, b)))
            .filter(|&(_, b)| b.within(from, to))
            .collect();
        hits.sort_by_key(|&(p,b)| (b < from, b, p.id.as_str()));
        hits.into_iter().map(|(p,_)| p).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
immediate:
 - {id: keaton, role: brother, birthday: "Oct 17"}
 - {id: ilona, role: mother, birthday: "Feb 17"}
close:
 - {id: peter, role: grandfather, birthday: "Jan 2"}
extended:
 - {id: stephanie, role: cousin, birthday: "Dec 21"}
 - {id: ben, role: uncle, birthday: "Dec 21"}
 - {id: noelani, role: cousin, birthday: "Dec 25"}
 - {id: tom, role: uncle}
"#;
    fn md(s: &str) -> MonthDay {
        s.parse().unwrap()
    }

    fn ids(v: Vec<&Person>) -> Vec<&str> {
        v.into_iter().map(|p| p.id.as_str()).collect()
    }

    #[test]
    fn loads_and_indexes() {
        let p = People::from_yaml(SAMPLE).unwrap();
        assert_eq!(p.len(), 7);
        assert_eq!(p.closeness("keaton"), Some(Closeness::Immediate));
        assert_eq!(p.closeness("peter"), Some(Closeness::Close));
        assert_eq!(p.closeness("tom"), Some(Closeness::Extended));
        assert_eq!(p.closeness("nemo"), None);
        assert_eq!(p.get("tom").unwrap().birthday, None);
        assert_eq!(p.get("ilona").unwrap().role, Role::Mother);
    }

    #[test]
    fn missing_buckets_are_empty() {
        let p = People::from_yaml("immediate:\n - {id: dave, role: father}\n").unwrap();
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn duplicate_across_buckets() {
        let yaml = r#"
close:
 - {id: melissa, role: aunt}
extended:
 - {id: melissa, role: cousin}
"#;
        match People::from_yaml(yaml) {
            Err(PeopleError::DuplicateId { id, first, second }) => {
                assert_eq!(id, "melissa");
                assert_eq!(first, Closeness::Close);
                assert_eq!(second, Closeness::Extended);
            }
            other => panic!("expected DuplicateId, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_within_bucket() {
        let yaml = "extended:\n - {id: amy, role: aunt}\n - {id: amy, role: cousin}\n";
        assert!(matches!(
            People::from_yaml(yaml),
            Err(PeopleError::DuplicateId { first: Closeness::Extended, second: Closeness::Extended, ..})
        ));
    }

    #[test]
    fn rejects_bad_input() {
        let cases = [
            ("unknown role", "close:\n - {id: x, role: friend}\n"),
            ("unknown field", "close:\n - {id: x, role: aunt, age: 40}\n"),
            ("unknown bucket", "friends:\n - {id: x, role: aunt}\n"),
            ("bad birthday", "close:\n - {id: x, role: aunt, birthday: \"Feb 30\"}\n"),
            ("missing role", "close:\n - {id: x}\n"),
            ("grouped extended", "extended:\n aunts:\n  - {id: x, role: aunt}\n"),
        ];
        for (name,yaml) in cases {
            assert!(
                matches!(People::from_yaml(yaml), Err(PeopleError::Yaml(_))),
                "{name} should fail to parse"
            );
        }
    }

    #[test]
    fn birthdays_plain_range() {
        let p = People::from_yaml(SAMPLE).unwrap();
        assert_eq!(ids(p.birthdays_between(md("Feb 1"), md("Oct 31"))), ["ilona", "keaton"]);
        assert!(p.birthdays_between(md("Mar 1"), md("Mar 31")).is_empty());
    }

    #[test]
    fn birthdays_wrap_new_year() {
        let p = People::from_yaml(SAMPLE).unwrap();
        assert_eq!(
            ids(p.birthdays_between(md("Dec 20"), md("Jan 5"))),
            ["ben", "stephanie", "noelani", "peter"]
        );
    }

    #[test]
    fn real_file_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/people.yaml");
        let p = People::load(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(p.len() > 50);
        assert_eq!(p.closeness("keaton"), Some(Closeness::Immediate));
    }

    #[test]
    fn missing_file_is_io_error() {
        let err = People::load(Path::new("does/not/exist.yaml")).unwrap_err();
        assert!(matches!(err, PeopleError::Io { .. }));
        assert!(err.to_string().contains("does/not/exist.yaml"));
    }
}
