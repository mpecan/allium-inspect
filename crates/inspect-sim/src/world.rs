//! The state a simulation runs against.
//!
//! A [`World`] is every entity instance the user has made, the configuration in
//! force, and what time it is. It is a plain value: a step takes one and returns
//! a new one, nothing is mutated in place, and the browser holds the current one
//! and posts it back with each event. That is what makes the server stateless
//! and a session shareable as a URL.
//!
//! Two properties are load-bearing and neither is negotiable.
//!
//! *Determinism.* Ordered maps throughout and a monotonic counter for ids, so
//! the same world and the same event always produce a byte-identical result.
//! Without it a trace cannot be snapshot-tested, a shared link shows something
//! different to the person who receives it, and mutation testing has no signal.
//!
//! *The clock is a field.* `now` is a number the user advances, never a reading
//! of the system clock. A temporal rule — `due_at <= now` — is then something
//! you can step *to* rather than something you have to wait for, and a
//! simulation that ran yesterday reproduces exactly today.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::value::{EntityId, Instance, Value};

/// Something that happens: a trigger, with the arguments it carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Event {
    /// The trigger's name, as the spec spells it.
    pub trigger: String,
    /// The module whose rules should see it.
    pub module: String,
    /// Argument bindings, by parameter name.
    pub arguments: BTreeMap<String, Value>,
}

impl Event {
    /// A trigger with no arguments.
    #[must_use]
    pub fn new(trigger: impl Into<String>, module: impl Into<String>) -> Self {
        Self { trigger: trigger.into(), module: module.into(), arguments: BTreeMap::new() }
    }

    /// The same event with `name` bound to `value`.
    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: Value) -> Self {
        self.arguments.insert(name.into(), value);
        self
    }

    /// The value bound to `name`.
    #[must_use]
    pub fn argument(&self, name: &str) -> Option<&Value> {
        self.arguments.get(name)
    }
}

/// Every instance, the configuration, and the time.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct World {
    /// Instances by id, ordered so the world serialises identically twice.
    pub entities: BTreeMap<EntityId, Instance>,
    /// Configuration in force, keyed `module.parameter`.
    pub config: BTreeMap<String, Value>,
    /// The current time, in milliseconds. Advanced only by the user.
    ///
    /// `number` on the wire; see the note on [`Value::Int`].
    #[ts(type = "number")]
    pub now: i64,
    /// The next ordinal for each entity type, so ids never repeat.
    ///
    /// Carried in the world rather than derived from what exists, so that
    /// removing an instance does not make the next creation reuse its id — two
    /// different things in one trace must never share a name.
    #[ts(type = "Record<string, number>")]
    pub next_ordinal: BTreeMap<String, u64>,
}

impl World {
    /// An empty world at time zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The instance with `id`.
    #[must_use]
    pub fn instance(&self, id: &EntityId) -> Option<&Instance> {
        self.entities.get(id)
    }

    /// Every instance of `entity`, in id order.
    pub fn instances_of<'a>(&'a self, entity: &'a str) -> impl Iterator<Item = &'a Instance> {
        self.entities.values().filter(move |instance| instance.entity == entity)
    }

    /// How many instances of `entity` exist.
    #[must_use]
    pub fn count_of(&self, entity: &str) -> usize {
        self.instances_of(entity).count()
    }

    /// Add `instance`, returning its id.
    pub fn insert(&mut self, instance: Instance) -> EntityId {
        let id = instance.id.clone();
        self.entities.insert(id.clone(), instance);
        id
    }

    /// Create an instance of `entity` with a fresh id.
    pub fn create(&mut self, entity: &str, module: &str) -> EntityId {
        let ordinal = self.next_ordinal.entry(entity.to_owned()).or_insert(1);
        let id = EntityId::new(entity, *ordinal);
        *ordinal += 1;
        self.insert(Instance::new(id.clone(), entity, module));
        id
    }

    /// Remove the instance with `id`, if it is there.
    pub fn remove(&mut self, id: &EntityId) -> Option<Instance> {
        self.entities.remove(id)
    }

    /// Set a field on an instance, if it exists.
    ///
    /// Returns the value that was there, so a trace can say what changed rather
    /// than only what it is now.
    pub fn set_field(&mut self, id: &EntityId, field: &str, value: Value) -> Option<Value> {
        let instance = self.entities.get_mut(id)?;
        let previous = instance.field(field);
        instance.set(field, value);
        Some(previous)
    }

    /// The configuration parameter `name`, looked up for `module`.
    ///
    /// Tried qualified first, then bare. A rule in `messaging` writing
    /// `config.max_attachment_bytes` means its own module's parameter, but a
    /// world seeded by hand may name it either way and both readings are what
    /// the user meant.
    #[must_use]
    pub fn config(&self, module: &str, name: &str) -> Value {
        self.config
            .get(&format!("{module}.{name}"))
            .or_else(|| self.config.get(name))
            .cloned()
            .unwrap_or(Value::Unknown)
    }

    /// Set a configuration parameter for `module`.
    pub fn set_config(&mut self, module: &str, name: &str, value: Value) {
        self.config.insert(format!("{module}.{name}"), value);
    }

    /// The same world with the clock at `now`.
    #[must_use]
    pub fn at(mut self, now: i64) -> Self {
        self.now = now;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_world_is_empty_at_time_zero() {
        let world = World::new();
        assert!(world.entities.is_empty());
        assert_eq!(world.now, 0);
    }

    #[test]
    fn creating_an_instance_numbers_it_from_one() {
        let mut world = World::new();
        assert_eq!(world.create("Message", "messaging").as_str(), "Message#1");
        assert_eq!(world.create("Message", "messaging").as_str(), "Message#2");
    }

    #[test]
    fn each_entity_type_is_numbered_separately() {
        let mut world = World::new();
        world.create("Message", "messaging");
        assert_eq!(world.create("Attachment", "messaging").as_str(), "Attachment#1");
    }

    #[test]
    fn an_id_is_never_reused_after_a_removal() {
        // Two different things in one trace must never share a name, or the
        // trace stops being readable at the point they collide.
        let mut world = World::new();
        let first = world.create("Message", "messaging");
        world.remove(&first);
        assert_eq!(world.create("Message", "messaging").as_str(), "Message#2");
    }

    #[test]
    fn instances_are_found_by_id_and_by_type() {
        let mut world = World::new();
        let id = world.create("Message", "messaging");
        world.create("Attachment", "messaging");
        assert_eq!(world.instance(&id).map(|i| i.entity.as_str()), Some("Message"));
        assert_eq!(world.count_of("Message"), 1);
        assert_eq!(world.count_of("Nothing"), 0);
    }

    #[test]
    fn instances_of_a_type_come_back_in_id_order() {
        // Every enumeration a step does has to be ordered, or two identical
        // runs produce different traces.
        let mut world = World::new();
        for _ in 0..3 {
            world.create("Message", "messaging");
        }
        let ids: Vec<&str> =
            world.instances_of("Message").map(|instance| instance.id.as_str()).collect();
        assert_eq!(ids, ["Message#1", "Message#2", "Message#3"]);
    }

    #[test]
    fn setting_a_field_reports_what_was_there_before() {
        // A trace says what changed, not only what it is now.
        let mut world = World::new();
        let id = world.create("Message", "messaging");
        let first = world.set_field(&id, "status", Value::Enum("visible".to_owned()));
        assert_eq!(first, Some(Value::Unknown), "it held nothing before");

        let second = world.set_field(&id, "status", Value::Enum("tombstoned".to_owned()));
        assert_eq!(second, Some(Value::Enum("visible".to_owned())));
    }

    #[test]
    fn setting_a_field_on_something_that_is_not_there_reports_nothing() {
        let mut world = World::new();
        assert_eq!(world.set_field(&EntityId::new("Ghost", 1), "x", Value::Null), None);
    }

    #[test]
    fn config_is_found_qualified_or_bare() {
        // A rule writes `config.loan_limit` meaning its own module's; a world
        // seeded by hand may name it either way, and both are what was meant.
        let mut world = World::new();
        world.set_config("lending", "loan_limit", Value::Int(5));
        assert_eq!(world.config("lending", "loan_limit"), Value::Int(5));

        world.config.insert("bare".to_owned(), Value::Int(7));
        assert_eq!(world.config("lending", "bare"), Value::Int(7));
    }

    #[test]
    fn a_modules_own_parameter_wins_over_a_bare_one_of_the_same_name() {
        let mut world = World::new();
        world.config.insert("loan_limit".to_owned(), Value::Int(1));
        world.set_config("lending", "loan_limit", Value::Int(5));
        assert_eq!(world.config("lending", "loan_limit"), Value::Int(5));
        assert_eq!(world.config("catalogue", "loan_limit"), Value::Int(1));
    }

    #[test]
    fn an_unset_parameter_is_unknown_rather_than_zero() {
        assert_eq!(World::new().config("m", "absent"), Value::Unknown);
    }

    #[test]
    fn the_clock_is_a_field_the_caller_sets() {
        // Never a reading of the system clock: a temporal rule is something you
        // step to, and yesterday's simulation reproduces today.
        let world = World::new().at(21 * 24 * 60 * 60 * 1000);
        assert_eq!(world.now, 1_814_400_000);
    }

    #[test]
    fn an_event_carries_its_arguments_by_name() {
        let event = Event::new("MemberBorrows", "lending")
            .with("member", Value::Ref(EntityId::new("Member", 1)))
            .with("copy", Value::Ref(EntityId::new("Copy", 3)));
        assert_eq!(event.argument("copy"), Some(&Value::Ref(EntityId::new("Copy", 3))));
        assert_eq!(event.argument("absent"), None);
        assert_eq!(event.arguments.len(), 2);
    }

    #[test]
    fn a_world_round_trips_through_the_wire_format() {
        // The browser holds the world and posts it back with each event, so
        // this is the actual contract rather than a convenience.
        let mut world = World::new().at(500);
        let id = world.create("Message", "messaging");
        world.set_field(&id, "body", Value::Str("hello".to_owned()));
        world.set_config("messaging", "limit", Value::Int(10));

        let json = serde_json::to_string(&world).expect("serialises");
        let back: World = serde_json::from_str(&json).expect("parses");
        assert_eq!(back, world);
    }

    #[test]
    fn two_identical_worlds_serialise_identically() {
        // Ordered maps throughout, so a snapshot test and a shared link both
        // mean something.
        let build = || {
            let mut world = World::new();
            for entity in ["Zebra", "Aardvark", "Moose"] {
                let id = world.create(entity, "m");
                world.set_field(&id, "b", Value::Int(2));
                world.set_field(&id, "a", Value::Int(1));
            }
            world
        };
        assert_eq!(
            serde_json::to_string(&build()).expect("serialises"),
            serde_json::to_string(&build()).expect("serialises")
        );
    }
}
