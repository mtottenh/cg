//! Does a proposed match time collide with another match?
//!
//! Nothing stopped a team agreeing the same evening against two opponents,
//! or two matches being agreed for the same hour on the one game server.
//! The check is a pure function over the matches around the candidate time
//! so it can be tested without a database.

use chrono::{DateTime, Duration, Utc};
use portal_core::ids::{TournamentMatchId, TournamentRegistrationId};

/// A team needs this long between the starts of two of its matches.
pub const TEAM_CLASH_WINDOW_MINUTES: i64 = 120;

/// The game server is taken for this long around a match's start.
pub const SERVER_CLASH_WINDOW_MINUTES: i64 = 60;

/// Another match of the same tournament that already has a time near the
/// candidate one and has not finished.
#[derive(Debug, Clone)]
pub struct ScheduleNeighbour {
    pub match_id: TournamentMatchId,
    pub participant1_registration_id: Option<TournamentRegistrationId>,
    pub participant2_registration_id: Option<TournamentRegistrationId>,
    pub participant1_name: Option<String>,
    pub participant2_name: Option<String>,
    pub scheduled_at: DateTime<Utc>,
}

/// What surrounds a candidate time for a match.
#[derive(Debug, Clone, Default)]
pub struct ScheduleContext {
    /// Unfinished matches of the tournament with a time within
    /// [`TEAM_CLASH_WINDOW_MINUTES`] of the candidate.
    pub neighbours: Vec<ScheduleNeighbour>,
    /// The tournament plays on the portal's game server and there is exactly
    /// one enabled server for its game, so two matches cannot overlap.
    pub single_game_server: bool,
}

fn label(n: &ScheduleNeighbour) -> String {
    format!(
        "{} vs {}",
        n.participant1_name.as_deref().unwrap_or("TBD"),
        n.participant2_name.as_deref().unwrap_or("TBD")
    )
}

/// The match a time is being chosen for.
#[derive(Debug, Clone, Copy)]
pub struct ScheduleSubject {
    pub match_id: TournamentMatchId,
    pub participants: [Option<TournamentRegistrationId>; 2],
}

/// Why `candidate` cannot be the time of `match_`, if it cannot.
///
/// A team clash (either side already plays within two hours) is reported
/// ahead of a server clash (any match within the hour on the only server).
#[must_use]
pub fn find_schedule_clash(
    match_: ScheduleSubject,
    candidate: DateTime<Utc>,
    context: &ScheduleContext,
) -> Option<String> {
    let mine = match_.participants;
    let others = || {
        context
            .neighbours
            .iter()
            .filter(|n| n.match_id != match_.match_id)
    };
    let gap = |n: &ScheduleNeighbour| (n.scheduled_at - candidate).abs();

    let team = others().find(|n| {
        gap(n) < Duration::minutes(TEAM_CLASH_WINDOW_MINUTES)
            && [
                n.participant1_registration_id,
                n.participant2_registration_id,
            ]
            .iter()
            .flatten()
            .any(|r| mine.contains(&Some(*r)))
    });
    if let Some(n) = team {
        return Some(format!(
            "One of the teams already plays {} at {} UTC; matches for the same team must start \
             at least two hours apart",
            label(n),
            n.scheduled_at.format("%a %d %b %H:%M")
        ));
    }

    if context.single_game_server {
        let server = others().find(|n| gap(n) < Duration::minutes(SERVER_CLASH_WINDOW_MINUTES));
        if let Some(n) = server {
            return Some(format!(
                "The game server is booked for {} at {} UTC; pick a time at least an hour away",
                label(n),
                n.scheduled_at.format("%a %d %b %H:%M")
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> TournamentRegistrationId {
        TournamentRegistrationId::new()
    }

    fn match_between(a: TournamentRegistrationId, b: TournamentRegistrationId) -> ScheduleSubject {
        ScheduleSubject {
            match_id: TournamentMatchId::new(),
            participants: [Some(a), Some(b)],
        }
    }

    fn neighbour(
        a: TournamentRegistrationId,
        b: TournamentRegistrationId,
        at: DateTime<Utc>,
    ) -> ScheduleNeighbour {
        ScheduleNeighbour {
            match_id: TournamentMatchId::new(),
            participant1_registration_id: Some(a),
            participant2_registration_id: Some(b),
            participant1_name: Some("Alpha".into()),
            participant2_name: Some("Bravo".into()),
            scheduled_at: at,
        }
    }

    #[test]
    fn a_free_evening_has_no_clash() {
        let m = match_between(reg(), reg());
        assert!(find_schedule_clash(m, Utc::now(), &ScheduleContext::default()).is_none());
    }

    #[test]
    fn a_team_cannot_start_two_matches_within_two_hours() {
        let (a, b, c) = (reg(), reg(), reg());
        let m = match_between(a, b);
        let at = Utc::now() + Duration::days(1);
        let context = ScheduleContext {
            neighbours: vec![neighbour(c, a, at + Duration::minutes(90))],
            single_game_server: false,
        };
        let clash = find_schedule_clash(m, at, &context).expect("team clash");
        assert!(clash.contains("Alpha vs Bravo"), "{clash}");

        // Two hours apart is fine.
        let context = ScheduleContext {
            neighbours: vec![neighbour(c, a, at + Duration::minutes(120))],
            single_game_server: false,
        };
        assert!(find_schedule_clash(m, at, &context).is_none());
    }

    #[test]
    fn other_teams_only_clash_when_they_share_the_one_server() {
        let m = match_between(reg(), reg());
        let at = Utc::now() + Duration::days(1);
        let other = neighbour(reg(), reg(), at - Duration::minutes(30));

        let shared = ScheduleContext {
            neighbours: vec![other.clone()],
            single_game_server: true,
        };
        let clash = find_schedule_clash(m, at, &shared).expect("server clash");
        assert!(clash.contains("game server is booked"), "{clash}");

        let separate = ScheduleContext {
            neighbours: vec![other],
            single_game_server: false,
        };
        assert!(find_schedule_clash(m, at, &separate).is_none());

        // An hour away is clear even on one server.
        let later = ScheduleContext {
            neighbours: vec![neighbour(reg(), reg(), at + Duration::minutes(60))],
            single_game_server: true,
        };
        assert!(find_schedule_clash(m, at, &later).is_none());
    }

    #[test]
    fn a_match_does_not_clash_with_itself() {
        let (a, b) = (reg(), reg());
        let m = match_between(a, b);
        let at = Utc::now() + Duration::days(1);
        let mut own = neighbour(a, b, at);
        own.match_id = m.match_id;
        let context = ScheduleContext {
            neighbours: vec![own],
            single_game_server: true,
        };
        assert!(find_schedule_clash(m, at, &context).is_none());
    }
}
