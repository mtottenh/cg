//! Repository-level tests for league team entities.
//!
//! These tests verify the database layer directly without going through the service or API layers.
//! They focus on:
//! - CRUD operations work correctly
//! - Query methods return expected results
//! - Database constraints are enforced
//! - Edge cases are handled properly

use chrono::Utc;
use portal_core::types::{
    LeagueTeamInvitationStatus, LeagueTeamInvitationType, LeagueTeamMemberStatus, LeagueTeamRole,
    LeagueTeamSeasonStatus, LeagueTeamStatus, RosterLockStatus, SeasonStatus,
};
use portal_core::{LeagueId, LeagueSeasonId, LeagueTeamId, LeagueTeamSeasonId, PlayerId, UserId};
use portal_db::adapters::{
    PgLeagueSeasonParticipantRepository, PgLeagueSeasonRepository, PgLeagueTeamInvitationRepository,
    PgLeagueTeamMemberRepository, PgLeagueTeamRepository, PgLeagueTeamSeasonRepository,
};
use portal_db::DbPool;
use portal_domain::repositories::league_team::{
    AddLeagueTeamMember, CreateLeagueSeason, CreateLeagueTeam, CreateLeagueTeamInvitation,
    CreateLeagueTeamSeason, LeagueSeasonParticipantRepository, LeagueSeasonRepository,
    LeagueTeamInvitationRepository, LeagueTeamMemberRepository, LeagueTeamRepository,
    LeagueTeamSeasonRepository, RegisterLeagueSeasonParticipant, UpdateLeagueSeason,
    UpdateLeagueTeam,
};
use portal_test::database::TestDb;
use portal_test::prelude::*;

// =============================================================================
// TEST HELPERS
// =============================================================================

async fn create_test_league(pool: &DbPool) -> LeagueId {
    let unique = uuid::Uuid::new_v4();
    let league = LeagueBuilder::new()
        .name(&format!("Test League {}", &unique.to_string()[..8]))
        .slug(&format!("test-league-{}", &unique.to_string()[..12]))
        .build_persisted(pool)
        .await;
    LeagueId::from_uuid(league.id)
}

async fn create_test_user(pool: &DbPool, _suffix: &str) -> UserId {
    // Use random UUID for uniqueness (UUID v4)
    let unique = uuid::Uuid::new_v4();
    // Keep username short (max 32 chars)
    let user = UserBuilder::new()
        .username(&format!("u{}", &unique.to_string()[..12]))
        .email(&format!("{}@t.com", &unique.to_string()[..12]))
        .build_persisted(pool)
        .await;
    UserId::from_uuid(user.id)
}

async fn create_test_player(pool: &DbPool, _suffix: &str) -> (PlayerId, UserId) {
    // Use random UUID for uniqueness (UUID v4)
    let unique = uuid::Uuid::new_v4();
    // Keep username short (max 32 chars)
    let user = UserBuilder::new()
        .username(&format!("p{}", &unique.to_string()[..12]))
        .email(&format!("p{}@t.com", &unique.to_string()[..12]))
        .build_persisted(pool)
        .await;
    // UserBuilder::build_persisted already creates a player with the same ID as the user
    (PlayerId::from_uuid(user.id), UserId::from_uuid(user.id))
}

// =============================================================================
// LEAGUE SEASON REPOSITORY TESTS
// =============================================================================

mod league_season_repository {
    use super::*;

    #[tokio::test]
    async fn test_create_season() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "createseason").await;

        // Note: The league trigger auto-creates "Season 1" with slug "season-1"
        // so we create "Season 2" to test the create functionality
        let cmd = CreateLeagueSeason {
            league_id,
            name: "Season 2".to_string(),
            slug: "season-2".to_string(),
            description: Some("Second season".to_string()),
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: Some(5),
            team_size_max: Some(7),
            max_substitutes: Some(2),
            max_teams: Some(16),
            created_by: user_id,
        };

        let season = repo.create(cmd).await.expect("Failed to create season");

        assert_eq!(season.name, "Season 2");
        assert_eq!(season.slug, "season-2");
        assert_eq!(season.status, SeasonStatus::Draft);
        assert_eq!(season.roster_lock_status, RosterLockStatus::Open);
    }

    #[tokio::test]
    async fn test_find_season_by_id() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "findseason").await;

        let cmd = CreateLeagueSeason {
            league_id,
            name: "Find Season".to_string(),
            slug: "find-season".to_string(),
            description: None,
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: None,
            team_size_max: None,
            max_substitutes: None,
            max_teams: None,
            created_by: user_id,
        };

        let created = repo.create(cmd).await.unwrap();
        let found = repo.find_by_id(created.id).await.unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn test_find_season_by_id_not_found() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());

        let result = repo.find_by_id(LeagueSeasonId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_find_season_by_slug() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "slugseason").await;

        let cmd = CreateLeagueSeason {
            league_id,
            name: "Slug Season".to_string(),
            slug: "slug-season-test".to_string(),
            description: None,
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: None,
            team_size_max: None,
            max_substitutes: None,
            max_teams: None,
            created_by: user_id,
        };

        repo.create(cmd).await.unwrap();

        let found = repo.find_by_slug(league_id, "slug-season-test").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().slug, "slug-season-test");
    }

    #[tokio::test]
    async fn test_update_season() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "updateseason").await;

        let cmd = CreateLeagueSeason {
            league_id,
            name: "Original Name".to_string(),
            slug: "original-slug".to_string(),
            description: None,
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: None,
            team_size_max: None,
            max_substitutes: None,
            max_teams: None,
            created_by: user_id,
        };

        let created = repo.create(cmd).await.unwrap();

        let update = UpdateLeagueSeason {
            name: Some("Updated Name".to_string()),
            slug: None,
            description: Some("New description".to_string()),
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: None,
            team_size_max: None,
            max_substitutes: None,
            max_teams: None,
            status: None,
            settings: None,
        };

        let updated = repo.update(created.id, update).await.unwrap();

        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.slug, "original-slug"); // Unchanged
        assert_eq!(updated.description.unwrap(), "New description");
    }

    #[tokio::test]
    async fn test_update_season_status() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "statusseason").await;

        let cmd = CreateLeagueSeason {
            league_id,
            name: "Status Season".to_string(),
            slug: "status-season".to_string(),
            description: None,
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: None,
            team_size_max: None,
            max_substitutes: None,
            max_teams: None,
            created_by: user_id,
        };

        let created = repo.create(cmd).await.unwrap();
        assert_eq!(created.status, SeasonStatus::Draft);

        let updated = repo.update_status(created.id, SeasonStatus::Registration).await.unwrap();
        assert_eq!(updated.status, SeasonStatus::Registration);

        let updated = repo.update_status(created.id, SeasonStatus::Active).await.unwrap();
        assert_eq!(updated.status, SeasonStatus::Active);
    }

    #[tokio::test]
    async fn test_update_roster_lock() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "rosterlock").await;

        let cmd = CreateLeagueSeason {
            league_id,
            name: "Roster Lock Season".to_string(),
            slug: "roster-lock-season".to_string(),
            description: None,
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: None,
            team_size_max: None,
            max_substitutes: None,
            max_teams: None,
            created_by: user_id,
        };

        let created = repo.create(cmd).await.unwrap();
        assert_eq!(created.roster_lock_status, RosterLockStatus::Open);

        let updated = repo
            .update_roster_lock(created.id, RosterLockStatus::SoftLock, Some(user_id))
            .await
            .unwrap();
        assert_eq!(updated.roster_lock_status, RosterLockStatus::SoftLock);

        let updated = repo
            .update_roster_lock(created.id, RosterLockStatus::HardLock, Some(user_id))
            .await
            .unwrap();
        assert_eq!(updated.roster_lock_status, RosterLockStatus::HardLock);
        assert!(updated.roster_locked_at.is_some());
    }

    #[tokio::test]
    async fn test_list_seasons_by_league() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "listseasons").await;

        // League creation auto-creates "Season 1" with slug "season-1"
        // So we add 2 more seasons (total 3)
        for i in 2..=3 {
            let cmd = CreateLeagueSeason {
                league_id,
                name: format!("Season {i}"),
                slug: format!("season-{i}"),
                description: None,
                registration_start: None,
                registration_end: None,
                season_start: None,
                season_end: None,
                team_size_min: None,
                team_size_max: None,
                max_substitutes: None,
                max_teams: None,
                created_by: user_id,
            };
            repo.create(cmd).await.unwrap();
        }

        let seasons = repo.list_by_league(league_id).await.unwrap();
        assert_eq!(seasons.len(), 3); // 1 auto-created + 2 manual
    }

    #[tokio::test]
    async fn test_slug_exists() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "slugexists").await;

        let cmd = CreateLeagueSeason {
            league_id,
            name: "Unique Season".to_string(),
            slug: "unique-slug".to_string(),
            description: None,
            registration_start: None,
            registration_end: None,
            season_start: None,
            season_end: None,
            team_size_min: None,
            team_size_max: None,
            max_substitutes: None,
            max_teams: None,
            created_by: user_id,
        };

        repo.create(cmd).await.unwrap();

        assert!(repo.slug_exists(league_id, "unique-slug").await.unwrap());
        assert!(!repo.slug_exists(league_id, "nonexistent-slug").await.unwrap());
    }

    #[tokio::test]
    async fn test_count_teams() {
        let db = TestDb::new().await;
        let season_repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let team_repo = PgLeagueTeamRepository::new(db.pool.clone());
        let team_season_repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "countteams").await;

        let season = season_repo
            .create(CreateLeagueSeason {
                league_id,
                name: "Count Teams Season".to_string(),
                slug: "count-teams-season".to_string(),
                description: None,
                registration_start: None,
                registration_end: None,
                season_start: None,
                season_end: None,
                team_size_min: None,
                team_size_max: None,
                max_substitutes: None,
                max_teams: None,
                created_by: user_id,
            })
            .await
            .unwrap();

        // Initially no teams
        assert_eq!(season_repo.count_teams(season.id).await.unwrap(), 0);

        // Create teams and register them
        for i in 1..=3 {
            let (player_id, _) = create_test_player(&db.pool, &format!("countteam{i}")).await;

            let team = team_repo
                .create(CreateLeagueTeam {
                    league_id,
                    name: format!("Team {i}"),
                    tag: format!("T{i}"),
                    description: None,
                    logo_url: None,
                    primary_color: None,
                    secondary_color: None,
                    owner_player_id: player_id,
                })
                .await
                .unwrap();

            team_season_repo
                .create(CreateLeagueTeamSeason {
                    team_id: team.id,
                    season_id: season.id,
                })
                .await
                .unwrap();
        }

        assert_eq!(season_repo.count_teams(season.id).await.unwrap(), 3);
    }
}

// =============================================================================
// LEAGUE TEAM REPOSITORY TESTS
// =============================================================================

mod league_team_repository {
    use super::*;

    #[tokio::test]
    async fn test_create_team() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "createteam").await;

        let cmd = CreateLeagueTeam {
            league_id,
            name: "Test Team".to_string(),
            tag: "TST".to_string(),
            description: Some("Test description".to_string()),
            logo_url: None,
            primary_color: Some("#FF0000".to_string()),
            secondary_color: Some("#0000FF".to_string()),
            owner_player_id: player_id,
        };

        let team = repo.create(cmd).await.expect("Failed to create team");

        assert_eq!(team.name, "Test Team");
        assert_eq!(team.tag, "TST");
        assert_eq!(team.owner_player_id, player_id);
        assert_eq!(team.status, LeagueTeamStatus::Active);
    }

    #[tokio::test]
    async fn test_find_team_by_id() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "findteam").await;

        let team = repo
            .create(CreateLeagueTeam {
                league_id,
                name: "Find Team".to_string(),
                tag: "FND".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player_id,
            })
            .await
            .unwrap();

        let found = repo.find_by_id(team.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, team.id);
    }

    #[tokio::test]
    async fn test_find_team_by_name() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "nameteam").await;

        repo.create(CreateLeagueTeam {
            league_id,
            name: "Named Team".to_string(),
            tag: "NMD".to_string(),
            description: None,
            logo_url: None,
            primary_color: None,
            secondary_color: None,
            owner_player_id: player_id,
        })
        .await
        .unwrap();

        let found = repo.find_by_name(league_id, "Named Team").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Named Team");
    }

    #[tokio::test]
    async fn test_find_team_by_name_case_insensitive() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "caseteam").await;

        repo.create(CreateLeagueTeam {
            league_id,
            name: "Case Team".to_string(),
            tag: "CSE".to_string(),
            description: None,
            logo_url: None,
            primary_color: None,
            secondary_color: None,
            owner_player_id: player_id,
        })
        .await
        .unwrap();

        // Should find with different cases
        let found = repo.find_by_name(league_id, "case team").await.unwrap();
        assert!(found.is_some());

        let found = repo.find_by_name(league_id, "CASE TEAM").await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_find_team_by_tag() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "tagteam").await;

        repo.create(CreateLeagueTeam {
            league_id,
            name: "Tagged Team".to_string(),
            tag: "TAG".to_string(),
            description: None,
            logo_url: None,
            primary_color: None,
            secondary_color: None,
            owner_player_id: player_id,
        })
        .await
        .unwrap();

        let found = repo.find_by_tag(league_id, "TAG").await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_update_team() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "updateteam").await;

        let team = repo
            .create(CreateLeagueTeam {
                league_id,
                name: "Original Team".to_string(),
                tag: "ORG".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player_id,
            })
            .await
            .unwrap();

        let updated = repo
            .update(
                team.id,
                UpdateLeagueTeam {
                    name: Some("Updated Team".to_string()),
                    tag: None,
                    description: Some("New description".to_string()),
                    logo_url: None,
                    banner_url: None,
                    primary_color: Some("#00FF00".to_string()),
                    secondary_color: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.name, "Updated Team");
        assert_eq!(updated.tag, "ORG"); // Unchanged
        assert_eq!(updated.description.unwrap(), "New description");
        assert_eq!(updated.primary_color.unwrap(), "#00FF00");
    }

    #[tokio::test]
    async fn test_update_team_status() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "statusteam").await;

        let team = repo
            .create(CreateLeagueTeam {
                league_id,
                name: "Status Team".to_string(),
                tag: "STS".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player_id,
            })
            .await
            .unwrap();

        assert_eq!(team.status, LeagueTeamStatus::Active);

        let updated = repo
            .update_status(team.id, LeagueTeamStatus::Disbanded)
            .await
            .unwrap();

        assert_eq!(updated.status, LeagueTeamStatus::Disbanded);
        assert!(updated.disbanded_at.is_some());
    }

    #[tokio::test]
    async fn test_transfer_ownership() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player1_id, _) = create_test_player(&db.pool, "transfer1").await;
        let (player2_id, _) = create_test_player(&db.pool, "transfer2").await;

        let team = repo
            .create(CreateLeagueTeam {
                league_id,
                name: "Transfer Team".to_string(),
                tag: "TRN".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player1_id,
            })
            .await
            .unwrap();

        assert_eq!(team.owner_player_id, player1_id);

        let updated = repo.transfer_ownership(team.id, player2_id).await.unwrap();
        assert_eq!(updated.owner_player_id, player2_id);
    }

    #[tokio::test]
    async fn test_list_teams_by_league() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;

        for i in 1..=5 {
            let (player_id, _) = create_test_player(&db.pool, &format!("listteam{i}")).await;
            repo.create(CreateLeagueTeam {
                league_id,
                name: format!("Team {i}"),
                tag: format!("T{i:02}"),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player_id,
            })
            .await
            .unwrap();
        }

        let (teams, count) = repo.list_by_league(league_id, None, None, 10, 0).await.unwrap();
        assert_eq!(teams.len(), 5);
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_list_teams_with_pagination() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;

        for i in 1..=10 {
            let (player_id, _) = create_test_player(&db.pool, &format!("pageteam{i}")).await;
            repo.create(CreateLeagueTeam {
                league_id,
                name: format!("Team {i}"),
                tag: format!("T{i:02}"),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player_id,
            })
            .await
            .unwrap();
        }

        let (teams, count) = repo.list_by_league(league_id, None, None, 3, 0).await.unwrap();
        assert_eq!(teams.len(), 3);
        assert_eq!(count, 10);

        let (teams, _) = repo.list_by_league(league_id, None, None, 3, 3).await.unwrap();
        assert_eq!(teams.len(), 3);
    }

    #[tokio::test]
    async fn test_list_teams_with_status_filter() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;

        // Create active teams
        for i in 1..=3 {
            let (player_id, _) = create_test_player(&db.pool, &format!("activeteam{i}")).await;
            repo.create(CreateLeagueTeam {
                league_id,
                name: format!("Active Team {i}"),
                tag: format!("A{i}"),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player_id,
            })
            .await
            .unwrap();
        }

        // Create and disband a team
        let (player_id, _) = create_test_player(&db.pool, "disbandedteam").await;
        let team = repo
            .create(CreateLeagueTeam {
                league_id,
                name: "Disbanded Team".to_string(),
                tag: "DIS".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player_id,
            })
            .await
            .unwrap();
        repo.update_status(team.id, LeagueTeamStatus::Disbanded).await.unwrap();

        let (active_teams, _) = repo
            .list_by_league(league_id, Some(LeagueTeamStatus::Active), None, 10, 0)
            .await
            .unwrap();
        assert_eq!(active_teams.len(), 3);

        let (disbanded_teams, _) = repo
            .list_by_league(league_id, Some(LeagueTeamStatus::Disbanded), None, 10, 0)
            .await
            .unwrap();
        assert_eq!(disbanded_teams.len(), 1);
    }

    #[tokio::test]
    async fn test_name_exists() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "existsteam").await;

        repo.create(CreateLeagueTeam {
            league_id,
            name: "Existing Team".to_string(),
            tag: "EXT".to_string(),
            description: None,
            logo_url: None,
            primary_color: None,
            secondary_color: None,
            owner_player_id: player_id,
        })
        .await
        .unwrap();

        assert!(repo.name_exists(league_id, "Existing Team").await.unwrap());
        assert!(repo.name_exists(league_id, "existing team").await.unwrap()); // Case insensitive
        assert!(!repo.name_exists(league_id, "Nonexistent Team").await.unwrap());
    }

    #[tokio::test]
    async fn test_tag_exists() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let league_id = create_test_league(&db.pool).await;
        let (player_id, _) = create_test_player(&db.pool, "tagexists").await;

        repo.create(CreateLeagueTeam {
            league_id,
            name: "Tag Team".to_string(),
            tag: "ABC".to_string(),
            description: None,
            logo_url: None,
            primary_color: None,
            secondary_color: None,
            owner_player_id: player_id,
        })
        .await
        .unwrap();

        assert!(repo.tag_exists(league_id, "ABC").await.unwrap());
        assert!(repo.tag_exists(league_id, "abc").await.unwrap()); // Case insensitive
        assert!(!repo.tag_exists(league_id, "XYZ").await.unwrap());
    }
}

// =============================================================================
// LEAGUE TEAM SEASON REPOSITORY TESTS
// =============================================================================

mod league_team_season_repository {
    use super::*;

    async fn create_season(db: &TestDb) -> (LeagueId, LeagueSeasonId) {
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "seasonsetup").await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());

        let season = repo
            .create(CreateLeagueSeason {
                league_id,
                name: "Test Season".to_string(),
                slug: format!("test-season-{}", uuid::Uuid::new_v4()),
                description: None,
                registration_start: None,
                registration_end: None,
                season_start: None,
                season_end: None,
                team_size_min: Some(5),
                team_size_max: Some(7),
                max_substitutes: Some(2),
                max_teams: None,
                created_by: user_id,
            })
            .await
            .unwrap();

        (league_id, season.id)
    }

    async fn create_team(db: &TestDb, league_id: LeagueId, suffix: &str) -> LeagueTeamId {
        let repo = PgLeagueTeamRepository::new(db.pool.clone());
        let (player_id, _) = create_test_player(&db.pool, suffix).await;
        // Use random UUID for unique tag (max 5 chars)
        let unique = uuid::Uuid::new_v4();
        let short_tag = &unique.to_string()[..5];

        let team = repo
            .create(CreateLeagueTeam {
                league_id,
                name: format!("Team {suffix}"),
                tag: short_tag.to_uppercase(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id: player_id,
            })
            .await
            .unwrap();

        team.id
    }

    #[tokio::test]
    async fn test_create_team_season() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());
        let (league_id, season_id) = create_season(&db).await;
        let team_id = create_team(&db, league_id, "createts").await;

        let team_season = repo
            .create(CreateLeagueTeamSeason { team_id, season_id })
            .await
            .unwrap();

        assert_eq!(team_season.team_id, team_id);
        assert_eq!(team_season.season_id, season_id);
        assert_eq!(team_season.status, LeagueTeamSeasonStatus::Forming);
    }

    #[tokio::test]
    async fn test_find_team_season_by_id() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());
        let (league_id, season_id) = create_season(&db).await;
        let team_id = create_team(&db, league_id, "findts").await;

        let created = repo
            .create(CreateLeagueTeamSeason { team_id, season_id })
            .await
            .unwrap();

        let found = repo.find_by_id(created.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn test_find_by_team_and_season() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());
        let (league_id, season_id) = create_season(&db).await;
        let team_id = create_team(&db, league_id, "findbyteamseason").await;

        repo.create(CreateLeagueTeamSeason { team_id, season_id })
            .await
            .unwrap();

        let found = repo.find_by_team_and_season(team_id, season_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.as_ref().unwrap().team_id, team_id);
        assert_eq!(found.as_ref().unwrap().season_id, season_id);
    }

    #[tokio::test]
    async fn test_update_team_season_status() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());
        let (league_id, season_id) = create_season(&db).await;
        let team_id = create_team(&db, league_id, "updatetsstatus").await;

        let created = repo
            .create(CreateLeagueTeamSeason { team_id, season_id })
            .await
            .unwrap();

        assert_eq!(created.status, LeagueTeamSeasonStatus::Forming);

        let updated = repo
            .update_status(created.id, LeagueTeamSeasonStatus::Registered)
            .await
            .unwrap();

        assert_eq!(updated.status, LeagueTeamSeasonStatus::Registered);
        assert!(updated.registered_at.is_some());
    }

    #[tokio::test]
    async fn test_list_team_seasons_by_season() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());
        let (league_id, season_id) = create_season(&db).await;

        for i in 1..=5 {
            let team_id = create_team(&db, league_id, &format!("listts{i}")).await;
            repo.create(CreateLeagueTeamSeason { team_id, season_id })
                .await
                .unwrap();
        }

        let (team_seasons, count) = repo.list_by_season(season_id, None, None, 10, 0).await.unwrap();
        assert_eq!(team_seasons.len(), 5);
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_is_registered() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());
        let (league_id, season_id) = create_season(&db).await;
        let team_id = create_team(&db, league_id, "isregistered").await;

        assert!(!repo.is_registered(team_id, season_id).await.unwrap());

        repo.create(CreateLeagueTeamSeason { team_id, season_id })
            .await
            .unwrap();

        assert!(repo.is_registered(team_id, season_id).await.unwrap());
    }
}

// =============================================================================
// LEAGUE TEAM MEMBER REPOSITORY TESTS
// =============================================================================

mod league_team_member_repository {
    use super::*;

    async fn setup_team_season(db: &TestDb) -> (LeagueTeamSeasonId, LeagueSeasonId, PlayerId) {
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "membersetup").await;
        let (owner_player_id, _) = create_test_player(&db.pool, "memberowner").await;

        let season_repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let team_repo = PgLeagueTeamRepository::new(db.pool.clone());
        let team_season_repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());

        let season = season_repo
            .create(CreateLeagueSeason {
                league_id,
                name: "Member Season".to_string(),
                slug: format!("member-season-{}", uuid::Uuid::new_v4()),
                description: None,
                registration_start: None,
                registration_end: None,
                season_start: None,
                season_end: None,
                team_size_min: Some(5),
                team_size_max: Some(7),
                max_substitutes: Some(2),
                max_teams: None,
                created_by: user_id,
            })
            .await
            .unwrap();

        let team = team_repo
            .create(CreateLeagueTeam {
                league_id,
                name: "Member Team".to_string(),
                tag: "MBR".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id,
            })
            .await
            .unwrap();

        let team_season = team_season_repo
            .create(CreateLeagueTeamSeason {
                team_id: team.id,
                season_id: season.id,
            })
            .await
            .unwrap();

        (team_season.id, season.id, owner_player_id)
    }

    #[tokio::test]
    async fn test_add_member() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;
        let (player_id, user_id) = create_test_player(&db.pool, "addmember").await;

        let member = repo
            .add_member(AddLeagueTeamMember {
                team_season_id,
                player_id,
                role: LeagueTeamRole::Player,
                position: Some("Mid".to_string()),
                jersey_number: Some(7),
                added_by: Some(user_id),
            })
            .await
            .unwrap();

        assert_eq!(member.player_id, player_id);
        assert_eq!(member.role, LeagueTeamRole::Player);
        assert_eq!(member.position.unwrap(), "Mid");
        assert_eq!(member.jersey_number.unwrap(), 7);
        assert_eq!(member.status, LeagueTeamMemberStatus::Active);
    }

    #[tokio::test]
    async fn test_find_member() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "findmember").await;

        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id,
            role: LeagueTeamRole::Captain,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        let found = repo.find_member(team_season_id, player_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().player_id, player_id);
    }

    #[tokio::test]
    async fn test_update_role() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "updaterole").await;

        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id,
            role: LeagueTeamRole::Player,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        let updated = repo
            .update_role(team_season_id, player_id, LeagueTeamRole::Captain)
            .await
            .unwrap();

        assert_eq!(updated.role, LeagueTeamRole::Captain);
    }

    #[tokio::test]
    async fn test_update_status() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "updatestatus").await;

        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id,
            role: LeagueTeamRole::Player,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        let updated = repo
            .update_status(team_season_id, player_id, LeagueTeamMemberStatus::Inactive)
            .await
            .unwrap();

        assert_eq!(updated.status, LeagueTeamMemberStatus::Inactive);
    }

    #[tokio::test]
    async fn test_remove_member() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "removemember").await;

        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id,
            role: LeagueTeamRole::Player,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        repo.remove_member(team_season_id, player_id).await.unwrap();

        // Member should no longer be found as active
        let found = repo.find_member(team_season_id, player_id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_list_members() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;

        for i in 1..=5 {
            let (player_id, _) = create_test_player(&db.pool, &format!("listmember{i}")).await;
            repo.add_member(AddLeagueTeamMember {
                team_season_id,
                player_id,
                role: if i == 1 {
                    LeagueTeamRole::Captain
                } else {
                    LeagueTeamRole::Player
                },
                position: None,
                jersey_number: None,
                added_by: None,
            })
            .await
            .unwrap();
        }

        let members = repo.list_members(team_season_id).await.unwrap();
        assert_eq!(members.len(), 5);
    }

    #[tokio::test]
    async fn test_count_by_role() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;

        // Add 2 captains
        for i in 1..=2 {
            let (player_id, _) = create_test_player(&db.pool, &format!("captain{i}")).await;
            repo.add_member(AddLeagueTeamMember {
                team_season_id,
                player_id,
                role: LeagueTeamRole::Captain,
                position: None,
                jersey_number: None,
                added_by: None,
            })
            .await
            .unwrap();
        }

        // Add 3 players
        for i in 1..=3 {
            let (player_id, _) = create_test_player(&db.pool, &format!("player{i}")).await;
            repo.add_member(AddLeagueTeamMember {
                team_season_id,
                player_id,
                role: LeagueTeamRole::Player,
                position: None,
                jersey_number: None,
                added_by: None,
            })
            .await
            .unwrap();
        }

        // Add 1 substitute
        let (sub_player_id, _) = create_test_player(&db.pool, "substitute").await;
        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id: sub_player_id,
            role: LeagueTeamRole::Substitute,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        assert_eq!(repo.count_captains(team_season_id).await.unwrap(), 2);
        assert_eq!(repo.count_by_role(team_season_id, LeagueTeamRole::Player).await.unwrap(), 3);
        assert_eq!(repo.count_substitutes(team_season_id).await.unwrap(), 1);
        assert_eq!(repo.count_primary_members(team_season_id).await.unwrap(), 5); // captains + players
        assert_eq!(repo.count_active_members(team_season_id).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn test_is_member() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "ismember").await;
        let (other_player_id, _) = create_test_player(&db.pool, "notmember").await;

        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id,
            role: LeagueTeamRole::Player,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        assert!(repo.is_member(team_season_id, player_id).await.unwrap());
        assert!(!repo.is_member(team_season_id, other_player_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_is_captain() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_team_season(&db).await;
        let (captain_id, _) = create_test_player(&db.pool, "captaintest").await;
        let (player_id, _) = create_test_player(&db.pool, "playertest").await;

        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id: captain_id,
            role: LeagueTeamRole::Captain,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id,
            role: LeagueTeamRole::Player,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        assert!(repo.is_captain(team_season_id, captain_id).await.unwrap());
        assert!(!repo.is_captain(team_season_id, player_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_find_primary_team_in_season() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamMemberRepository::new(db.pool.clone());
        let (team_season_id, season_id, _) = setup_team_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "primaryteam").await;

        // Initially not in any team
        let result = repo.find_primary_team_in_season(season_id, player_id).await.unwrap();
        assert!(result.is_none());

        // Add as player (primary role)
        repo.add_member(AddLeagueTeamMember {
            team_season_id,
            player_id,
            role: LeagueTeamRole::Player,
            position: None,
            jersey_number: None,
            added_by: None,
        })
        .await
        .unwrap();

        let result = repo.find_primary_team_in_season(season_id, player_id).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), team_season_id);
    }
}

// =============================================================================
// LEAGUE TEAM INVITATION REPOSITORY TESTS
// =============================================================================

mod league_team_invitation_repository {
    use super::*;

    async fn setup_invitation_context(db: &TestDb) -> (LeagueTeamSeasonId, PlayerId, UserId) {
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "invitesetup").await;
        let (owner_player_id, _) = create_test_player(&db.pool, "inviteowner").await;

        let season_repo = PgLeagueSeasonRepository::new(db.pool.clone());
        let team_repo = PgLeagueTeamRepository::new(db.pool.clone());
        let team_season_repo = PgLeagueTeamSeasonRepository::new(db.pool.clone());

        let season = season_repo
            .create(CreateLeagueSeason {
                league_id,
                name: "Invite Season".to_string(),
                slug: format!("invite-season-{}", uuid::Uuid::new_v4()),
                description: None,
                registration_start: None,
                registration_end: None,
                season_start: None,
                season_end: None,
                team_size_min: Some(5),
                team_size_max: Some(7),
                max_substitutes: Some(2),
                max_teams: None,
                created_by: user_id,
            })
            .await
            .unwrap();

        let team = team_repo
            .create(CreateLeagueTeam {
                league_id,
                name: "Invite Team".to_string(),
                tag: "INV".to_string(),
                description: None,
                logo_url: None,
                primary_color: None,
                secondary_color: None,
                owner_player_id,
            })
            .await
            .unwrap();

        let team_season = team_season_repo
            .create(CreateLeagueTeamSeason {
                team_id: team.id,
                season_id: season.id,
            })
            .await
            .unwrap();

        (team_season.id, owner_player_id, user_id)
    }

    #[tokio::test]
    async fn test_create_invitation() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamInvitationRepository::new(db.pool.clone());
        let (team_season_id, _, user_id) = setup_invitation_context(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "invitee").await;

        let invitation = repo
            .create(CreateLeagueTeamInvitation {
                team_season_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Invite,
                role: LeagueTeamRole::Player,
                message: Some("Join our team!".to_string()),
                invited_by: Some(user_id),
            })
            .await
            .unwrap();

        assert_eq!(invitation.player_id, player_id);
        assert_eq!(invitation.invitation_type, LeagueTeamInvitationType::Invite);
        assert_eq!(invitation.role, LeagueTeamRole::Player);
        assert_eq!(invitation.status, LeagueTeamInvitationStatus::Pending);
        assert!(invitation.expires_at > Utc::now());
    }

    #[tokio::test]
    async fn test_find_invitation_by_id() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamInvitationRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_invitation_context(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "findbyid").await;

        let created = repo
            .create(CreateLeagueTeamInvitation {
                team_season_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Invite,
                role: LeagueTeamRole::Player,
                message: None,
                invited_by: None,
            })
            .await
            .unwrap();

        let found = repo.find_by_id(created.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn test_update_invitation_status() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamInvitationRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_invitation_context(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "updateinvite").await;

        let invitation = repo
            .create(CreateLeagueTeamInvitation {
                team_season_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Invite,
                role: LeagueTeamRole::Player,
                message: None,
                invited_by: None,
            })
            .await
            .unwrap();

        let updated = repo
            .update_status(invitation.id, LeagueTeamInvitationStatus::Accepted, None)
            .await
            .unwrap();

        assert_eq!(updated.status, LeagueTeamInvitationStatus::Accepted);
        assert!(updated.responded_at.is_some());
    }

    #[tokio::test]
    async fn test_find_pending_for_player() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamInvitationRepository::new(db.pool.clone());
        let (_team_season_id, _, _) = setup_invitation_context(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "pendingplayer").await;

        // Create multiple invitations
        for _ in 0..3 {
            let (ts_id, _, _) = setup_invitation_context(&db).await;
            repo.create(CreateLeagueTeamInvitation {
                team_season_id: ts_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Invite,
                role: LeagueTeamRole::Player,
                message: None,
                invited_by: None,
            })
            .await
            .unwrap();
        }

        let pending = repo.find_pending_for_player(player_id).await.unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[tokio::test]
    async fn test_find_existing_pending() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamInvitationRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_invitation_context(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "existingpending").await;

        // No existing pending
        let result = repo.find_existing_pending(team_season_id, player_id).await.unwrap();
        assert!(result.is_none());

        // Create invitation
        repo.create(CreateLeagueTeamInvitation {
            team_season_id,
            player_id,
            invitation_type: LeagueTeamInvitationType::Invite,
            role: LeagueTeamRole::Player,
            message: None,
            invited_by: None,
        })
        .await
        .unwrap();

        // Should find existing
        let result = repo.find_existing_pending(team_season_id, player_id).await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_cancel_pending_for_player() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamInvitationRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_invitation_context(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "cancelpending").await;

        let invitation = repo
            .create(CreateLeagueTeamInvitation {
                team_season_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Invite,
                role: LeagueTeamRole::Player,
                message: None,
                invited_by: None,
            })
            .await
            .unwrap();

        repo.cancel_pending_for_player(team_season_id, player_id).await.unwrap();

        let updated = repo.find_by_id(invitation.id).await.unwrap().unwrap();
        assert_eq!(updated.status, LeagueTeamInvitationStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_count_pending_for_player() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamInvitationRepository::new(db.pool.clone());
        let (player_id, _) = create_test_player(&db.pool, "countpending").await;

        // Create 5 pending invitations
        for _ in 0..5 {
            let (ts_id, _, _) = setup_invitation_context(&db).await;
            repo.create(CreateLeagueTeamInvitation {
                team_season_id: ts_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Invite,
                role: LeagueTeamRole::Player,
                message: None,
                invited_by: None,
            })
            .await
            .unwrap();
        }

        let count = repo.count_pending_for_player(player_id).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_create_request_type_invitation() {
        let db = TestDb::new().await;
        let repo = PgLeagueTeamInvitationRepository::new(db.pool.clone());
        let (team_season_id, _, _) = setup_invitation_context(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "requester").await;

        let invitation = repo
            .create(CreateLeagueTeamInvitation {
                team_season_id,
                player_id,
                invitation_type: LeagueTeamInvitationType::Request,
                role: LeagueTeamRole::Player,
                message: Some("I want to join!".to_string()),
                invited_by: None, // No inviter for requests
            })
            .await
            .unwrap();

        assert_eq!(invitation.invitation_type, LeagueTeamInvitationType::Request);
        assert!(invitation.invited_by.is_none());
    }
}

// =============================================================================
// LEAGUE SEASON PARTICIPANT REPOSITORY TESTS (Individual Format)
// =============================================================================

mod league_season_participant_repository {
    use super::*;

    async fn setup_participant_season(db: &TestDb) -> LeagueSeasonId {
        let league_id = create_test_league(&db.pool).await;
        let user_id = create_test_user(&db.pool, "participantsetup").await;
        let repo = PgLeagueSeasonRepository::new(db.pool.clone());

        let season = repo
            .create(CreateLeagueSeason {
                league_id,
                name: "Participant Season".to_string(),
                slug: format!("participant-season-{}", uuid::Uuid::new_v4()),
                description: None,
                registration_start: None,
                registration_end: None,
                season_start: None,
                season_end: None,
                team_size_min: None, // Individual format
                team_size_max: None,
                max_substitutes: None,
                max_teams: None,
                created_by: user_id,
            })
            .await
            .unwrap();

        season.id
    }

    #[tokio::test]
    async fn test_register_participant() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonParticipantRepository::new(db.pool.clone());
        let season_id = setup_participant_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "registerparticipant").await;

        let participant = repo
            .register(RegisterLeagueSeasonParticipant { season_id, player_id })
            .await
            .unwrap();

        assert_eq!(participant.season_id, season_id);
        assert_eq!(participant.player_id, player_id);
        assert_eq!(participant.matches_played, 0);
    }

    #[tokio::test]
    async fn test_find_participant_by_id() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonParticipantRepository::new(db.pool.clone());
        let season_id = setup_participant_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "findparticipant").await;

        let created = repo
            .register(RegisterLeagueSeasonParticipant { season_id, player_id })
            .await
            .unwrap();

        let found = repo.find_by_id(created.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn test_find_by_season_and_player() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonParticipantRepository::new(db.pool.clone());
        let season_id = setup_participant_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "findseasonplayer").await;

        repo.register(RegisterLeagueSeasonParticipant { season_id, player_id })
            .await
            .unwrap();

        let found = repo.find_by_season_and_player(season_id, player_id).await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_withdraw_participant() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonParticipantRepository::new(db.pool.clone());
        let season_id = setup_participant_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "withdrawparticipant").await;

        let participant = repo
            .register(RegisterLeagueSeasonParticipant { season_id, player_id })
            .await
            .unwrap();

        let withdrawn = repo.withdraw(participant.id).await.unwrap();

        assert_eq!(withdrawn.status.to_string(), "withdrawn");
        assert!(withdrawn.withdrawn_at.is_some());
    }

    #[tokio::test]
    async fn test_is_registered() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonParticipantRepository::new(db.pool.clone());
        let season_id = setup_participant_season(&db).await;
        let (player_id, _) = create_test_player(&db.pool, "isregisteredparticipant").await;
        let (other_player_id, _) = create_test_player(&db.pool, "notregistered").await;

        assert!(!repo.is_registered(season_id, player_id).await.unwrap());

        repo.register(RegisterLeagueSeasonParticipant { season_id, player_id })
            .await
            .unwrap();

        assert!(repo.is_registered(season_id, player_id).await.unwrap());
        assert!(!repo.is_registered(season_id, other_player_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_list_participants_by_season() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonParticipantRepository::new(db.pool.clone());
        let season_id = setup_participant_season(&db).await;

        for i in 1..=10 {
            let (player_id, _) = create_test_player(&db.pool, &format!("listparticipant{i}")).await;
            repo.register(RegisterLeagueSeasonParticipant { season_id, player_id })
                .await
                .unwrap();
        }

        let (participants, count) = repo.list_by_season(season_id, None, 10, 0).await.unwrap();
        assert_eq!(participants.len(), 10);
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_list_participants_with_pagination() {
        let db = TestDb::new().await;
        let repo = PgLeagueSeasonParticipantRepository::new(db.pool.clone());
        let season_id = setup_participant_season(&db).await;

        for i in 1..=20 {
            let (player_id, _) = create_test_player(&db.pool, &format!("pageparticipant{i}")).await;
            repo.register(RegisterLeagueSeasonParticipant { season_id, player_id })
                .await
                .unwrap();
        }

        let (page1, count) = repo.list_by_season(season_id, None, 5, 0).await.unwrap();
        assert_eq!(page1.len(), 5);
        assert_eq!(count, 20);

        let (page2, _) = repo.list_by_season(season_id, None, 5, 5).await.unwrap();
        assert_eq!(page2.len(), 5);
    }
}
