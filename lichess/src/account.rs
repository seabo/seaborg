//! Authenticated-account details from `GET /api/account`.

use serde::Deserialize;

/// The account behind the API token.
///
/// Only the fields the bot needs are modeled: the title distinguishes a BOT
/// account, and the game count gates the irreversible upgrade.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// The account id.
    pub id: String,
    /// The account username.
    pub username: String,
    /// The account title, if any. `BOT` marks a bot account.
    #[serde(default)]
    pub title: Option<String>,
    /// Aggregate game counts, present on normal accounts.
    #[serde(default)]
    pub count: Option<GameCount>,
}

/// Aggregate counts of an account's games.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GameCount {
    /// Total games the account has played across all categories.
    #[serde(default)]
    pub all: u64,
}

impl Account {
    /// Whether this is a BOT account, eligible to use the Bot API.
    pub fn is_bot(&self) -> bool {
        self.title.as_deref() == Some("BOT")
    }

    /// Total games played, defaulting to zero when Lichess omits the counts.
    pub fn games_played(&self) -> u64 {
        self.count.as_ref().map_or(0, |c| c.all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Account {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn bot_title_is_detected() {
        let account = parse(r#"{"id":"b","username":"Bot","title":"BOT","count":{"all":0}}"#);
        assert!(account.is_bot());
        assert_eq!(account.games_played(), 0);
    }

    #[test]
    fn human_account_is_not_a_bot() {
        let account = parse(r#"{"id":"h","username":"Human","title":null,"count":{"all":42}}"#);
        assert!(!account.is_bot());
        assert_eq!(account.games_played(), 42);
    }

    #[test]
    fn absent_count_reports_zero_games() {
        let account = parse(r#"{"id":"h","username":"Human"}"#);
        assert!(!account.is_bot());
        assert_eq!(account.games_played(), 0);
    }
}
