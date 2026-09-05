//! Runtime-only browser control lease.
//!
//! Human Takeover transfers ACTION CONTROL, not browser process ownership.
//! State is per logical session, never persisted, and contains no secrets,
//! filesystem paths, or backend/vendor fields.

use crate::tools::browser_types::BrowserSessionKey;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

/// Who currently holds action control of a headed browser session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserControlOwner {
    Agent,
    Human,
}

/// Control-lease phase. Timeout never auto-returns Agent ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserTakeoverPhase {
    AgentControlled,
    TakeoverRequested,
    HumanControlled,
    TimedOut,
    Resynchronizing,
    BrowserLost,
}

/// Why takeover was requested. Detection itself belongs to B3.6, not B3.4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeoverReason {
    Captcha,
    Mfa,
    QrLogin,
    SmsVerification,
    Sso,
    ManualCorrection,
    UnexpectedModal,
    ExplicitUserRequest,
    Unknown,
}

/// Safe public snapshot of per-session control state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserControlState {
    pub owner: BrowserControlOwner,
    pub phase: BrowserTakeoverPhase,
    pub generation: u64,
    pub in_flight: u32,
    pub pending_takeover: bool,
    pub reason: Option<TakeoverReason>,
    pub since: SystemTime,
}

impl BrowserControlState {
    fn initial() -> Self {
        Self {
            owner: BrowserControlOwner::Agent,
            phase: BrowserTakeoverPhase::AgentControlled,
            generation: 0,
            in_flight: 0,
            pending_takeover: false,
            reason: None,
            since: SystemTime::now(),
        }
    }
}

/// Classification used at the Runtime choke point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentOpClass {
    Observe,
    Mutate,
}

/// Backend-neutral control errors. Runtime maps these onto `BrowserBackendError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserControlError {
    HumanTakeoverActive {
        phase: BrowserTakeoverPhase,
    },
    UnsupportedHeadless,
    StaleAssumptions,
    BrowserLost {
        generation: u64,
    },
    Rejected {
        detail: &'static str,
    },
}

struct SessionControl {
    owner: BrowserControlOwner,
    phase: BrowserTakeoverPhase,
    generation: u64,
    in_flight: u32,
    pending_takeover: bool,
    reason: Option<TakeoverReason>,
    since: SystemTime,
    headless: Option<bool>,
    timeout_at: Option<Instant>,
}

impl SessionControl {
    fn initial() -> Self {
        Self {
            owner: BrowserControlOwner::Agent,
            phase: BrowserTakeoverPhase::AgentControlled,
            generation: 0,
            in_flight: 0,
            pending_takeover: false,
            reason: None,
            since: SystemTime::now(),
            headless: None,
            timeout_at: None,
        }
    }

    fn snapshot(&self) -> BrowserControlState {
        BrowserControlState {
            owner: self.owner,
            phase: self.phase,
            generation: self.generation,
            in_flight: self.in_flight,
            pending_takeover: self.pending_takeover,
            reason: self.reason,
            since: self.since,
        }
    }

    fn expire_timeout_if_due(&mut self) {
        if self.phase == BrowserTakeoverPhase::HumanControlled {
            if let Some(deadline) = self.timeout_at {
                if Instant::now() >= deadline {
                    self.phase = BrowserTakeoverPhase::TimedOut;
                    self.owner = BrowserControlOwner::Human;
                    self.pending_takeover = false;
                    self.timeout_at = None;
                    self.since = SystemTime::now();
                }
            }
        }
    }

    fn enter_human_controlled(&mut self, reason: TakeoverReason) {
        self.owner = BrowserControlOwner::Human;
        self.phase = BrowserTakeoverPhase::HumanControlled;
        self.pending_takeover = false;
        self.reason = Some(reason);
        self.since = SystemTime::now();
        self.timeout_at = None;
    }

    fn enter_resynchronizing(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.owner = BrowserControlOwner::Agent;
        self.phase = BrowserTakeoverPhase::Resynchronizing;
        self.pending_takeover = false;
        self.timeout_at = None;
        self.since = SystemTime::now();
    }
}

/// Per-session control registry owned by `BrowserRuntime`.
pub struct BrowserControlRegistry {
    inner: Mutex<HashMap<BrowserSessionKey, SessionControl>>,
}

impl BrowserControlRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<BrowserSessionKey, SessionControl>> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub fn get(&self, key: &BrowserSessionKey) -> BrowserControlState {
        let mut map = self.lock();
        match map.get_mut(key) {
            Some(state) => {
                state.expire_timeout_if_due();
                state.snapshot()
            }
            None => BrowserControlState::initial(),
        }
    }

    pub fn remember_headless(&self, key: &BrowserSessionKey, headless: bool) {
        let mut map = self.lock();
        let state = map
            .entry(key.clone())
            .or_insert_with(SessionControl::initial);
        state.headless = Some(headless);
    }

    pub fn begin_agent_operation(
        self: &Arc<Self>,
        key: &BrowserSessionKey,
        class: AgentOpClass,
    ) -> Result<AgentOpPermit, BrowserControlError> {
        let mut map = self.lock();
        let state = map
            .entry(key.clone())
            .or_insert_with(SessionControl::initial);
        state.expire_timeout_if_due();
        match state.phase {
            BrowserTakeoverPhase::HumanControlled
            | BrowserTakeoverPhase::TakeoverRequested
            | BrowserTakeoverPhase::TimedOut => {
                return Err(BrowserControlError::HumanTakeoverActive { phase: state.phase });
            }
            BrowserTakeoverPhase::BrowserLost => {
                return Err(BrowserControlError::BrowserLost {
                    generation: state.generation,
                });
            }
            BrowserTakeoverPhase::Resynchronizing if class != AgentOpClass::Observe => {
                return Err(BrowserControlError::StaleAssumptions);
            }
            BrowserTakeoverPhase::AgentControlled | BrowserTakeoverPhase::Resynchronizing => {}
        }
        state.in_flight = state.in_flight.saturating_add(1);
        Ok(AgentOpPermit {
            registry: Arc::clone(self),
            key: key.clone(),
        })
    }

    fn end_operation(&self, key: &BrowserSessionKey) {
        let mut map = self.lock();
        let Some(state) = map.get_mut(key) else {
            return;
        };
        state.in_flight = state.in_flight.saturating_sub(1);
        if state.phase == BrowserTakeoverPhase::TakeoverRequested
            && state.pending_takeover
            && state.in_flight == 0
        {
            let reason = state.reason.unwrap_or(TakeoverReason::Unknown);
            state.enter_human_controlled(reason);
        }
    }

    pub fn request_human_takeover(
        &self,
        key: &BrowserSessionKey,
        requested_headless: bool,
        reason: TakeoverReason,
    ) -> Result<BrowserControlState, BrowserControlError> {
        let mut map = self.lock();
        let state = map
            .entry(key.clone())
            .or_insert_with(SessionControl::initial);
        state.expire_timeout_if_due();
        let headless = state.headless.unwrap_or(requested_headless);
        if headless {
            return Err(BrowserControlError::UnsupportedHeadless);
        }
        match state.phase {
            BrowserTakeoverPhase::Resynchronizing => {
                return Err(BrowserControlError::Rejected {
                    detail: "takeover cannot start while the session is resynchronizing",
                });
            }
            BrowserTakeoverPhase::BrowserLost => {
                return Err(BrowserControlError::BrowserLost {
                    generation: state.generation,
                });
            }
            BrowserTakeoverPhase::HumanControlled | BrowserTakeoverPhase::TimedOut => {
                if state.reason.is_none() {
                    state.reason = Some(reason);
                }
                return Ok(state.snapshot());
            }
            BrowserTakeoverPhase::TakeoverRequested => {
                if state.reason.is_none() {
                    state.reason = Some(reason);
                }
                return Ok(state.snapshot());
            }
            BrowserTakeoverPhase::AgentControlled => {}
        }
        state.reason = Some(reason);
        if state.in_flight == 0 {
            state.enter_human_controlled(reason);
        } else {
            state.owner = BrowserControlOwner::Agent;
            state.phase = BrowserTakeoverPhase::TakeoverRequested;
            state.pending_takeover = true;
            state.since = SystemTime::now();
        }
        Ok(state.snapshot())
    }

    pub fn release_human_takeover(
        &self,
        key: &BrowserSessionKey,
    ) -> Result<BrowserControlState, BrowserControlError> {
        let mut map = self.lock();
        let state = map
            .entry(key.clone())
            .or_insert_with(SessionControl::initial);
        state.expire_timeout_if_due();
        match state.phase {
            BrowserTakeoverPhase::BrowserLost => {
                let generation = state.generation;
                map.remove(key);
                return Err(BrowserControlError::BrowserLost { generation });
            }
            BrowserTakeoverPhase::HumanControlled | BrowserTakeoverPhase::TimedOut => {
                state.enter_resynchronizing();
            }
            BrowserTakeoverPhase::TakeoverRequested => {
                // Human ownership was never granted.
                state.owner = BrowserControlOwner::Agent;
                state.phase = BrowserTakeoverPhase::AgentControlled;
                state.pending_takeover = false;
                state.reason = None;
                state.timeout_at = None;
                state.since = SystemTime::now();
            }
            BrowserTakeoverPhase::AgentControlled | BrowserTakeoverPhase::Resynchronizing => {}
        }
        Ok(state.snapshot())
    }

    pub fn cancel_human_takeover(
        &self,
        key: &BrowserSessionKey,
    ) -> Result<BrowserControlState, BrowserControlError> {
        let mut map = self.lock();
        let state = map
            .entry(key.clone())
            .or_insert_with(SessionControl::initial);
        state.expire_timeout_if_due();
        match state.phase {
            BrowserTakeoverPhase::BrowserLost => {
                let generation = state.generation;
                map.remove(key);
                return Err(BrowserControlError::BrowserLost { generation });
            }
            BrowserTakeoverPhase::TakeoverRequested => {
                state.owner = BrowserControlOwner::Agent;
                state.phase = BrowserTakeoverPhase::AgentControlled;
                state.pending_takeover = false;
                state.reason = None;
                state.timeout_at = None;
                state.since = SystemTime::now();
            }
            BrowserTakeoverPhase::HumanControlled | BrowserTakeoverPhase::TimedOut => {
                state.enter_resynchronizing();
            }
            BrowserTakeoverPhase::AgentControlled | BrowserTakeoverPhase::Resynchronizing => {}
        }
        Ok(state.snapshot())
    }

    pub fn complete_resync(&self, key: &BrowserSessionKey) {
        let mut map = self.lock();
        let Some(state) = map.get_mut(key) else {
            return;
        };
        if state.phase == BrowserTakeoverPhase::Resynchronizing {
            state.owner = BrowserControlOwner::Agent;
            state.phase = BrowserTakeoverPhase::AgentControlled;
            state.pending_takeover = false;
            state.timeout_at = None;
            state.since = SystemTime::now();
        }
    }

    pub fn note_browser_lost(&self, key: &BrowserSessionKey) {
        let mut map = self.lock();
        let Some(state) = map.get_mut(key) else {
            return;
        };
        match state.phase {
            BrowserTakeoverPhase::HumanControlled
            | BrowserTakeoverPhase::TakeoverRequested
            | BrowserTakeoverPhase::TimedOut => {
                state.phase = BrowserTakeoverPhase::BrowserLost;
                state.pending_takeover = false;
                state.timeout_at = None;
                state.since = SystemTime::now();
            }
            BrowserTakeoverPhase::AgentControlled
            | BrowserTakeoverPhase::Resynchronizing
            | BrowserTakeoverPhase::BrowserLost => {}
        }
    }

    pub fn blocks_crash_recovery(&self, key: &BrowserSessionKey) -> bool {
        matches!(
            self.get(key).phase,
            BrowserTakeoverPhase::HumanControlled
                | BrowserTakeoverPhase::TakeoverRequested
                | BrowserTakeoverPhase::TimedOut
                | BrowserTakeoverPhase::BrowserLost
        )
    }

    pub fn remove(&self, key: &BrowserSessionKey) {
        self.lock().remove(key);
    }

    pub fn contains(&self, key: &BrowserSessionKey) -> bool {
        self.lock().contains_key(key)
    }

    /// Test/internal: HumanControlled → TimedOut without returning Agent ownership.
    pub fn force_timeout(&self, key: &BrowserSessionKey) -> Result<BrowserControlState, BrowserControlError> {
        let mut map = self.lock();
        let Some(state) = map.get_mut(key) else {
            return Err(BrowserControlError::Rejected {
                detail: "no control state to time out",
            });
        };
        match state.phase {
            BrowserTakeoverPhase::HumanControlled => {
                state.phase = BrowserTakeoverPhase::TimedOut;
                state.owner = BrowserControlOwner::Human;
                state.pending_takeover = false;
                state.timeout_at = None;
                state.since = SystemTime::now();
                Ok(state.snapshot())
            }
            BrowserTakeoverPhase::TimedOut => Ok(state.snapshot()),
            other => Err(BrowserControlError::Rejected {
                detail: match other {
                    BrowserTakeoverPhase::AgentControlled => "timeout requires HumanControlled",
                    _ => "timeout cannot apply in the current phase",
                },
            }),
        }
    }
}

impl Default for BrowserControlRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII in-flight permit. Drop always decrements, including panic unwind.
pub struct AgentOpPermit {
    registry: Arc<BrowserControlRegistry>,
    key: BrowserSessionKey,
}

impl Drop for AgentOpPermit {
    fn drop(&mut self) {
        self.registry.end_operation(&self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn key(id: &str) -> BrowserSessionKey {
        BrowserSessionKey::new(id).unwrap()
    }

    fn registry() -> Arc<BrowserControlRegistry> {
        Arc::new(BrowserControlRegistry::new())
    }

    #[test]
    fn initial_state_is_agent_controlled() {
        let reg = registry();
        let state = reg.get(&key("s1"));
        assert_eq!(state.owner, BrowserControlOwner::Agent);
        assert_eq!(state.phase, BrowserTakeoverPhase::AgentControlled);
        assert_eq!(state.generation, 0);
        assert_eq!(state.in_flight, 0);
        assert!(!state.pending_takeover);
        assert!(state.reason.is_none());
    }

    #[test]
    fn request_with_zero_in_flight_grants_human_control() {
        let reg = registry();
        let k = key("s1");
        let state = reg
            .request_human_takeover(&k, false, TakeoverReason::ExplicitUserRequest)
            .unwrap();
        assert_eq!(state.phase, BrowserTakeoverPhase::HumanControlled);
        assert_eq!(state.owner, BrowserControlOwner::Human);
        assert!(!state.pending_takeover);
        assert_eq!(state.reason, Some(TakeoverReason::ExplicitUserRequest));
    }

    #[test]
    fn request_with_in_flight_enters_takeover_requested() {
        let reg = registry();
        let k = key("s1");
        let _permit = reg
            .begin_agent_operation(&k, AgentOpClass::Mutate)
            .unwrap();
        let state = reg
            .request_human_takeover(&k, false, TakeoverReason::Captcha)
            .unwrap();
        assert_eq!(state.phase, BrowserTakeoverPhase::TakeoverRequested);
        assert!(state.pending_takeover);
        assert_eq!(state.in_flight, 1);
        assert_eq!(state.owner, BrowserControlOwner::Agent);
    }

    #[test]
    fn pending_grant_happens_when_in_flight_reaches_zero() {
        let reg = registry();
        let k = key("s1");
        let permit = reg
            .begin_agent_operation(&k, AgentOpClass::Observe)
            .unwrap();
        reg.request_human_takeover(&k, false, TakeoverReason::Mfa)
            .unwrap();
        assert_eq!(
            reg.get(&k).phase,
            BrowserTakeoverPhase::TakeoverRequested
        );
        drop(permit);
        let state = reg.get(&k);
        assert_eq!(state.phase, BrowserTakeoverPhase::HumanControlled);
        assert_eq!(state.owner, BrowserControlOwner::Human);
        assert_eq!(state.in_flight, 0);
        assert!(!state.pending_takeover);
    }

    #[test]
    fn repeated_request_is_idempotent() {
        let reg = registry();
        let k = key("s1");
        let first = reg
            .request_human_takeover(&k, false, TakeoverReason::QrLogin)
            .unwrap();
        let second = reg
            .request_human_takeover(&k, false, TakeoverReason::Unknown)
            .unwrap();
        assert_eq!(first.phase, second.phase);
        assert_eq!(first.generation, second.generation);
        assert_eq!(second.reason, Some(TakeoverReason::QrLogin));
    }

    #[test]
    fn new_operation_rejected_after_takeover_requested() {
        let reg = registry();
        let k = key("s1");
        let _permit = reg
            .begin_agent_operation(&k, AgentOpClass::Mutate)
            .unwrap();
        reg.request_human_takeover(&k, false, TakeoverReason::Sso)
            .unwrap();
        assert!(matches!(
            reg.begin_agent_operation(&k, AgentOpClass::Observe),
            Err(BrowserControlError::HumanTakeoverActive {
                phase: BrowserTakeoverPhase::TakeoverRequested
            })
        ));
        assert_eq!(reg.get(&k).in_flight, 1);
    }

    #[test]
    fn human_controlled_blocks_observe_and_mutate() {
        let reg = registry();
        let k = key("s1");
        reg.request_human_takeover(&k, false, TakeoverReason::ManualCorrection)
            .unwrap();
        assert!(matches!(
            reg.begin_agent_operation(&k, AgentOpClass::Observe),
            Err(BrowserControlError::HumanTakeoverActive {
                phase: BrowserTakeoverPhase::HumanControlled
            })
        ));
        assert!(matches!(
            reg.begin_agent_operation(&k, AgentOpClass::Mutate),
            Err(BrowserControlError::HumanTakeoverActive { .. })
        ));
        assert_eq!(reg.get(&k).in_flight, 0);
    }

    #[test]
    fn release_increments_generation_and_enters_resynchronizing() {
        let reg = registry();
        let k = key("s1");
        reg.request_human_takeover(&k, false, TakeoverReason::ExplicitUserRequest)
            .unwrap();
        let released = reg.release_human_takeover(&k).unwrap();
        assert_eq!(released.phase, BrowserTakeoverPhase::Resynchronizing);
        assert_eq!(released.generation, 1);
        assert_eq!(released.owner, BrowserControlOwner::Agent);
        let again = reg.release_human_takeover(&k).unwrap();
        assert_eq!(again.phase, BrowserTakeoverPhase::Resynchronizing);
        assert_eq!(again.generation, 1);
    }

    #[test]
    fn resync_allows_observe_then_returns_agent_control() {
        let reg = registry();
        let k = key("s1");
        reg.request_human_takeover(&k, false, TakeoverReason::SmsVerification)
            .unwrap();
        reg.release_human_takeover(&k).unwrap();
        assert!(matches!(
            reg.begin_agent_operation(&k, AgentOpClass::Mutate),
            Err(BrowserControlError::StaleAssumptions)
        ));
        let permit = reg
            .begin_agent_operation(&k, AgentOpClass::Observe)
            .unwrap();
        drop(permit);
        reg.complete_resync(&k);
        let state = reg.get(&k);
        assert_eq!(state.phase, BrowserTakeoverPhase::AgentControlled);
        assert_eq!(state.generation, 1);
        assert!(
            reg.begin_agent_operation(&k, AgentOpClass::Mutate).is_ok(),
            "mutation after observe should be allowed"
        );
    }

    #[test]
    fn request_during_resync_is_rejected() {
        let reg = registry();
        let k = key("s1");
        reg.request_human_takeover(&k, false, TakeoverReason::Unknown)
            .unwrap();
        reg.release_human_takeover(&k).unwrap();
        let err = reg
            .request_human_takeover(&k, false, TakeoverReason::Unknown)
            .unwrap_err();
        assert!(matches!(err, BrowserControlError::Rejected { .. }));
    }

    #[test]
    fn headless_request_fails_without_granting_control() {
        let reg = registry();
        let k = key("s1");
        let err = reg
            .request_human_takeover(&k, true, TakeoverReason::ExplicitUserRequest)
            .unwrap_err();
        assert_eq!(err, BrowserControlError::UnsupportedHeadless);
        assert_eq!(reg.get(&k).phase, BrowserTakeoverPhase::AgentControlled);
    }

    #[test]
    fn recorded_headless_wins_over_request_argument() {
        let reg = registry();
        let k = key("s1");
        reg.remember_headless(&k, true);
        let err = reg
            .request_human_takeover(&k, false, TakeoverReason::ExplicitUserRequest)
            .unwrap_err();
        assert_eq!(err, BrowserControlError::UnsupportedHeadless);
        reg.remember_headless(&k, false);
        let state = reg
            .request_human_takeover(&k, true, TakeoverReason::ExplicitUserRequest)
            .unwrap();
        assert_eq!(state.phase, BrowserTakeoverPhase::HumanControlled);
    }

    #[test]
    fn timeout_blocks_agent_and_does_not_return_ownership() {
        let reg = registry();
        let k = key("s1");
        reg.request_human_takeover(&k, false, TakeoverReason::Mfa)
            .unwrap();
        let timed = reg.force_timeout(&k).unwrap();
        assert_eq!(timed.phase, BrowserTakeoverPhase::TimedOut);
        assert_eq!(timed.owner, BrowserControlOwner::Human);
        assert!(matches!(
            reg.begin_agent_operation(&k, AgentOpClass::Observe),
            Err(BrowserControlError::HumanTakeoverActive {
                phase: BrowserTakeoverPhase::TimedOut
            })
        ));
        let released = reg.release_human_takeover(&k).unwrap();
        assert_eq!(released.phase, BrowserTakeoverPhase::Resynchronizing);
        assert_eq!(released.generation, 1);
        assert_ne!(released.phase, BrowserTakeoverPhase::AgentControlled);
    }

    #[test]
    fn cancel_before_grant_returns_agent_without_resync() {
        let reg = registry();
        let k = key("s1");
        let _permit = reg
            .begin_agent_operation(&k, AgentOpClass::Mutate)
            .unwrap();
        reg.request_human_takeover(&k, false, TakeoverReason::Unknown)
            .unwrap();
        let cancelled = reg.cancel_human_takeover(&k).unwrap();
        assert_eq!(cancelled.phase, BrowserTakeoverPhase::AgentControlled);
        assert_eq!(cancelled.generation, 0);
        assert!(!cancelled.pending_takeover);
    }

    #[test]
    fn cancel_after_human_control_requires_resync() {
        let reg = registry();
        let k = key("s1");
        reg.request_human_takeover(&k, false, TakeoverReason::Unknown)
            .unwrap();
        let cancelled = reg.cancel_human_takeover(&k).unwrap();
        assert_eq!(cancelled.phase, BrowserTakeoverPhase::Resynchronizing);
        assert_eq!(cancelled.generation, 1);
        reg.force_timeout(&k).unwrap_err();
        let timed_reg = registry();
        let k2 = key("s2");
        timed_reg
            .request_human_takeover(&k2, false, TakeoverReason::Unknown)
            .unwrap();
        timed_reg.force_timeout(&k2).unwrap();
        let after_timeout = timed_reg.cancel_human_takeover(&k2).unwrap();
        assert_eq!(after_timeout.phase, BrowserTakeoverPhase::Resynchronizing);
        assert_eq!(after_timeout.generation, 1);
    }

    #[test]
    fn browser_lost_during_takeover_blocks_and_release_reports_lost() {
        let reg = registry();
        let k = key("s1");
        reg.request_human_takeover(&k, false, TakeoverReason::Unknown)
            .unwrap();
        reg.note_browser_lost(&k);
        assert_eq!(reg.get(&k).phase, BrowserTakeoverPhase::BrowserLost);
        assert!(matches!(
            reg.begin_agent_operation(&k, AgentOpClass::Observe),
            Err(BrowserControlError::BrowserLost { .. })
        ));
        let err = reg.release_human_takeover(&k).unwrap_err();
        assert!(matches!(err, BrowserControlError::BrowserLost { .. }));
        assert!(!reg.contains(&k));
    }

    #[test]
    fn close_removes_control_state() {
        let reg = registry();
        let k = key("s1");
        reg.request_human_takeover(&k, false, TakeoverReason::Unknown)
            .unwrap();
        assert!(reg.contains(&k));
        reg.remove(&k);
        assert!(!reg.contains(&k));
        assert_eq!(reg.get(&k).phase, BrowserTakeoverPhase::AgentControlled);
        reg.remove(&k);
        assert!(!reg.contains(&k));
    }

    #[test]
    fn in_flight_permit_drop_is_exception_safe() {
        let reg = registry();
        let k = key("s1");
        let result = thread::spawn({
            let reg = Arc::clone(&reg);
            let k = k.clone();
            move || {
                let _permit = reg.begin_agent_operation(&k, AgentOpClass::Mutate).unwrap();
                assert_eq!(reg.get(&k).in_flight, 1);
                panic!("boom");
            }
        })
        .join();
        assert!(result.is_err());
        assert_eq!(reg.get(&k).in_flight, 0);
    }

    #[test]
    fn multiple_cycles_increment_generation() {
        let reg = registry();
        let k = key("s1");
        for expected in 1..=3 {
            reg.request_human_takeover(&k, false, TakeoverReason::ExplicitUserRequest)
                .unwrap();
            let released = reg.release_human_takeover(&k).unwrap();
            assert_eq!(released.generation, expected);
            let permit = reg
                .begin_agent_operation(&k, AgentOpClass::Observe)
                .unwrap();
            drop(permit);
            reg.complete_resync(&k);
            assert_eq!(reg.get(&k).phase, BrowserTakeoverPhase::AgentControlled);
        }
    }

    #[test]
    fn lost_during_takeover_requested_does_not_grant_human() {
        let reg = registry();
        let k = key("s1");
        let permit = reg
            .begin_agent_operation(&k, AgentOpClass::Mutate)
            .unwrap();
        reg.request_human_takeover(&k, false, TakeoverReason::Unknown)
            .unwrap();
        reg.note_browser_lost(&k);
        drop(permit);
        assert_eq!(reg.get(&k).phase, BrowserTakeoverPhase::BrowserLost);
        assert_eq!(reg.get(&k).in_flight, 0);
    }
}
