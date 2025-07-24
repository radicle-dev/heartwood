use localtime::{LocalDuration, LocalTime};
use nonempty::{nonempty, NonEmpty};

/// How often to run the "idle" task.
pub const IDLE_INTERVAL: LocalDuration = LocalDuration::from_secs(30);
/// How often to run the "gossip" task.
pub const GOSSIP_INTERVAL: LocalDuration = LocalDuration::from_secs(6);
/// How often to run the "announce" task.
pub const ANNOUNCE_INTERVAL: LocalDuration = LocalDuration::from_mins(60);
/// How often to run the "sync" task.
pub const SYNC_INTERVAL: LocalDuration = LocalDuration::from_secs(60);
/// How often to run the "prune" task.
pub const PRUNE_INTERVAL: LocalDuration = LocalDuration::from_mins(30);

/// The [`TaskTimers`] keeps track of the state of different timers and duration
/// intervals for when to emit a set of tasks.
///
/// [`TaskTimers::tick`] checks which intervals have elapsed and produces a
/// [`TaskTimerCommand`].
///
/// The [`TaskTimerCommand`] will provide a set of [`TimerEvent`]s which can be
/// used to [`TaskTimers::advance`] the clocks forward.
///
/// It also provides a set [`TaskEvent`]s which are tasks that the rest of the
/// system should execute.
pub struct TaskTimers {
    last_idle: LocalTime,
    last_gossip: LocalTime,
    last_sync: LocalTime,
    last_announce: LocalTime,
    last_prune: LocalTime,
    intervals: Intervals,
}

/// Interval durations used in [`TaskTimers`] for deciding when to issue
/// [`TaskEvent`]s and [`TimerEvent`]s.
///
/// The default values for each interval are given by:
///   - Idle: 30s
///   - Gossip: 6s
///   - Sync: 60s
///   - Announce: 60m
///   - Prune: 30m
pub struct Intervals {
    pub idle: LocalDuration,
    pub gossip: LocalDuration,
    pub sync: LocalDuration,
    pub announce: LocalDuration,
    pub prune: LocalDuration,
}

impl Default for Intervals {
    fn default() -> Self {
        Self {
            idle: IDLE_INTERVAL,
            gossip: GOSSIP_INTERVAL,
            sync: SYNC_INTERVAL,
            announce: ANNOUNCE_INTERVAL,
            prune: PRUNE_INTERVAL,
        }
    }
}

impl Intervals {
    /// The set of [`Intervals`] that never elapse.
    pub const NEVER: Intervals = Intervals {
        idle: LocalDuration::MAX,
        gossip: LocalDuration::MAX,
        sync: LocalDuration::MAX,
        announce: LocalDuration::MAX,
        prune: LocalDuration::MAX,
    };

    /// The set of [`Intervals`] that always elapse – as long as at least a
    /// millisecond of time passes.
    pub const ALWAYS: Intervals = Intervals {
        idle: LocalDuration::from_millis(0),
        gossip: LocalDuration::from_millis(0),
        sync: LocalDuration::from_millis(0),
        announce: LocalDuration::from_millis(0),
        prune: LocalDuration::from_millis(0),
    };

    /// Set the idle interval – issues tasks based on the node's idleness.
    pub fn with_idle(mut self, idle: LocalDuration) -> Self {
        self.idle = idle;
        self
    }

    /// Set the gossip interval – issues tasks for gossiping to the network.
    pub fn with_gossip(mut self, gossip: LocalDuration) -> Self {
        self.gossip = gossip;
        self
    }

    /// Set the sync interval – issues tasks for syncing with the network.
    pub fn with_sync(mut self, sync: LocalDuration) -> Self {
        self.sync = sync;
        self
    }

    /// Set the announce interval – issues tasks for announcing changes to the
    /// network.
    pub fn with_announce(mut self, announce: LocalDuration) -> Self {
        self.announce = announce;
        self
    }

    /// Set the prune interval – issues tasks for cleaning up stale data on the
    /// node.
    pub fn with_prune(mut self, prune: LocalDuration) -> Self {
        self.prune = prune;
        self
    }
}

/// Task events that are emitted by the [`TaskTimers`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TaskEvent {
    /// Maintain any configured persistent connections.
    MaintainPersistentConnections,
    /// Idle tasks.
    Idle(Idle),
    /// Gossip tasks.
    Gossip(Gossip),
    /// Synchronization tasks.
    Sync(Sync),
    /// Announcement tasks.
    Announce(Announce),
    /// Prune tasks.
    Prune(Prune),
}

impl TaskEvent {
    /// Produce all [`TaskEvent`]s.
    pub fn all() -> NonEmpty<Self> {
        let mut events = nonempty![TaskEvent::MaintainPersistentConnections];
        events.extend(Idle::all().map(Self::from));
        events.extend(Gossip::all().map(Self::from));
        events.extend(Sync::all().map(Self::from));
        events.extend(Announce::all().map(Self::from));
        events.extend(Prune::all().map(Self::from));
        events
    }
}

/// Events that describe how timer state should change.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimerEvent {
    /// Advance the idle clock of [`TaskTimers`].
    AdvanceIdle(LocalTime),
    /// Advance the gossip clock of [`TaskTimers`].
    AdvanceGossip(LocalTime),
    /// Advance the sync clock of [`TaskTimers`].
    AdvanceSync(LocalTime),
    /// Advance the announce clock of [`TaskTimers`].
    AdvanceAnnounce(LocalTime),
    /// Advance the prune clock of [`TaskTimers`].
    AdvancePrune(LocalTime),
}

/// Tasks for the machine to perform while it is idling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Idle {
    /// Maintain any existing connections.
    MaintainConnections,
    /// Issue a keep alive message to connections.
    KeepAlive,
    /// Disconnect any unresponsive peers.
    DisconnectUnresponsivePeers,
    /// Dequeue any queued fetches.
    DequeueFetches,
}

impl Idle {
    /// Return all the [`Idle`] variants.
    pub fn all() -> NonEmpty<Self> {
        nonempty![
            Self::MaintainConnections,
            Self::KeepAlive,
            Self::DisconnectUnresponsivePeers,
            Self::DequeueFetches,
        ]
    }
}

impl From<Idle> for TaskEvent {
    fn from(value: Idle) -> Self {
        TaskEvent::Idle(value)
    }
}

/// Gossip tasks for the machine to execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gossip {
    /// Relay announcements to connections.
    RelayAnnouncements,
}

impl Gossip {
    /// Return all the [`Gossip`] variants.
    pub fn all() -> NonEmpty<Self> {
        nonempty![Self::RelayAnnouncements]
    }
}

impl From<Gossip> for TaskEvent {
    fn from(value: Gossip) -> Self {
        TaskEvent::Gossip(value)
    }
}

/// Synchronization tasks for the machine to execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sync {
    /// Fetch missing repositories from connections. Missing repositories are
    /// repositories that are seeded, but not in storage.
    MissingRepositories,
}

impl Sync {
    /// Return all the [`Sync`] variants.
    pub fn all() -> NonEmpty<Self> {
        nonempty![Self::MissingRepositories]
    }
}

impl From<Sync> for TaskEvent {
    fn from(value: Sync) -> Self {
        TaskEvent::Sync(value)
    }
}

/// Announcement tasks for the machine to execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Announce {
    /// Announce inventory to connections.
    Inventory,
}

impl Announce {
    /// Return all the [`Announce`] variants.
    pub fn all() -> NonEmpty<Self> {
        nonempty![Self::Inventory]
    }
}

impl From<Announce> for TaskEvent {
    fn from(value: Announce) -> Self {
        TaskEvent::Announce(value)
    }
}

/// Pruning tasks for the machine to execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prune {
    /// Prune any routing entries that are no longer required.
    RoutingEntries,
    /// Prune any announcements that are no longer required.
    Announcements,
}

impl Prune {
    /// Return all the [`Prune`] variants.
    pub fn all() -> NonEmpty<Self> {
        nonempty![Self::RoutingEntries, Self::Announcements]
    }
}

impl From<Prune> for TaskEvent {
    fn from(value: Prune) -> Self {
        TaskEvent::Prune(value)
    }
}

/// The result of performing a [`TaskTimers::tick`].
pub struct TaskTimerCommand {
    /// The set of tasks that should be performed next.
    pub tasks: NonEmpty<TaskEvent>,
    /// The set of timers that should be used to advance the [`TaskTimers`]
    /// ([`TaskTimers::advance`]).
    pub timers: Vec<TimerEvent>,
}

impl TaskTimers {
    /// Construct the [`Timers`] starting all timers at `now`.
    pub fn new(now: LocalTime) -> Self {
        Self {
            last_idle: now,
            last_gossip: now,
            last_sync: now,
            last_announce: now,
            last_prune: now,
            intervals: Intervals::default(),
        }
    }

    /// Set the [`Intervals`] for the [`Timers`].
    pub fn with_intervals(mut self, intervals: Intervals) -> Self {
        self.intervals = intervals;
        self
    }

    /// Issue [`TaskEvent`]s and [`TimerEvent`]s based on the [`Intervals`] of
    /// the timers, and the time passed in `now`.
    ///
    /// The [`TimerEvent`]s can be used to advance the timers, using
    /// [`Timers::advance`].
    pub fn tick(&self, now: LocalTime) -> TaskTimerCommand {
        let mut tasks = NonEmpty::new(TaskEvent::MaintainPersistentConnections);
        let mut timers = Vec::with_capacity(5);

        if self.has_idle_elapsed(now) {
            tasks.extend(Idle::all().map(TaskEvent::from));
            timers.push(TimerEvent::AdvanceIdle(now));
        }

        if self.has_gossip_elapsed(now) {
            tasks.extend(Gossip::all().map(TaskEvent::from));
            timers.push(TimerEvent::AdvanceGossip(now));
        }

        if self.has_sync_elapsed(now) {
            tasks.extend(Sync::all().map(TaskEvent::from));
            timers.push(TimerEvent::AdvanceSync(now));
        }

        if self.has_announce_elapsed(now) {
            tasks.extend(Announce::all().map(TaskEvent::from));
            timers.push(TimerEvent::AdvanceAnnounce(now));
        }

        if self.has_prune_elapsed(now) {
            tasks.extend(Prune::all().map(TaskEvent::from));
            timers.push(TimerEvent::AdvancePrune(now));
        }

        TaskTimerCommand { tasks, timers }
    }

    /// Apply [`TimerEvent`]s to advance their clocks.
    pub fn advance(&mut self, events: &[TimerEvent]) {
        for event in events {
            self.advance_timer(event);
        }
    }

    fn advance_timer(&mut self, event: &TimerEvent) {
        match event {
            TimerEvent::AdvanceIdle(time) => self.last_idle = *time,
            TimerEvent::AdvanceGossip(time) => self.last_gossip = *time,
            TimerEvent::AdvanceSync(time) => self.last_sync = *time,
            TimerEvent::AdvanceAnnounce(time) => self.last_announce = *time,
            TimerEvent::AdvancePrune(time) => self.last_prune = *time,
        }
    }

    fn has_idle_elapsed(&self, now: LocalTime) -> bool {
        now - self.last_idle >= self.intervals.idle
    }

    fn has_gossip_elapsed(&self, now: LocalTime) -> bool {
        now - self.last_gossip >= self.intervals.gossip
    }

    fn has_sync_elapsed(&self, now: LocalTime) -> bool {
        now - self.last_sync >= self.intervals.sync
    }

    fn has_announce_elapsed(&self, now: LocalTime) -> bool {
        now - self.last_announce >= self.intervals.announce
    }

    fn has_prune_elapsed(&self, now: LocalTime) -> bool {
        now - self.last_prune >= self.intervals.prune
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use localtime::{LocalDuration, LocalTime};
    use nonempty::nonempty;

    // Helper function to create a test time
    fn test_time(secs: u64) -> LocalTime {
        LocalTime::from_secs(secs)
    }

    // Helper function to create test duration
    fn test_duration(secs: u64) -> LocalDuration {
        LocalDuration::from_secs(secs)
    }

    fn assert_tasks(got: &NonEmpty<TaskEvent>, expected: NonEmpty<TaskEvent>) {
        assert_eq!(
            got.len(),
            expected.len(),
            "extra task(s) emitted: {:?}",
            got
        );
        for t in expected {
            assert!(got.contains(&t), "missing expected task: {t:?}")
        }
    }

    fn assert_timers(got: &Vec<TimerEvent>, expected: Vec<TimerEvent>) {
        assert_eq!(
            got.len(),
            expected.len(),
            "extra timer(s) emitted: {:?}",
            got
        );
        for t in expected {
            assert!(got.contains(&t), "missing expected timer: {t:?}")
        }
    }

    fn gen_duration(seed: usize) -> LocalDuration {
        LocalDuration::from_secs(radicle::test::arbitrary::gen(seed))
    }

    #[test]
    fn test_tick_at_start_time_only_persistent_connections() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals::NEVER);

        let command = timers.tick(start_time);

        assert_eq!(
            command.tasks,
            nonempty![TaskEvent::MaintainPersistentConnections]
        );
        assert_eq!(command.timers, vec![]);
    }

    #[test]
    fn test_tick_idle_interval_elapsed() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals {
            idle: LocalDuration::from_secs(1),
            ..Intervals::NEVER
        });

        // Advance time by exactly the idle interval
        let tick_time = start_time + test_duration(2);
        let command = timers.tick(tick_time);

        let mut expected = nonempty![TaskEvent::MaintainPersistentConnections];
        expected.append(&mut Idle::all().map(TaskEvent::from).into());
        assert_tasks(&command.tasks, expected);
        assert_timers(&command.timers, vec![TimerEvent::AdvanceIdle(tick_time)]);
    }

    #[test]
    fn test_tick_gossip_only() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals {
            gossip: LocalDuration::from_secs(1),
            ..Intervals::NEVER
        });

        let tick_time = start_time + test_duration(2);
        let command = timers.tick(tick_time);

        let mut expected = nonempty![TaskEvent::MaintainPersistentConnections];
        expected.append(&mut Gossip::all().map(TaskEvent::from).into());
        assert_tasks(&command.tasks, expected);
        assert_timers(&command.timers, vec![TimerEvent::AdvanceGossip(tick_time)]);
    }

    #[test]
    fn test_tick_sync_interval_elapsed() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals {
            sync: LocalDuration::from_secs(1),
            ..Intervals::NEVER
        });

        let tick_time = start_time + test_duration(2);
        let command = timers.tick(tick_time);

        let mut expected = nonempty![TaskEvent::MaintainPersistentConnections];
        expected.append(&mut Sync::all().map(TaskEvent::from).into());
        assert_tasks(&command.tasks, expected);
        assert_timers(&command.timers, vec![TimerEvent::AdvanceSync(tick_time)]);
    }

    #[test]
    fn test_tick_announce_interval_elapsed() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals {
            announce: LocalDuration::from_secs(1),
            ..Intervals::NEVER
        });

        let tick_time = start_time + test_duration(2);
        let command = timers.tick(tick_time);

        let mut expected = nonempty![TaskEvent::MaintainPersistentConnections];
        expected.append(&mut Announce::all().map(TaskEvent::from).into());
        assert_tasks(&command.tasks, expected);
        assert_timers(
            &command.timers,
            vec![TimerEvent::AdvanceAnnounce(tick_time)],
        );
    }

    #[test]
    fn test_tick_prune_interval_elapsed() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals {
            prune: LocalDuration::from_secs(1),
            ..Intervals::NEVER
        });

        let tick_time = start_time + test_duration(2);
        let command = timers.tick(tick_time);

        let mut expected = nonempty![TaskEvent::MaintainPersistentConnections];
        expected.append(&mut Prune::all().map(TaskEvent::from).into());
        assert_tasks(&command.tasks, expected);
        assert_timers(&command.timers, vec![TimerEvent::AdvancePrune(tick_time)]);
    }

    #[test]
    fn test_tick_multiple_intervals_elapsed() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals {
            idle: LocalDuration::from_secs(1),
            gossip: LocalDuration::from_secs(1),
            sync: LocalDuration::from_secs(1),
            ..Intervals::NEVER
        });

        let tick_time = start_time + test_duration(2);
        let command = timers.tick(tick_time);

        let mut expected = nonempty![TaskEvent::MaintainPersistentConnections];
        expected.append(&mut Idle::all().map(TaskEvent::from).into());
        expected.append(&mut Gossip::all().map(TaskEvent::from).into());
        expected.append(&mut Sync::all().map(TaskEvent::from).into());
        assert_tasks(&command.tasks, expected);
        assert_timers(
            &command.timers,
            vec![
                TimerEvent::AdvanceIdle(tick_time),
                TimerEvent::AdvanceGossip(tick_time),
                TimerEvent::AdvanceSync(tick_time),
            ],
        );
    }

    #[test]
    fn test_tick_all_intervals_elapsed() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals::ALWAYS);

        let tick_time = start_time + test_duration(1);
        let command = timers.tick(tick_time);

        assert_tasks(&command.tasks, TaskEvent::all());
        assert_timers(
            &command.timers,
            vec![
                TimerEvent::AdvanceIdle(tick_time),
                TimerEvent::AdvanceGossip(tick_time),
                TimerEvent::AdvanceSync(tick_time),
                TimerEvent::AdvanceAnnounce(tick_time),
                TimerEvent::AdvancePrune(tick_time),
            ],
        );
    }

    #[test]
    fn test_advance_single_timer() {
        let start_time = test_time(1000);
        let mut timers = TaskTimers::new(start_time);

        let new_time = test_time(2000);
        let events = vec![TimerEvent::AdvanceIdle(new_time)];

        timers.advance(&events);

        assert_eq!(timers.last_idle, new_time);
        assert_eq!(timers.last_gossip, start_time);
        assert_eq!(timers.last_sync, start_time);
        assert_eq!(timers.last_announce, start_time);
        assert_eq!(timers.last_prune, start_time);
    }

    #[test]
    fn test_advance_multiple_timers() {
        let start_time = test_time(1000);
        let mut timers = TaskTimers::new(start_time);

        let new_time = test_time(2000);
        let events = vec![
            TimerEvent::AdvanceIdle(new_time),
            TimerEvent::AdvanceGossip(new_time),
            TimerEvent::AdvanceSync(new_time),
        ];

        timers.advance(&events);

        assert_eq!(timers.last_idle, new_time);
        assert_eq!(timers.last_gossip, new_time);
        assert_eq!(timers.last_sync, new_time);
        assert_eq!(timers.last_announce, start_time);
        assert_eq!(timers.last_prune, start_time);
    }

    #[test]
    fn test_advance_all_timers() {
        let start_time = test_time(1000);
        let mut timers = TaskTimers::new(start_time);

        let new_time = test_time(2000);
        let events = vec![
            TimerEvent::AdvanceIdle(new_time),
            TimerEvent::AdvanceGossip(new_time),
            TimerEvent::AdvanceSync(new_time),
            TimerEvent::AdvanceAnnounce(new_time),
            TimerEvent::AdvancePrune(new_time),
        ];

        timers.advance(&events);

        assert_eq!(timers.last_idle, new_time);
        assert_eq!(timers.last_gossip, new_time);
        assert_eq!(timers.last_sync, new_time);
        assert_eq!(timers.last_announce, new_time);
        assert_eq!(timers.last_prune, new_time);
    }

    #[test]
    fn test_advance_empty_events() {
        let start_time = test_time(1000);
        let mut timers = TaskTimers::new(start_time);

        timers.advance(&[]);

        // All timers should remain unchanged
        assert_eq!(timers.last_idle, start_time);
        assert_eq!(timers.last_gossip, start_time);
        assert_eq!(timers.last_sync, start_time);
        assert_eq!(timers.last_announce, start_time);
        assert_eq!(timers.last_prune, start_time);
    }

    #[test]
    fn test_exactly_at_interval_boundary() {
        let start_time = test_time(1000);
        let timers = TaskTimers::new(start_time).with_intervals(Intervals {
            idle: LocalDuration::from_secs(30),
            ..Intervals::NEVER
        });

        // Test exactly at the idle boundary
        let boundary_time = start_time + test_duration(30);
        let command = timers.tick(boundary_time);

        assert!(command.tasks.contains(&TaskEvent::Idle(Idle::KeepAlive)));
    }

    #[test]
    fn test_just_before_interval_boundary() {
        let start_time = test_time(1000);
        // Use different intervals to avoid gossip triggering when testing idle boundary
        let intervals = Intervals {
            idle: test_duration(30),
            ..Intervals::NEVER
        };
        let timers = TaskTimers::new(start_time).with_intervals(intervals);

        // Test just before the idle boundary
        let almost_boundary = start_time + test_duration(29);
        let command = timers.tick(almost_boundary);

        // Should not trigger idle tasks, only persistent connections
        assert_tasks(
            &command.tasks,
            nonempty![TaskEvent::MaintainPersistentConnections],
        );
    }

    #[test]
    fn test_tick_after_advance() {
        let start_time = test_time(1000);
        let mut timers = TaskTimers::new(start_time).with_intervals(Intervals {
            idle: LocalDuration::from_secs(30),
            gossip: LocalDuration::from_secs(30),
            ..Intervals::NEVER
        });

        // First tick - should trigger idle and gossip
        let first_tick = start_time + test_duration(30);
        let command1 = timers.tick(first_tick);

        // Advance the timers
        timers.advance(&command1.timers);

        // Second tick at same time - should not trigger anything again
        let command2 = timers.tick(first_tick);

        assert!(!command2.tasks.contains(&TaskEvent::Idle(Idle::KeepAlive)));
        assert!(!command2
            .tasks
            .contains(&TaskEvent::Gossip(Gossip::RelayAnnouncements)));
        assert_eq!(
            command2.tasks,
            nonempty![TaskEvent::MaintainPersistentConnections]
        );
    }

    #[test]
    fn test_tick_advance_tick_cycle() {
        let start_time = test_time(1000);
        let mut timers = TaskTimers::new(start_time).with_intervals(Intervals {
            idle: LocalDuration::from_secs(30),
            ..Intervals::NEVER
        });

        // First cycle
        let tick1 = start_time + test_duration(30);
        let command1 = timers.tick(tick1);
        timers.advance(&command1.timers);

        // Second cycle - advance by another idle interval
        let tick2 = tick1 + test_duration(30);
        let command2 = timers.tick(tick2);

        // Should trigger idle tasks again
        assert!(command2.tasks.contains(&TaskEvent::Idle(Idle::KeepAlive)));
    }

    #[test]
    fn test_custom_intervals_behavior() {
        let start_time = test_time(1000);
        let custom_intervals = Intervals {
            idle: test_duration(1),   // Very short idle interval
            gossip: test_duration(2), // Short gossip interval
            ..Intervals::NEVER
        };
        let timers = TaskTimers::new(start_time).with_intervals(custom_intervals);

        // Test that custom intervals are respected
        let tick_time = start_time + test_duration(1);
        let command = timers.tick(tick_time);

        assert!(command.tasks.contains(&TaskEvent::Idle(Idle::KeepAlive)));
        // Gossip should not trigger yet (interval is 2s)
        assert!(!command
            .tasks
            .contains(&TaskEvent::Gossip(Gossip::RelayAnnouncements)));

        // Test gossip triggers at its interval
        let gossip_time = start_time + test_duration(2);
        let command2 = timers.tick(gossip_time);

        assert!(command2
            .tasks
            .contains(&TaskEvent::Gossip(Gossip::RelayAnnouncements)));
    }

    #[test]
    fn test_zero_duration_intervals() {
        let start_time = test_time(1000);
        let zero_intervals = Intervals {
            idle: LocalDuration::from_secs(0),
            gossip: LocalDuration::from_secs(0),
            ..Intervals::NEVER
        };
        let timers = TaskTimers::new(start_time).with_intervals(zero_intervals);

        // Even with zero duration, should trigger immediately
        let command = timers.tick(start_time);

        assert!(command.tasks.contains(&TaskEvent::Idle(Idle::KeepAlive)));
        assert!(command
            .tasks
            .contains(&TaskEvent::Gossip(Gossip::RelayAnnouncements)));
    }

    #[test]
    fn test_partial_timer_advance() {
        let start_time = test_time(1000);
        let mut timers = TaskTimers::new(start_time);

        let new_time = test_time(2000);

        // Only advance some timers (not all that might have been generated)
        let partial_events = vec![
            TimerEvent::AdvanceIdle(new_time),
            TimerEvent::AdvanceGossip(new_time),
            // Intentionally omit sync, announce, and prune
        ];

        timers.advance(&partial_events);

        // Only idle and gossip should be advanced
        assert_eq!(timers.last_idle, new_time);
        assert_eq!(timers.last_gossip, new_time);
        assert_eq!(timers.last_sync, start_time);
        assert_eq!(timers.last_announce, start_time);
        assert_eq!(timers.last_prune, start_time);
    }

    #[test]
    fn test_property_timer_monotonicity() {
        let start_time = test_time(1000);
        let intervals = Intervals {
            idle: gen_duration(0),
            gossip: gen_duration(1),
            sync: gen_duration(2),
            announce: gen_duration(3),
            prune: gen_duration(4),
        };
        let mut timers = TaskTimers::new(start_time).with_intervals(intervals);

        let mut current_time = start_time;

        for _ in 0..100 {
            current_time = current_time + test_duration(10);
            let command = timers.tick(current_time);

            // Advance all timers
            timers.advance(&command.timers);

            // Verify all timers are monotonically increasing
            assert!(timers.last_idle >= start_time);
            assert!(timers.last_gossip >= start_time);
            assert!(timers.last_sync >= start_time);
            assert!(timers.last_announce >= start_time);
            assert!(timers.last_prune >= start_time);
        }
    }
}
