//! **Deserialize** puts info into typed structs and identifies
//! errors and/or unknown values.
//! **Validate** cross-references and checks for unique ranks
//! and repeated IDs/TBDs.
//!
//! Expression strings stay raw `Expr` strings and are compiled
//! and checked elsewhere.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use super::people::{People, PeopleError};
use super::units::{Duration, DurationRange, TimeOfDay, WeeklyAt};

/// Some value, can be left as `TBD` if undecided
#[derive(Debug, Clone, PartialEq)]
pub enum Setting<T> {
    Tbd,
    Value(T),
}

impl<T> Setting<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Setting::Tbd => None,
            Setting::Value(v) => Some(v),
        }
    }

    pub fn is_tbd(&self) -> bool {
        matches!(self, Setting::Tbd)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Setting<T> {
    /// Exact string `TBD` becomes `Tbd`, else parsed as `T`.
    /// Goes thru YAML for more built-in error handling
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_yaml_ng::Value::deserialize(d)?;
        if v.as_str() == Some("TBD") {
            return Ok(Setting::Tbd);
        }
        T::deserialize(v)
            .map(Setting::Value)
            .map_err(serde::de::Error::custom)
    }
}

/// Raw expression source to be dealt with later
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Expr(pub String);

/// Stakes tier rating
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "u8")]
pub struct Tier(u8);

impl Tier {
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Tier {
    type Error = String;
    fn try_from(n: u8) -> Result<Self, Self::Error> {
        if (1..=4).contains(&n) {
            Ok(Tier(n))
        } else {
            Err(format!("tier {n} out of range 1-4"))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bound {
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinMax {
    pub min: Duration,
    pub max: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    Soft,
    Hard,
    BestEffort,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Life {
    pub domains: Domains,
    pub stakes: Stakes,
    pub logistics: Logistics,
    pub religion: Religion,
    pub sleep: Sleep,
    pub work: Work,
    pub training: Training,
    pub weekends: Weekends,
    pub friends: Friends,
    #[serde(rename = "self")]
    pub self_: SelfCare,
    pub hobbies: Hobbies,
    pub arcs: Vec<Arc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Domain {
    Family,
    Religion,
    Friends,
    Work,
    Training,
    SelfCare,
    Hobbies,
}

impl Domain {
    pub const ALL: [Domain; 7] = [
        Domain::Family,
        Domain::Religion,
        Domain::Friends,
        Domain::Work,
        Domain::Training,
        Domain::SelfCare,
        Domain::Hobbies,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Domain::Family => "family",
            Domain::Religion => "religion",
            Domain::Friends => "friends",
            Domain::Work => "work",
            Domain::Training => "training",
            Domain::SelfCare => "self",
            Domain::Hobbies => "hobbies",
        }
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Domain fields raise parse errors if misspelled or unknown
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Domains {
    pub family: FamilyDomain,
    pub religion: DomainCfg,
    pub friends: DomainCfg,
    pub work: DomainCfg,
    pub training: DomainCfg,
    #[serde(rename = "self")]
    pub self_: DomainCfg,
    pub hobbies: DomainCfg,
}

impl Domains {
    pub fn rank(&self, d: Domain) -> u8 {
        match d {
            Domain::Family => self.family.rank,
            Domain::Religion => self.religion.rank,
            Domain::Friends => self.friends.rank,
            Domain::Work => self.work.rank,
            Domain::Training => self.training.rank,
            Domain::SelfCare => self.self_.rank,
            Domain::Hobbies => self.hobbies.rank,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainCfg {
    pub rank: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyDomain {
    pub rank: u8,
    pub people: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stakes {
    pub tiers: TierNames,
    pub closeness_modifier: ClosenessModifier,
    pub resolution: Vec<ResolutionKey>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierNames {
    pub routine: Tier,
    pub notable: Tier,
    pub rare: Tier,
    pub once_ever: Tier,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosenessModifier {
    pub immediate: i8,
    pub close: i8,
    pub extended: i8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionKey {
    EffectiveTierDesc,
    DomainRankAsc,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logistics {
    pub prep: PlannedRange,
    pub commute: Commute,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedRange {
    pub min: Duration,
    pub max: Duration,
    pub plan_with: Bound,
}

/// `default` and `plan_with` are required
#[derive(Debug, Clone, Deserialize)]
pub struct Commute {
    pub default: String,
    pub plan_with: Bound,
    #[serde(flatten)]
    pub modes: HashMap<String, MinMax>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Religion {
    pub hard: Vec<HardBlock>,
    pub growth: ReligionGrowth,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HardBlock {
    pub id: String,
    pub when: WeeklyAt,
    pub duration: Duration,
    pub tier: Tier,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReligionGrowth {
    pub self_study: SelfStudy,
    pub involvement: Involvement,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfStudy {
    pub weekly_min: Setting<Duration>,
    pub preferred: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Involvement {
    #[serde(rename = "type")]
    pub kind: GoalKind,
    pub candidates: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    Exploration,
    Performance,
    Dormant,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sleep {
    pub rule: SleepRule,
    pub target: Duration,
    pub wake_at: Expr,
    pub bedtime: Expr,
    pub hard: bool,
    pub observed: ObservedSleep,
    pub early_session_warning: WarningLead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SleepRule {
    Derived,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedSleep {
    pub normal_wake: TimeOfDay,
    pub earliest_wake: TimeOfDay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningLead {
    DayBefore,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub default: WorkDefault,
    pub window: WorkWindow,
    pub emergency_end: TimeOfDay,
    pub late_threshold: TimeOfDay,
    pub late_days_per_week: LateDays,
    pub stance: String,
    pub slack_log: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkDefault {
    pub start: TimeOfDay,
    pub end: TimeOfDay,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkWindow {
    pub earliest_start: TimeOfDay,
    pub latest_end: TimeOfDay,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LateDays {
    pub max: u8,
    pub on_exceed: OnExceed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnExceed {
    Warn,
    Pushback,
    Block,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Training {
    /// Must match the `owner` of a performance arc
    pub source: String,
    /// Path relative to directory containing life.yaml
    pub import: PathBuf,
    /// Influential references
    pub respects: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weekends {
    pub shape: WeekendShape,
    pub fill_policy: FillPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeekendShape {
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillPolicy {
    SuggestNotSchedule,
    Schedule,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Friends {
    pub nights_out: NightsOut,
    /// Requires an arc ID
    pub arc: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightsOut {
    pub cap: Cap,
    pub end_by: Expr,
    pub pre_commit: PreCommit,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap {
    pub count: u8,
    pub per: Duration,
    pub mode: Enforcement,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreCommit {
    pub drink_limit: Setting<u8>,
    pub ask: AskWhen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskWhen {
    OnCreation,
    Never,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelfCare {
    pub floors: HashMap<String, Floor>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Floor {
    pub per_week: Setting<u8>,
    #[serde(default)]
    pub target: Option<u8>,
    #[serde(default)]
    pub duration: Option<DurationRange>,
    #[serde(default)]
    pub mode: Option<Enforcement>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hobbies {
    pub pool: Vec<String>,
    pub policy: Enforcement,
    pub starvation_boost: StarvationBoost,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarvationBoost {
    pub after_weeks: u8,
    pub weight_bonus: Setting<f32>,
}

/// Only cross-kind tag is `type`, allowing individual kind fields
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Arc {
    Exploration {
        id: String,
        #[serde(default)]
        budget: Option<ArcBudget>,
        #[serde(default)]
        review: Option<Review>,
        #[serde(default)]
        prefer_overlap: Vec<String>,
    },
    Performance {
        id: String,
        target: Points,
        current: Points,
        owner: String,
    },
    Dormant {
        id: String,
        wake: Wake,
    },
}

impl Arc {
    pub fn id(&self) -> &str {
        match self {
            Arc::Exploration { id, .. } | Arc::Performance { id, .. } | Arc::Dormant { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArcBudget {
    pub floor: ArcFloor,
    #[serde(default)]
    pub opportunistic: Option<Opportunistic>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArcFloor {
    pub sessions_per_week: u8,
    pub duration: Duration,
    pub outside_work: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opportunistic {
    pub source: String,
    pub credit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Review {
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Points {
    pub points: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wake {
    pub manual_confirm: bool,
}

#[derive(Debug, Error)]
pub enum LifeError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parsing life.yaml: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error("{} problem(s) in life.yaml:\n{}", .0.len(),
    .0.iter().map(|p| format!(" - {p}")).collect::<Vec<_>>().join("\n"))]
    Invalid(Vec<Problem>),
    #[error("loading people: {0}")]
    People(#[from] PeopleError),
}

/// Validation failure showing why
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub at: String,
    pub msg: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.at, self.msg)
    }
}

/// Contains everything a `cargo check` needs
#[derive(Debug)]
pub struct Loaded {
    pub life: Life,
    pub people: People,
    pub tbds: Vec<String>,
    /// Directory with life.yaml in it
    pub base: PathBuf,
}

impl Life {
    pub fn from_yaml(s: &str) -> Result<Self, LifeError> {
        let life: Life = serde_yaml_ng::from_str(s)?;
        let problems = life.validate();
        if problems.is_empty() {
            Ok(life)
        } else {
            Err(LifeError::Invalid(problems))
        }
    }

    /// Load life.yaml and people.yaml
    pub fn load(path: &Path) -> Result<Loaded, LifeError> {
        let text = std::fs::read_to_string(path).map_err(|source| LifeError::Io {
            path: path.to_owned(),
            source,
        })?;
        let life = Self::from_yaml(&text)?;
        let base = path.parent().unwrap_or(Path::new(".")).to_owned();
        let people = People::load(&base.join(&life.domains.family.people))?;
        let tbds = life.tbds();
        Ok(Loaded { life, people, tbds, base })
    }

    /// Cross-field entire struct instance checks
    pub fn validate(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        let mut bad = |at: &str, msg: String| out.push(Problem { at: at.to_owned(), msg });

        let mut seen: HashMap<u8, Domain> = HashMap::new();
        for d in Domain::ALL {
            let r = self.domains.rank(d);
            if !(1..=Domain::ALL.len() as u8).contains(&r) {
                bad(&format!("domains.{d}.rank"), format!("{r} out of range 1-{}", Domain::ALL.len()));
            } else if let Some(other) = seen.insert(r,d) {
                bad(&format!("domains.{d}.rank"), format!("rank {r} already used by {other}"));
            }
        }

        let t = &self.stakes.tiers;
        if !(t.routine < t.notable && t.notable < t.rare && t.rare < t.once_ever) {
            bad("stakes.tiers", "must increase: routine < notable < rare < once_ever".into());
        }

        let res = &self.stakes.resolution;
        if res.is_empty() {
            bad("stakes.resolution", "must list at least one key".into());
        }
        if res.iter().collect::<HashSet<_>>().len() != res.len() {
            bad("stakes.resolution", "duplicate key".into());
        }

        let p = &self.logistics.prep;
        if p.min > p.max {
            bad("logistics.prep", format!("min {} > max {}", p.min, p.max));
        }
        let c = &self.logistics.commute;
        if c.modes.is_empty() {
            bad("logistics.commute", "no travel modes defined".into());
        }
        if !c.modes.contains_key(&c.default) {
            bad("logistics.commute.default", format!("{:?} is not a defined mode", c.default));
        }
        for (name,m) in &c.modes {
            if m.min > m.max {
                bad(&format!("logistics.commute.{name}"), format!("min {} > max {}", m.min, m.max));
            }
        }

        let w = &self.work;
        let chain = [
            ("window.earliest_start", w.window.earliest_start),
            ("default.start", w.default.start),
            ("default.end", w.default.end),
            ("window.latest_end", w.window.latest_end),
            ("late_threshold", w.late_threshold),
            ("emergency_end", w.emergency_end),
        ];
        for pair in chain.windows(2) {
            let ((an,a), (bn,b)) = (pair[0], pair[1]);
            if a > b {
                bad("work", format!("{an} ({a}) is after {bn} ({b})"));
            }
        }
        if w.default.start == w.default.end {
            bad("work.default", "start and end are equal".into());
        }

        let o = &self.sleep.observed;
        if o.earliest_wake > o.normal_wake {
            bad("sleep.observed", format!("earliest_wake {} is after normal_wake {}", o.earliest_wake, o.normal_wake));
        }

        let mut ids = HashSet::new();
        for h in &self.religion.hard {
            if !ids.insert(h.id.as_str()) {
                bad("religion.hard", format!("duplicate id {:?}", h.id));
            }
        }

        let mut arc_ids = HashSet::new();
        for a in &self.arcs {
            if !arc_ids.insert(a.id()) {
                bad("arcs", format!("duplicate arc id {:?}", a.id()));
            }
        }
        if !arc_ids.contains(self.friends.arc.as_str()) {
            bad("friends.arc", format!("{:?} is not an arc id", self.friends.arc));
        }
        let owners: HashSet<&str> = self
            .arcs
            .iter()
            .filter_map(|a| match a {
                Arc::Performance { owner, .. } => Some(owner.as_str()),
                _ => None,
            })
            .collect();
        if !owners.contains(self.training.source.as_str()) {
            bad("training.source", format!("{:?} owns no performance arc", self.training.source));
        }

        let domain_names: HashSet<&str> = Domain::ALL.iter().map(|d| d.name()).collect();
        for a in &self.arcs {
            if let Arc::Exploration { id, prefer_overlap, .. } = a {
                for d in prefer_overlap {
                    if !domain_names.contains(d.as_str()) {
                        bad(&format!("arcs.{id}.prefer_overlap"), format!("{d:?} is not a domain"));
                    }
                }
            }
        }

        let known_refs = ["sleep", "work.window", "work.slack_log"];
        for r in &self.training.respects {
            if !known_refs.contains(&r.as_str()) {
                bad("training.respects", format!("unknown reference {r:?}"));
            }
        }
        for a in &self.arcs {
            if let Arc::Exploration { id, budget: Some(b), .. } = a && let Some(op) = &b.opportunistic {
                let at = format!("arcs.{id}.budget.opportunistic.source");
                if !known_refs.contains(&op.source.as_str()) {
                    bad(&at, format!("unknown reference {:?}", op.source));
                }
                if op.source == "work.slack_log" && !self.work.slack_log {
                    bad(&at, "work.slack_log is disabled".into());
                }
            }
        }

        for (name,f) in &self.self_.floors {
            if let (Some(&per), Some(target)) = (f.per_week.value(), f.target) && target < per {
                bad(&format!("self.floors.{name}"), format!("target {target} < per_week {per}"));
            }
        }

        let mut hob = HashSet::new();
        for h in &self.hobbies.pool {
            if !hob.insert(h.as_str()) {
                bad("hobbies.pool", format!("duplicate {h:?}"));
            }
        }

        out
    }

    pub fn tbds(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.religion.growth.self_study.weekly_min.is_tbd() {
            out.push("religion.growth.self_study.weekly_min".to_owned());
        }
        if self.friends.nights_out.pre_commit.drink_limit.is_tbd() {
            out.push("friends.nights_out.pre_commit.drink_limit".to_owned());
        }
        if self.hobbies.starvation_boost.weight_bonus.is_tbd() {
            out.push("hobbies.starvation_boost.weight_bonus".to_owned());
        }
        for (name,f) in &self.self_.floors {
            if f.per_week.is_tbd() {
                out.push(format!("self.floors.{name}.per_week"));
            }
        }
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn real_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("data/life.yaml")
    }

    fn real_text() -> String {
        std::fs::read_to_string(real_path()).unwrap()
    }

    fn with_sub(from: &str, to: &str) -> Result<Life, LifeError> {
        let text = real_text();
        assert!(text.contains(from), "fixture text {from:?} not found in life.yaml");
        Life::from_yaml(&text.replacen(from, to, 1))
    }

    fn problems(r: Result<Life, LifeError>) -> Vec<Problem> {
        match r {
            Err(LifeError::Invalid(p)) => p,
            Err(e) => panic!("expected validation problems, got {e}"),
            Ok(_) => panic!("expected validation problems, got Ok"),
        }
    }

    fn has_problems(problems: &[Problem], at: &str) -> bool {
        problems.iter().any(|p| p.at.starts_with(at))
    }

    #[test]
    fn real_file_loads() {
        let loaded = Life::load(&real_path()).unwrap_or_else(|e| panic!("{e}"));
        let l = &loaded.life;
        assert_eq!(l.domains.rank(Domain::Family), 1);
        assert_eq!(l.domains.rank(Domain::Hobbies), 7);
        assert_eq!(l.religion.hard[0].when.to_string(), "sun 08:45");
        assert_eq!(l.logistics.commute.modes["bike"].max.minutes(), 35);
        assert_eq!(l.stakes.closeness_modifier.immediate, 1);
        assert_eq!(l.sleep.wake_at.0, "first_commitment - prep.max - commute(mode).max");
        assert_eq!(l.arcs.len(), 4);
        assert!(loaded.people.len() > 50);
    }

    #[test]
    fn tbds_listed() {
        let l = Life::from_yaml(&real_text()).unwrap();
        assert_eq!(
            l.tbds(),
            [
                "friends.nights_out.pre_commit.drink_limit",
                "hobbies.starvation_boost.weight_bonus",
                "religion.growth.self_study.weekly_min",
                "self.floors.reflection.per_week",
            ]
        );
    }

    #[test]
    fn setting_accepts_value_and_rejects_garbage() {
        let l = with_sub("drink_limit: TBD", "drink_limit: 3").unwrap();
        assert_eq!(l.friends.nights_out.pre_commit.drink_limit, Setting::Value(3));
        let err = with_sub("drink_limit: TBD", "drink_limit: tbd").unwrap_err();
        assert!(matches!(err, LifeError::Yaml(_)), "{err}");
    }

    #[test]
    fn shape_errors() {
        let cases = [
            ("unknown top-level key", "weekends:", "weekendz:"),
            ("missing domain", " hobbies: {rank: 7}", ""),
            ("bad tier", "duration: 60m, tier: 3", "duration: 60m, tier: 5"),
            ("bad duration", "target: 8h", "target: 8x"),
            ("bad enum", "on_exceed: pushback", "on_exceed: shit_on_the_floor"),
            ("unknown arc type", "type: dormant", "type: not_fucking_happening"),
            ("wrong field for arc type", "wake: {manual_confirm: true}", "wake: {manual_confirm: true}\n  owner: your_mom"),
        ];
        for (name,from,to) in cases {
            assert!(matches!(with_sub(from,to), Err(LifeError::Yaml(_))), "{name} should be a parse error");
        }
    }

    #[test]
    fn duplicate_rank() {
        let p = problems(with_sub("hobbies: {rank: 7}", "hobbies: {rank: 6}"));
        assert!(has_problems(&p, "domains."), "{p:?}");
    }

    #[test]
    fn commute_default_must_exist() {
        let p = problems(with_sub("default: bike", "default: scooter"));
        assert!(has_problems(&p, "logistics.commute.default"), "{p:?}");
    }

    #[test]
    fn work_time_chain() {
        let p = problems(with_sub("emergency_end: \"18:00\"", "emergency_end: \"16:00\""));
        assert!(has_problems(&p, "work"), "{p:?}");
    }

    #[test]
    fn dangling_references() {
        assert!(has_problems(&problems(with_sub("arc: new_friends", "arc: old_friends")), "friends.arc"));
        assert!(has_problems(&problems(with_sub("source: coach_agent", "source: nobody")), "training.source"));
        assert!(has_problems(&problems(with_sub("prefer_overlap: [hobbies,", "prefer_overlap: [hobbbies,")), "arcs.new_friends.prefer_overlap"));
        assert!(has_problems(&problems(with_sub("work.window]", "work.windw]")), "training.respects"));
    }

    #[test]
    fn collects_all_problems() {
        let text = real_text()
            .replacen("default: bike", "default: scooter", 1)
            .replacen("arc: new_friends", "arc: shittin", 1);
        let p = problems(Life::from_yaml(&text));
        assert!(p.len() >= 2, "{p:?}");
    }
}
