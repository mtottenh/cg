//! Notifications: short messages to one user about something in a match that
//! asks for their attention or that they will want to know.

use chrono::{DateTime, Utc};
use portal_core::{
    NotificationId, TournamentId, TournamentMatchId, TournamentRegistrationId, UserId,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// What a notification is about. The wire value is the snake_case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    ProposalReceived,
    ProposalAccepted,
    ProposalRejected,
    ProposalExpired,
    MatchTimeSet,
    CheckinOpen,
    CheckinReminder,
    VetoStarted,
    ServerReady,
    ServerQueued,
    ServerAssignedFromQueue,
    ResultToConfirm,
    ResultFinal,
    DisputeRuled,
    MatchReset,
}

impl NotificationKind {
    /// Every kind, in a stable order.
    pub const ALL: [Self; 15] = [
        Self::ProposalReceived,
        Self::ProposalAccepted,
        Self::ProposalRejected,
        Self::ProposalExpired,
        Self::MatchTimeSet,
        Self::CheckinOpen,
        Self::CheckinReminder,
        Self::VetoStarted,
        Self::ServerReady,
        Self::ServerQueued,
        Self::ServerAssignedFromQueue,
        Self::ResultToConfirm,
        Self::ResultFinal,
        Self::DisputeRuled,
        Self::MatchReset,
    ];

    /// The stored and serialised name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProposalReceived => "proposal_received",
            Self::ProposalAccepted => "proposal_accepted",
            Self::ProposalRejected => "proposal_rejected",
            Self::ProposalExpired => "proposal_expired",
            Self::MatchTimeSet => "match_time_set",
            Self::CheckinOpen => "checkin_open",
            Self::CheckinReminder => "checkin_reminder",
            Self::VetoStarted => "veto_started",
            Self::ServerReady => "server_ready",
            Self::ServerQueued => "server_queued",
            Self::ServerAssignedFromQueue => "server_assigned_from_queue",
            Self::ResultToConfirm => "result_to_confirm",
            Self::ResultFinal => "result_final",
            Self::DisputeRuled => "dispute_ruled",
            Self::MatchReset => "match_reset",
        }
    }

    /// Whether the event is also posted to the tournament's Discord channel
    /// (when one is configured): the things a whole team wants to see.
    #[must_use]
    pub const fn goes_to_discord(self) -> bool {
        matches!(
            self,
            Self::ProposalReceived
                | Self::ProposalAccepted
                | Self::MatchTimeSet
                | Self::CheckinOpen
                | Self::CheckinReminder
                | Self::ServerReady
                | Self::ServerAssignedFromQueue
                | Self::ResultFinal
        )
    }
}

impl fmt::Display for NotificationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for NotificationKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| format!("unknown notification kind: {s}"))
    }
}

/// A stored notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: NotificationId,
    pub user_id: UserId,
    /// Kept as text so a row written by a newer build still lists.
    pub kind: String,
    pub title: String,
    pub body: String,
    /// SPA path to open.
    pub link: Option<String>,
    pub tournament_id: Option<TournamentId>,
    pub match_id: Option<TournamentMatchId>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

/// One notification to write for each recipient.
#[derive(Debug, Clone)]
pub struct NewNotification {
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    pub tournament_id: Option<TournamentId>,
    pub match_id: Option<TournamentMatchId>,
    /// A recipient gets at most one notification per key.
    pub dedupe_key: Option<String>,
}

/// Who among a registration's people a notification is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Everyone on the active roster (or the registered player).
    Roster,
    /// Those who can act for the registration: the registered player, or the
    /// team's captain, owner and active delegates.
    Schedulers,
}

/// What a notification needs to know about the match it is about.
#[derive(Debug, Clone)]
pub struct MatchNotificationContext {
    pub match_id: TournamentMatchId,
    pub tournament_id: TournamentId,
    pub tournament_name: String,
    pub tournament_slug: String,
    pub round: i32,
    pub participant1_registration_id: Option<TournamentRegistrationId>,
    pub participant1_name: Option<String>,
    pub participant2_registration_id: Option<TournamentRegistrationId>,
    pub participant2_name: Option<String>,
    pub participant1_score: Option<i32>,
    pub participant2_score: Option<i32>,
    pub winner_registration_id: Option<TournamentRegistrationId>,
    /// The match status as stored (`completed`, `forfeit`, …).
    pub status: String,
}

impl MatchNotificationContext {
    /// "A vs B".
    #[must_use]
    pub fn fixture(&self) -> String {
        format!(
            "{} vs {}",
            self.participant1_name.as_deref().unwrap_or("TBD"),
            self.participant2_name.as_deref().unwrap_or("TBD")
        )
    }

    /// The name of a participant, or "a team".
    #[must_use]
    pub fn name_of(&self, registration_id: TournamentRegistrationId) -> String {
        if Some(registration_id) == self.participant1_registration_id {
            self.participant1_name
                .clone()
                .unwrap_or_else(|| "A team".to_string())
        } else if Some(registration_id) == self.participant2_registration_id {
            self.participant2_name
                .clone()
                .unwrap_or_else(|| "A team".to_string())
        } else {
            "A team".to_string()
        }
    }

    /// The other participant of the match.
    #[must_use]
    pub fn opponent_of(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Option<TournamentRegistrationId> {
        if Some(registration_id) == self.participant1_registration_id {
            self.participant2_registration_id
        } else if Some(registration_id) == self.participant2_registration_id {
            self.participant1_registration_id
        } else {
            None
        }
    }

    /// Both participants that are set.
    #[must_use]
    pub fn participants(&self) -> Vec<TournamentRegistrationId> {
        [
            self.participant1_registration_id,
            self.participant2_registration_id,
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// SPA path of the match page.
    #[must_use]
    pub fn link(&self) -> String {
        format!(
            "/tournaments/{}/matches/{}",
            self.tournament_slug, self.match_id
        )
    }
}

/// Admin-only delivery settings of a tournament.
#[derive(Debug, Clone)]
pub struct TournamentNotificationSettings {
    pub tournament_id: TournamentId,
    pub discord_webhook_url: Option<String>,
    pub discord_enabled: bool,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<UserId>,
}

/// A registration whose side has not checked in and whose window closes soon.
#[derive(Debug, Clone)]
pub struct CheckInReminderCandidate {
    pub match_id: TournamentMatchId,
    pub registration_id: TournamentRegistrationId,
    pub deadline: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::NotificationKind;

    #[test]
    fn kind_names_round_trip() {
        for kind in NotificationKind::ALL {
            assert_eq!(kind.as_str().parse::<NotificationKind>(), Ok(kind));
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::Value::String(kind.as_str().to_string())
            );
        }
    }
}
