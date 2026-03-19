//! VADER-style lexicon-based sentiment analysis for financial text.
//!
//! Implements a domain-specific variant of the VADER (Valence Aware Dictionary
//! and sEntiment Reasoner) algorithm tailored for financial news headlines and
//! summaries.  Key features:
//!
//! * **Financial lexicon** — 100+ terms with pre-assigned polarity weights
//!   (e.g. "bullish" +0.8, "crash" −0.9)
//! * **Negation handling** — "not bullish" flips the polarity
//! * **Degree modifiers** — "very strong" amplifies, "slightly weak" dampens
//! * **Capitalisation boost** — ALL-CAPS words get a small intensity bump
//! * **Punctuation boost** — trailing `!` adds emphasis
//!
//! The final compound score is normalised to \[−1.0, +1.0\] using a sigmoid-
//! like function identical to VADER's `normalize()`.

use std::collections::HashMap;
use std::sync::LazyLock;

// ── Lexicon ──────────────────────────────────────────────────────────────────

/// Financial sentiment lexicon: word → base polarity (−1.0 … +1.0).
static LEXICON: LazyLock<HashMap<&'static str, f32>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Strongly positive (finance-specific)
    m.insert("bullish", 0.8);
    m.insert("surge", 0.8);
    m.insert("soar", 0.8);
    m.insert("rally", 0.7);
    m.insert("boom", 0.7);
    m.insert("outperform", 0.7);
    m.insert("breakout", 0.7);
    m.insert("upgrade", 0.6);
    m.insert("growth", 0.6);
    m.insert("profit", 0.6);
    m.insert("gains", 0.6);
    m.insert("revenue", 0.4);
    m.insert("dividend", 0.5);
    m.insert("beat", 0.5);
    m.insert("exceeds", 0.5);
    m.insert("strong", 0.5);
    m.insert("positive", 0.5);
    m.insert("buy", 0.4);
    m.insert("overweight", 0.4);
    m.insert("upside", 0.5);
    m.insert("recovery", 0.5);
    m.insert("record", 0.4);
    m.insert("high", 0.3);
    m.insert("up", 0.2);
    m.insert("rise", 0.4);
    m.insert("rising", 0.4);
    m.insert("climbs", 0.4);
    m.insert("accelerate", 0.5);
    m.insert("improve", 0.4);
    m.insert("optimistic", 0.6);
    m.insert("opportunity", 0.4);
    m.insert("momentum", 0.3);
    m.insert("accumulate", 0.3);
    m.insert("expand", 0.4);
    m.insert("outpace", 0.5);

    // Strongly negative (finance-specific)
    m.insert("bearish", -0.8);
    m.insert("crash", -0.9);
    m.insert("plunge", -0.8);
    m.insert("collapse", -0.9);
    m.insert("recession", -0.8);
    m.insert("crisis", -0.8);
    m.insert("downgrade", -0.7);
    m.insert("underperform", -0.7);
    m.insert("loss", -0.6);
    m.insert("losses", -0.6);
    m.insert("decline", -0.6);
    m.insert("miss", -0.5);
    m.insert("misses", -0.5);
    m.insert("weak", -0.5);
    m.insert("negative", -0.5);
    m.insert("sell", -0.4);
    m.insert("underweight", -0.4);
    m.insert("downside", -0.5);
    m.insert("debt", -0.4);
    m.insert("risk", -0.3);
    m.insert("volatile", -0.3);
    m.insert("volatility", -0.3);
    m.insert("warning", -0.6);
    m.insert("layoffs", -0.6);
    m.insert("bankruptcy", -0.9);
    m.insert("fraud", -0.9);
    m.insert("investigation", -0.5);
    m.insert("default", -0.8);
    m.insert("slump", -0.6);
    m.insert("tumble", -0.6);
    m.insert("plummet", -0.8);
    m.insert("fall", -0.4);
    m.insert("falling", -0.4);
    m.insert("drops", -0.4);
    m.insert("down", -0.2);
    m.insert("pessimistic", -0.6);
    m.insert("fear", -0.5);
    m.insert("panic", -0.7);
    m.insert("sell-off", -0.7);
    m.insert("selloff", -0.7);
    m.insert("stagnant", -0.3);
    m.insert("erode", -0.5);

    m
});

/// Words that negate the next sentiment-bearing token.
const NEGATION_WORDS: &[&str] = &[
    "not", "no", "never", "neither", "nobody", "nothing",
    "nowhere", "nor", "cannot", "without", "hardly", "barely",
    "scarcely", "don't", "doesn't", "didn't", "isn't", "wasn't",
    "shouldn't", "wouldn't", "couldn't", "won't",
];

/// Degree modifiers: word → multiplier.
static BOOSTERS: LazyLock<HashMap<&'static str, f32>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Amplifiers
    m.insert("very", 1.3);
    m.insert("extremely", 1.5);
    m.insert("incredibly", 1.5);
    m.insert("significantly", 1.4);
    m.insert("substantially", 1.3);
    m.insert("massively", 1.5);
    m.insert("sharply", 1.3);
    m.insert("strongly", 1.3);
    m.insert("hugely", 1.4);
    m.insert("dramatically", 1.4);
    m.insert("remarkably", 1.3);
    // Dampeners
    m.insert("slightly", 0.7);
    m.insert("somewhat", 0.8);
    m.insert("marginally", 0.7);
    m.insert("modestly", 0.8);
    m.insert("barely", 0.6);
    m.insert("partly", 0.7);
    m
});

// ── Constants ────────────────────────────────────────────────────────────────

const NEGATION_SCALAR: f32 = -0.74; // VADER constant
const CAPS_INCR: f32 = 0.1; // boost for ALL-CAPS
const EXCL_INCR: f32 = 0.05; // boost per trailing `!`
const MAX_EXCL_BOOST: f32 = 0.2; // cap on `!` boost
const ALPHA: f32 = 15.0; // normalisation constant (VADER default)

// ── Public API ───────────────────────────────────────────────────────────────

/// Compute a compound sentiment score in \[−1.0, +1.0\] for the given text.
///
/// Positive values indicate bullish/positive sentiment, negative values
/// indicate bearish/negative sentiment, and values near zero are neutral.
pub fn score(text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }

    let tokens = tokenise(text);
    if tokens.is_empty() {
        return 0.0;
    }

    let mut sentiments: Vec<f32> = Vec::new();

    for (i, token) in tokens.iter().enumerate() {
        let word = token.to_lowercase();

        // Look up base valence
        let Some(&base_valence) = LEXICON.get(word.as_str()) else {
            continue;
        };

        let mut valence = base_valence;

        // 1. Capitalisation boost — if the original token is ALL-CAPS
        if token.len() > 1 && token.chars().all(|c| c.is_uppercase()) {
            if valence > 0.0 {
                valence += CAPS_INCR;
            } else {
                valence -= CAPS_INCR;
            }
        }

        // 2. Degree modifier — check the preceding token
        if i > 0 {
            let prev = tokens[i - 1].to_lowercase();
            if let Some(&mult) = BOOSTERS.get(prev.as_str()) {
                valence *= mult;
            }
        }

        // 3. Negation — check the 3 preceding tokens for negation words
        let negated = (1..=3).any(|offset| {
            if i >= offset {
                let w = tokens[i - offset].to_lowercase();
                NEGATION_WORDS.contains(&w.as_str())
            } else {
                false
            }
        });
        if negated {
            valence *= NEGATION_SCALAR;
        }

        sentiments.push(valence);
    }

    if sentiments.is_empty() {
        return 0.0;
    }

    // Sum raw valences
    let mut compound: f32 = sentiments.iter().sum();

    // Punctuation boost (exclamation marks signal emphasis)
    let excl_count = text.chars().filter(|&c| c == '!').count() as f32;
    let excl_boost = (excl_count * EXCL_INCR).min(MAX_EXCL_BOOST);
    if compound > 0.0 {
        compound += excl_boost;
    } else if compound < 0.0 {
        compound -= excl_boost;
    }

    // VADER normalisation: compound / sqrt(compound² + alpha)
    normalize(compound, ALPHA)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// VADER-style normalisation function.  Maps an unbounded sum into \[−1, +1\].
fn normalize(score: f32, alpha: f32) -> f32 {
    let norm = score / (score * score + alpha).sqrt();
    norm.clamp(-1.0, 1.0)
}

/// Simple whitespace + punctuation tokeniser.
fn tokenise(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == ':')
        .map(|s| s.trim_matches(|c: char| c == '.' || c == '(' || c == ')' || c == '"' || c == '\''))
        .filter(|s| !s.is_empty())
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_headline() {
        let s = score("Stock surges on strong earnings and bullish outlook");
        assert!(s > 0.3, "expected strong positive, got {s}");
    }

    #[test]
    fn negative_headline() {
        let s = score("Company crashes after bearish warning and losses");
        assert!(s < -0.3, "expected strong negative, got {s}");
    }

    #[test]
    fn negation_flips_polarity() {
        let plain = score("This stock is bullish");
        let negated = score("This stock is not bullish");
        assert!(plain > 0.0, "plain should be positive: {plain}");
        assert!(negated < plain, "negation should lower score: {negated} vs {plain}");
    }

    #[test]
    fn degree_modifier_amplifies() {
        let plain = score("earnings growth");
        let boosted = score("extremely strong earnings growth");
        assert!(boosted > plain, "boosted should exceed plain: {boosted} vs {plain}");
    }

    #[test]
    fn caps_boost() {
        let lower = score("stock surges");
        let upper = score("stock SURGES");
        assert!(upper >= lower, "ALL-CAPS should not reduce score: {upper} vs {lower}");
    }

    #[test]
    fn neutral_text_near_zero() {
        let s = score("The company held its quarterly meeting on Tuesday");
        assert!(s.abs() < 0.3, "neutral text should be near zero, got {s}");
    }

    #[test]
    fn empty_text() {
        assert_eq!(score(""), 0.0);
    }

    #[test]
    fn compound_score_bounded() {
        // Even extremely long bullish text should stay within [-1, 1]
        let s = score("bullish surge rally boom outperform breakout gains profit upgrade growth bullish surge rally boom");
        assert!((-1.0..=1.0).contains(&s), "score out of range: {s}");
    }

    #[test]
    fn financial_bear_case() {
        let s = score("Recession fears mount as markets plunge amid crisis and panic sell-off");
        assert!(s < -0.5, "strong bear headline should be very negative, got {s}");
    }
}
