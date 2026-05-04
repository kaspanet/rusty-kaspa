use regex::Regex;
use semver::Version;
use std::cmp::Ordering;
use std::fmt;

#[derive(Debug)]
pub struct UserAgentRuleSet {
    allow: Vec<UserAgentRule>,
    reject: Vec<UserAgentRule>,
}

#[derive(Debug)]
pub struct UserAgentRule {
    source: String,
    action: UserAgentRuleAction,
    matcher: UserAgentRuleMatcher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserAgentRuleAction {
    Allow,
    Reject,
}

#[derive(Debug)]
enum UserAgentRuleMatcher {
    Regex(Regex),
    Version { name: String, op: VersionOp, version: Version },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VersionOp {
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
    Ne,
}

#[derive(Debug)]
pub enum UserAgentRuleRejectReason<'a> {
    AllowanceExcluded,
    Rejection(&'a UserAgentRule),
}

#[derive(Debug, PartialEq, Eq)]
pub struct UserAgentRuleParseError(String);

impl UserAgentRuleSet {
    pub fn empty() -> Self {
        Self { allow: vec![], reject: vec![] }
    }

    pub fn parse_lossy(rules: &[String]) -> Self {
        let mut rule_set = Self::empty();
        for rule in rules {
            match UserAgentRule::parse(rule) {
                Ok(rule) => rule_set.add(rule),
                Err(err) => log::warn!("Ignoring invalid user agent rule `{}`: {}", rule, err),
            }
        }
        rule_set
    }

    pub fn reject_reason<'a>(&'a self, user_agent: &str) -> Option<UserAgentRuleRejectReason<'a>> {
        if !self.allow.is_empty() && !self.allow.iter().any(|rule| rule.is_match(user_agent)) {
            return Some(UserAgentRuleRejectReason::AllowanceExcluded);
        }

        self.reject.iter().find(|rule| rule.is_match(user_agent)).map(UserAgentRuleRejectReason::Rejection)
    }

    fn add(&mut self, rule: UserAgentRule) {
        match rule.action {
            UserAgentRuleAction::Allow => self.allow.push(rule),
            UserAgentRuleAction::Reject => self.reject.push(rule),
        }
    }
}

impl UserAgentRule {
    pub fn parse(source: &str) -> Result<Self, UserAgentRuleParseError> {
        let (action, matcher) = source
            .split_once(';')
            .ok_or_else(|| UserAgentRuleParseError("expected `<allow|reject>;<regex|reg|ver>:...`".to_string()))?;

        let action = match action.trim() {
            "allow" => UserAgentRuleAction::Allow,
            "reject" => UserAgentRuleAction::Reject,
            action => return Err(UserAgentRuleParseError(format!("unknown action `{}`", action))),
        };

        let (matcher_kind, matcher_expr) =
            matcher.split_once(':').ok_or_else(|| UserAgentRuleParseError("expected matcher kind followed by `:`".to_string()))?;

        let matcher = match matcher_kind.trim() {
            "regex" | "reg" => UserAgentRuleMatcher::Regex(
                Regex::new(matcher_expr.trim()).map_err(|err| UserAgentRuleParseError(format!("invalid regex: {}", err)))?,
            ),
            "ver" => parse_version_matcher(matcher_expr.trim())?,
            matcher_kind => return Err(UserAgentRuleParseError(format!("unknown matcher `{}`", matcher_kind))),
        };

        Ok(Self { source: source.to_string(), action, matcher })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    fn is_match(&self, user_agent: &str) -> bool {
        match &self.matcher {
            UserAgentRuleMatcher::Regex(regex) => regex.is_match(user_agent),
            UserAgentRuleMatcher::Version { name, op, version } => {
                user_agent_versions(user_agent, name).any(|ua_version| op.matches(&ua_version, version))
            }
        }
    }
}

impl VersionOp {
    fn matches(self, left: &Version, right: &Version) -> bool {
        let cmp = compare_version_core(left, right);
        matches!(
            (self, cmp),
            (VersionOp::Lt, Ordering::Less)
                | (VersionOp::Lte, Ordering::Less | Ordering::Equal)
                | (VersionOp::Gt, Ordering::Greater)
                | (VersionOp::Gte, Ordering::Greater | Ordering::Equal)
                | (VersionOp::Eq, Ordering::Equal)
                | (VersionOp::Ne, Ordering::Less | Ordering::Greater)
        )
    }
}

impl fmt::Display for UserAgentRuleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UserAgentRuleParseError {}

fn parse_version_matcher(expr: &str) -> Result<UserAgentRuleMatcher, UserAgentRuleParseError> {
    let (op_index, op_len, op) =
        find_version_op(expr).ok_or_else(|| UserAgentRuleParseError("expected version comparison operator".to_string()))?;
    let name = expr[..op_index].trim().trim_end_matches(':').trim();
    let version = expr[op_index + op_len..].trim();

    if name.is_empty() {
        return Err(UserAgentRuleParseError("expected user agent name in version rule".to_string()));
    }

    let version = Version::parse(version).map_err(|err| UserAgentRuleParseError(format!("invalid version: {}", err)))?;
    Ok(UserAgentRuleMatcher::Version { name: name.to_string(), op, version })
}

fn find_version_op(expr: &str) -> Option<(usize, usize, VersionOp)> {
    // `min_by_key` keeps the first candidate on equal indexes, so we keep longer
    // overlapping operators before their prefixes (`<=` before `<`, etc.).
    [
        ("<=", VersionOp::Lte),
        (">=", VersionOp::Gte),
        ("==", VersionOp::Eq),
        ("!=", VersionOp::Ne),
        ("<", VersionOp::Lt),
        (">", VersionOp::Gt),
        ("=", VersionOp::Eq),
    ]
    .iter()
    .filter_map(|(symbol, op)| expr.find(symbol).map(|index| (index, symbol.len(), *op)))
    .min_by_key(|(index, _, _)| *index)
}

fn compare_version_core(left: &Version, right: &Version) -> Ordering {
    (left.major, left.minor, left.patch).cmp(&(right.major, right.minor, right.patch))
}

fn user_agent_versions<'a>(user_agent: &'a str, expected_name: &'a str) -> impl Iterator<Item = Version> + 'a {
    user_agent.split('/').filter_map(move |segment| {
        let (name, version) = segment.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case(expected_name) {
            return None;
        }

        let version = version.split_once('(').map_or(version, |(version, _)| version).trim();
        Version::parse(version).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::{UserAgentRule, UserAgentRuleRejectReason, UserAgentRuleSet};

    fn rules(rules: &[&str]) -> UserAgentRuleSet {
        let mut rule_set = UserAgentRuleSet::empty();
        for rule in rules {
            rule_set.add(UserAgentRule::parse(rule).unwrap());
        }
        rule_set
    }

    #[test]
    fn rejects_when_allow_rules_exist_and_none_match() {
        let rules = rules(&["allow;regex:(^|/)kaspad:"]);

        assert!(matches!(rules.reject_reason("/kaspa-dnsseeder:1.0.0/"), Some(UserAgentRuleRejectReason::AllowanceExcluded)));
    }

    #[test]
    fn reject_rules_veto_allowed_user_agents() {
        let rules = rules(&["allow;regex:(^|/)(kaspad|kaspa-dnsseeder):", "reject;ver:kaspad<1.1.1"]);

        assert!(matches!(rules.reject_reason("/kaspad:1.1.0/"), Some(UserAgentRuleRejectReason::Rejection(_))));
        assert!(rules.reject_reason("/kaspad:1.1.1-toc.1/").is_none());
        assert!(rules.reject_reason("/kaspa-dnsseeder:1.0.0/").is_none());
    }

    #[test]
    fn supports_version_rule_colon_form_and_comparison_ops() {
        assert!(UserAgentRule::parse("reject;ver:kaspad:<1.1.1").unwrap().is_match("/kaspad:1.1.0/"));
        assert!(UserAgentRule::parse("reject;ver:kaspad<=1.1.1").unwrap().is_match("/kaspad:1.1.1/"));
        assert!(!UserAgentRule::parse("reject;ver:kaspad<=1.1.1").unwrap().is_match("/kaspad:1.1.2/"));
        assert!(UserAgentRule::parse("reject;ver:kaspad>=1.1.1").unwrap().is_match("/kaspad:1.1.1-toc.1/"));
        assert!(!UserAgentRule::parse("reject;ver:kaspad>=1.1.1").unwrap().is_match("/kaspad:1.1.0/"));
        assert!(UserAgentRule::parse("reject;ver:kaspad>1.1.1").unwrap().is_match("/kaspad:1.1.2/"));
        assert!(UserAgentRule::parse("reject;ver:kaspad=1.1.1").unwrap().is_match("/kaspad:1.1.1/"));
        assert!(UserAgentRule::parse("reject;ver:kaspad==1.1.1").unwrap().is_match("/kaspad:1.1.1/"));
        assert!(UserAgentRule::parse("reject;ver:kaspad!=1.1.1").unwrap().is_match("/kaspad:1.1.2/"));
        assert!(!UserAgentRule::parse("reject;ver:kaspad!=1.1.1").unwrap().is_match("/kaspad:1.1.1-toc.1/"));
    }

    #[test]
    fn regex_rules_match_the_whole_user_agent() {
        let rules = rules(&["reject;reg:(^|/)kaspad:1\\.1\\.0"]);

        assert!(matches!(rules.reject_reason("/kaspad:1.1.0/kaspad:1.1.0/"), Some(UserAgentRuleRejectReason::Rejection(_))));
        assert!(rules.reject_reason("/kaspa-dnsseeder:1.1.0/").is_none());
    }
}
