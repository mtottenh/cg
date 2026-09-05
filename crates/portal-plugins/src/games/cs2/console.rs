//! The admin server console's CS2 knowledge.
//!
//! What `status` prints, which console commands the portal refuses to pass
//! through, how to quote an argument, and how to change map (design:
//! docs/server-console-design.md).
//!
//! `status` has no fixed contract, so [`parse_status`] is lenient: header
//! lines are matched by prefix, player rows by shape, and anything it does
//! not recognise is ignored. Two row shapes are known — the CS:GO-era
//! `# userid name uniqueid connected ping loss state rate adr` and the CS2
//! `id time ping loss state rate adr name` — and a Steam id is picked out of
//! either wherever it appears. Fixtures in the tests below are what the
//! parser was built against; a dump from a real host supersedes them.

use crate::games::cs2::matchzy::workshop_numeric_id;
use crate::traits::MapInfo;
use std::sync::LazyLock;

/// Steam's 64-bit id base for individual accounts.
const STEAM64_BASE: u64 = 76_561_197_960_265_728;

/// A player row from `status`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConnectedPlayer {
    pub userid: u32,
    pub name: String,
    /// Steam ID64 when the row carried any Steam id form; bots have none.
    pub steam_id64: Option<u64>,
    pub bot: bool,
    pub connected_secs: Option<u32>,
    pub ping: Option<u32>,
    pub loss: Option<u32>,
    pub state: Option<String>,
}

/// What `status` said about the server.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ServerStatus {
    pub hostname: Option<String>,
    /// Engine map name, e.g. `de_mirage`; for a workshop map the last path
    /// segment of whatever the server printed.
    pub map: Option<String>,
    pub humans: Option<u32>,
    pub bots: Option<u32>,
    pub max_players: Option<u32>,
    pub players: Vec<ConnectedPlayer>,
}

impl ServerStatus {
    /// Humans plus bots, from the `players :` line when present, else counted.
    #[must_use]
    pub fn player_count(&self) -> u32 {
        match (self.humans, self.bots) {
            (Some(h), Some(b)) => h + b,
            (Some(h), None) => h,
            _ => u32::try_from(self.players.len()).unwrap_or(u32::MAX),
        }
    }
}

static MAP_LINE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?im)^\s*map\s*:\s*(\S+)").unwrap());
static HOSTNAME_LINE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?im)^\s*hostname\s*:\s*(.+?)\s*$").unwrap());
static PLAYERS_LINE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?im)^\s*players\s*:\s*(\d+)\s+humans?,\s*(\d+)\s+bots?\s*\((\d+)").unwrap()
});
/// CS:GO shape: `# 2 1 "Name" STEAM_1:0:123 05:23 45 0 active 786432 1.2.3.4:27005`.
static ROW_HASH: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r#"^#\s*(?P<userid>\d+)\s+\d+\s+"(?P<name>.*)"\s+(?P<uid>\S+)\s+(?P<time>\S+)\s+(?P<ping>\d+)\s+(?P<loss>\d+)\s+(?P<state>\S+)"#,
    )
    .unwrap()
});
/// CS2 shape: `  2  00:35   30    0     active 786432 1.2.3.4:27005 'Name'`.
static ROW_CS2: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"^\s*(?P<userid>\d+)\s+(?P<time>\S+)\s+(?P<ping>\d+)\s+(?P<loss>\d+)\s+(?P<state>\S+)\s+\d+\s+(?P<adr>\S+)\s+'(?P<name>.*)'\s*$",
    )
    .unwrap()
});
static STEAM3: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\[U:1:(\d+)\]").unwrap());
static STEAM2: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"STEAM_[0-5]:([01]):(\d+)").unwrap());
static ADDRESS: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(?::\d{1,5})?\b").unwrap()
});

/// Parse `status` output. Never fails; unknown lines are skipped.
#[must_use]
pub fn parse_status(output: &str) -> ServerStatus {
    let mut status = ServerStatus::default();
    if let Some(c) = HOSTNAME_LINE.captures(output) {
        status.hostname = Some(c[1].to_string());
    }
    if let Some(c) = MAP_LINE.captures(output) {
        let printed = &c[1];
        status.map = Some(
            printed
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(printed)
                .trim_end_matches(".vpk")
                .to_string(),
        );
    }
    if let Some(c) = PLAYERS_LINE.captures(output) {
        status.humans = c[1].parse().ok();
        status.bots = c[2].parse().ok();
        status.max_players = c[3].parse().ok();
    }
    for line in output.lines() {
        if let Some(p) = parse_player_row(line) {
            status.players.push(p);
        }
    }
    status
}

fn parse_player_row(line: &str) -> Option<ConnectedPlayer> {
    let trimmed = line.trim_end();
    if trimmed.starts_with("#end")
        || trimmed.contains("userid name")
        || trimmed.contains(" state ")
            && trimmed.contains(" name")
            && trimmed.contains("ping")
            && !trimmed.contains('\'')
            && !trimmed.contains('"')
    {
        return None;
    }
    let steam = steam_id64_in(trimmed);
    if let Some(c) = ROW_HASH.captures(trimmed) {
        let uid = &c["uid"];
        let bot = uid.eq_ignore_ascii_case("BOT");
        return Some(ConnectedPlayer {
            userid: c["userid"].parse().ok()?,
            name: c["name"].to_string(),
            steam_id64: if bot { None } else { steam },
            bot,
            connected_secs: parse_duration(&c["time"]),
            ping: c["ping"].parse().ok(),
            loss: c["loss"].parse().ok(),
            state: Some(c["state"].to_string()),
        });
    }
    if let Some(c) = ROW_CS2.captures(trimmed) {
        let bot = c["time"].eq_ignore_ascii_case("BOT") || c["adr"].eq_ignore_ascii_case("BOT");
        return Some(ConnectedPlayer {
            userid: c["userid"].parse().ok()?,
            name: c["name"].to_string(),
            steam_id64: if bot { None } else { steam },
            bot,
            connected_secs: parse_duration(&c["time"]),
            ping: c["ping"].parse().ok(),
            loss: c["loss"].parse().ok(),
            state: Some(c["state"].to_string()),
        });
    }
    None
}

/// A Steam ID64 from any Steam id form on the line.
#[must_use]
pub fn steam_id64_in(text: &str) -> Option<u64> {
    if let Some(c) = STEAM3.captures(text) {
        return c[1].parse::<u64>().ok().map(|n| STEAM64_BASE + n);
    }
    if let Some(c) = STEAM2.captures(text) {
        let y: u64 = c[1].parse().ok()?;
        let z: u64 = c[2].parse().ok()?;
        return Some(STEAM64_BASE + z * 2 + y);
    }
    None
}

fn parse_duration(s: &str) -> Option<u32> {
    let parts: Vec<u32> = s
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<_>>>()?;
    match parts.as_slice() {
        [m, sec] => Some(m * 60 + sec),
        [h, m, sec] => Some(h * 3600 + m * 60 + sec),
        _ => None,
    }
}

/// Replace player addresses with a marker before the text is stored or shown.
#[must_use]
pub fn redact_addresses(output: &str) -> String {
    ADDRESS.replace_all(output, "<addr>").into_owned()
}

/// Strip control characters (keeping newline and tab) and bound the length.
#[must_use]
pub fn sanitise_output(output: &str, max_bytes: usize) -> String {
    let cleaned: String = output
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    if cleaned.len() <= max_bytes {
        return cleaned;
    }
    let mut end = max_bytes;
    while !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[truncated]", &cleaned[..end])
}

/// Verbs the portal owns: its RCON, webhook and demo settings.
const PORTAL_OWNED: &[&str] = &[
    "rcon_password",
    "sv_password",
    "tv_password",
    "matchzy_loadmatch_url",
    "matchzy_loadmatch",
    "matchzy_loadbackup_url",
    "sv_downloadurl",
    "host_writeconfig",
];
const PORTAL_OWNED_PREFIXES: &[&str] = &[
    "sv_rcon_",
    "matchzy_remote_",
    "matchzy_demo_upload_",
    "logaddress_",
];
/// Verbs that run commands the check cannot see.
const NO_ESCAPE: &[&str] = &["alias", "exec"];
/// Verbs that end the server process.
const PROCESS: &[&str] = &["quit", "exit", "_restart", "killserver", "crash"];

/// Why a raw console command is refused, if it is. The agent applies the
/// same rule (`exec_refusal` in portal-server-agent); this copy answers
/// with a 400 before anything crosses the channel.
#[must_use]
pub fn exec_refusal(command: &str) -> Option<String> {
    if command.chars().any(char::is_control) {
        return Some("control characters are not allowed".to_string());
    }
    if command.contains(';') {
        return Some("one command per request; ';' is not allowed".to_string());
    }
    let verb = command
        .trim_start()
        .split(char::is_whitespace)
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if verb.is_empty() {
        return Some("empty command".to_string());
    }
    if PORTAL_OWNED.contains(&verb.as_str())
        || PORTAL_OWNED_PREFIXES.iter().any(|p| verb.starts_with(p))
    {
        return Some(format!("{verb} is portal-owned"));
    }
    if NO_ESCAPE.contains(&verb.as_str()) {
        return Some(format!("{verb} could run commands the check cannot see"));
    }
    if PROCESS.contains(&verb.as_str()) {
        return Some(format!("{verb} would end the server process"));
    }
    None
}

/// Quote one console argument, refusing anything that could break out of the
/// quotes or chain a command.
pub fn console_quote(arg: &str) -> Result<String, String> {
    if arg.contains('"') || arg.contains(';') || arg.chars().any(char::is_control) {
        return Err("argument contains console metacharacters".to_string());
    }
    Ok(format!("\"{arg}\""))
}

fn is_safe_map_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The console command that loads a catalogue map: `host_workshop_map <id>`
/// for a workshop map, `changelevel <engine name>` otherwise — the same
/// rule the match-config builder applies to maplist tokens.
pub fn map_change_command(map: &MapInfo) -> Result<String, String> {
    if let Some(external) = map.external_id.as_deref() {
        return workshop_numeric_id(external)
            .map(|id| format!("host_workshop_map {id}"))
            .ok_or_else(|| {
                format!(
                    "'{}' has a workshop id that is not numeric",
                    map.display_name
                )
            });
    }
    let name = map.engine_name.as_deref().unwrap_or(&map.id);
    if !is_safe_map_name(name) {
        return Err(format!("'{name}' is not a map name the console can take"));
    }
    Ok(format!("changelevel {name}"))
}

/// The console command for a free-text target: a workshop URL or numeric
/// id loads through the workshop, anything else is a map name.
pub fn custom_map_command(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("enter a map name, a workshop id or a workshop link".to_string());
    }
    if let Some(id) = workshop_numeric_id(trimmed) {
        return Ok(format!("host_workshop_map {id}"));
    }
    if !is_safe_map_name(trimmed) {
        return Err(format!(
            "'{trimmed}' is not a map name the console can take"
        ));
    }
    Ok(format!("changelevel {trimmed}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CS2_STATUS: &str = "hostname: CS2 10 Mans #1\n\
version : 1.40.7.3/14073 10529 secure  public\n\
os/type : Linux dedicated\n\
map     : de_ancient\n\
players : 2 humans, 1 bots (12 max) (not hibernating) (unreserved)\n\
\n\
  id     time ping loss      state   rate adr name\n\
  0     BOT    0    0     active      0 BOT 'Kyle'\n\
  2  05:23   31    0     active 786432 203.0.113.9:27005 'Player One'\n\
  3  00:12   58    2     active 786432 198.51.100.4:27005 'Two [U:1:12345678]'\n\
#end\n";

    const CSGO_STATUS: &str = "hostname: legacy\n\
map     : workshop/3070284539/de_cache\n\
players : 1 humans, 0 bots (10/0 max) (hibernating)\n\
# userid name uniqueid connected ping loss state rate adr\n\
#  2 1 \"Solo\" STEAM_1:1:44 01:02 45 0 active 786432 192.0.2.7:27005\n\
#end\n";

    #[test]
    fn parses_cs2_shape() {
        let s = parse_status(CS2_STATUS);
        assert_eq!(s.hostname.as_deref(), Some("CS2 10 Mans #1"));
        assert_eq!(s.map.as_deref(), Some("de_ancient"));
        assert_eq!(
            (s.humans, s.bots, s.max_players),
            (Some(2), Some(1), Some(12))
        );
        assert_eq!(s.player_count(), 3);
        assert_eq!(s.players.len(), 3);
        let bot = &s.players[0];
        assert!(bot.bot && bot.steam_id64.is_none() && bot.name == "Kyle");
        let one = &s.players[1];
        assert_eq!(
            (one.userid, one.ping, one.loss, one.connected_secs),
            (2, Some(31), Some(0), Some(323))
        );
        assert_eq!(one.steam_id64, None);
        let two = &s.players[2];
        assert_eq!(two.steam_id64, Some(STEAM64_BASE + 12_345_678));
    }

    #[test]
    fn parses_csgo_shape_and_workshop_map_path() {
        let s = parse_status(CSGO_STATUS);
        assert_eq!(s.map.as_deref(), Some("de_cache"));
        assert_eq!(s.players.len(), 1);
        let p = &s.players[0];
        assert_eq!(p.name, "Solo");
        assert_eq!(p.steam_id64, Some(STEAM64_BASE + 44 * 2 + 1));
        assert_eq!(p.connected_secs, Some(62));
    }

    #[test]
    fn empty_or_garbage_gives_an_empty_status() {
        assert_eq!(parse_status(""), ServerStatus::default());
        assert_eq!(parse_status("Unknown command 'status'\n").players.len(), 0);
    }

    #[test]
    fn addresses_are_redacted() {
        let r = redact_addresses(CS2_STATUS);
        assert!(!r.contains("203.0.113.9"));
        assert!(r.contains("<addr>"));
        assert!(
            parse_status(&r).players.len() == 3,
            "redaction must not break parsing"
        );
    }

    #[test]
    fn output_is_bounded_and_clean() {
        let s = sanitise_output("ok\u{7}\r\n\tx", 100);
        assert_eq!(s, "ok\n\tx");
        assert!(sanitise_output(&"é".repeat(100), 21).ends_with("[truncated]"));
    }

    #[test]
    fn refusals_mirror_the_agent() {
        assert_eq!(exec_refusal("status"), None);
        assert_eq!(exec_refusal("mp_warmuptime 60"), None);
        assert!(exec_refusal("rcon_password x").is_some());
        assert!(exec_refusal("SV_PASSWORD x").is_some());
        assert!(exec_refusal("matchzy_remote_log_url x").is_some());
        assert!(exec_refusal("alias a b").is_some());
        assert!(exec_refusal("exec server").is_some());
        assert!(exec_refusal("quit").is_some());
        assert!(exec_refusal("say hi; quit").is_some());
        assert!(exec_refusal("status\r").is_some());
    }

    #[test]
    fn quoting_and_map_commands() {
        assert_eq!(console_quote("gl hf").unwrap(), "\"gl hf\"");
        assert!(console_quote("a\"b").is_err());
        assert!(console_quote("a;b").is_err());
        let stock = MapInfo {
            id: "de_mirage".into(),
            display_name: "Mirage".into(),
            image_url: None,
            game_modes: vec![],
            engine_name: None,
            external_id: None,
            external_url: None,
        };
        assert_eq!(map_change_command(&stock).unwrap(), "changelevel de_mirage");
        let workshop = MapInfo {
            id: "cache".into(),
            display_name: "Cache".into(),
            image_url: None,
            game_modes: vec![],
            engine_name: Some("de_cache".into()),
            external_id: Some(
                "https://steamcommunity.com/sharedfiles/filedetails/?id=3070284539".into(),
            ),
            external_url: None,
        };
        assert_eq!(
            map_change_command(&workshop).unwrap(),
            "host_workshop_map 3070284539"
        );
        assert_eq!(
            custom_map_command("3070284539").unwrap(),
            "host_workshop_map 3070284539"
        );
        assert_eq!(
            custom_map_command(" de_nuke ").unwrap(),
            "changelevel de_nuke"
        );
        assert!(custom_map_command("de_nuke; quit").is_err());
        assert!(custom_map_command("").is_err());
    }
}
