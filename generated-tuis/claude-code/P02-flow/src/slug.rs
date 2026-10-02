//! Card / column identifier generation and validation.
//!
//! Ids become path components (`cols/<column_id>/<card_id>.md`), so validation here is the
//! only thing standing between a user-supplied string and the filesystem. Nothing in this
//! module is specific to any particular id: slugs are derived from whatever title the user
//! typed, and validation is purely structural.

/// Longest id we accept. Keeps `<id>.md` comfortably inside the 255-byte limit that ext4 and
/// friends impose on a single path component, even after the `-NN` de-duplication suffix.
pub const MAX_ID_LEN: usize = 64;

/// Why an id was rejected. Rendered straight into the UI, so the text is user-facing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdError {
    Empty,
    TooLong,
    Separator,
    Traversal,
    LeadingDot,
    Control,
}

impl std::fmt::Display for IdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            IdError::Empty => "id must not be empty",
            IdError::TooLong => "id is too long (max 64 characters)",
            IdError::Separator => "id must not contain '/' or '\\'",
            IdError::Traversal => "id must not be '.' or '..'",
            IdError::LeadingDot => "id must not start with '.'",
            IdError::Control => "id must not contain control characters",
        };
        f.write_str(msg)
    }
}

/// Normalize a user-typed id: trim surrounding whitespace and drop a trailing `.md`, since
/// card ids are defined as filenames *without* the extension and users habitually type it.
pub fn normalize_id(raw: &str) -> String {
    let trimmed = raw.trim();
    match trimmed.strip_suffix(".md") {
        // Guard against ".md" itself normalizing to "", which would report the wrong error.
        Some(stem) if !stem.is_empty() => stem.to_string(),
        _ => trimmed.to_string(),
    }
}

/// Structural validation for anything used as a path component.
pub fn validate_id(id: &str) -> Result<(), IdError> {
    if id.is_empty() {
        return Err(IdError::Empty);
    }
    if id.chars().count() > MAX_ID_LEN {
        return Err(IdError::TooLong);
    }
    if id.contains('/') || id.contains('\\') {
        return Err(IdError::Separator);
    }
    if id == "." || id == ".." {
        return Err(IdError::Traversal);
    }
    if id.starts_with('.') {
        return Err(IdError::LeadingDot);
    }
    // Catches NUL along with newlines and anything else that would corrupt order.txt,
    // which is a line-oriented file.
    if id.chars().any(|c| c.is_control()) {
        return Err(IdError::Control);
    }
    Ok(())
}

/// Convenience wrapper: normalize then validate.
pub fn normalize_and_validate(raw: &str) -> Result<String, IdError> {
    let id = normalize_id(raw);
    validate_id(&id)?;
    Ok(id)
}

/// Derive a filesystem-friendly slug from arbitrary text.
///
/// ASCII alphanumerics are lowercased and kept; everything else collapses into a single `-`.
/// Scripts with no ASCII at all (CJK, Cyrillic, emoji-only titles) legitimately slugify to the
/// empty string — callers fall back to a generated id rather than treating that as an error.
pub fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if out.chars().count() > MAX_ID_LEN {
        out = out.chars().take(MAX_ID_LEN).collect();
    }
    out
}

/// Pick an id that is not already taken, given a desired base.
///
/// Tries `base`, then `base-2`, `base-3`, ... The base is truncated as needed so the suffixed
/// result still fits in [`MAX_ID_LEN`]. `taken` is a predicate rather than a set so callers can
/// answer it from the real filesystem instead of a cached list.
pub fn unique_id(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !base.is_empty() && !taken(base) {
        return base.to_string();
    }
    for n in 2u32.. {
        let suffix = format!("-{n}");
        let room = MAX_ID_LEN.saturating_sub(suffix.chars().count());
        let stem: String = base.chars().take(room).collect();
        let candidate = format!("{stem}{suffix}");
        if !taken(&candidate) {
            return candidate;
        }
        // Practically unreachable, but never spin forever on a pathological `taken`.
        if n > 100_000 {
            break;
        }
    }
    format!("{base}-x")
}

/// Fallback id for titles that slugify to nothing (e.g. a CJK-only title).
/// Numbered `card-1`, `card-2`, ... skipping ids that already exist.
pub fn fallback_card_id(taken: impl Fn(&str) -> bool) -> String {
    for n in 1u32.. {
        let candidate = format!("card-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        if n > 100_000 {
            break;
        }
    }
    "card".to_string()
}

/// Suggest a card id for a title: slug if usable, generated fallback otherwise.
pub fn suggest_card_id(title: &str, taken: impl Fn(&str) -> bool + Copy) -> String {
    let slug = slugify(title);
    if slug.is_empty() {
        fallback_card_id(taken)
    } else {
        unique_id(&slug, taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn slugify_handles_arbitrary_text() {
        assert_eq!(slugify("Fix login bug"), "fix-login-bug");
        assert_eq!(slugify("  Deploy   to   prod!! "), "deploy-to-prod");
        assert_eq!(slugify("API/v2 (beta)"), "api-v2-beta");
        assert_eq!(slugify("Release 1.2.3"), "release-1-2-3");
        assert_eq!(slugify("--leading and trailing--"), "leading-and-trailing");
        assert_eq!(slugify("MiXeD CaSe"), "mixed-case");
    }

    #[test]
    fn slugify_returns_empty_for_non_ascii_only_titles() {
        // Not an error: callers fall back to a generated id.
        assert_eq!(slugify("English-only text"), "");
        assert_eq!(slugify("🚀🚀"), "");
        assert_eq!(slugify("   "), "");
    }

    #[test]
    fn slugify_keeps_ascii_from_mixed_scripts() {
        assert_eq!(slugify("English-only text login English-only text"), "login");
    }

    #[test]
    fn slugify_respects_length_cap() {
        let long = "a".repeat(500);
        assert_eq!(slugify(&long).chars().count(), MAX_ID_LEN);
    }

    #[test]
    fn validate_rejects_path_escapes() {
        assert_eq!(validate_id(""), Err(IdError::Empty));
        assert_eq!(validate_id("a/b"), Err(IdError::Separator));
        assert_eq!(validate_id("a\\b"), Err(IdError::Separator));
        assert_eq!(validate_id(".."), Err(IdError::Traversal));
        assert_eq!(validate_id("."), Err(IdError::Traversal));
        assert_eq!(validate_id(".hidden"), Err(IdError::LeadingDot));
        assert_eq!(validate_id("a\nb"), Err(IdError::Control));
        assert_eq!(validate_id("a\0b"), Err(IdError::Control));
        assert_eq!(validate_id(&"x".repeat(65)), Err(IdError::TooLong));
    }

    #[test]
    fn validate_accepts_ordinary_and_unicode_ids() {
        assert!(validate_id("item-1").is_ok());
        assert!(validate_id("Card_42").is_ok());
        assert!(validate_id("English-only text").is_ok(), "unicode ids are valid filenames");
        assert!(validate_id("a.b").is_ok(), "dots are fine except leading");
        assert!(validate_id(&"x".repeat(64)).is_ok());
    }

    #[test]
    fn normalize_strips_md_extension_and_whitespace() {
        assert_eq!(normalize_id("  item-1  "), "item-1");
        assert_eq!(normalize_id("item-1.md"), "item-1");
        assert_eq!(normalize_id("item-1.MD"), "item-1.MD", "case-sensitive by design");
        // ".md" alone must not normalize to empty and mask the real error.
        assert_eq!(normalize_id(".md"), ".md");
        assert_eq!(normalize_and_validate(".md"), Err(IdError::LeadingDot));
    }

    #[test]
    fn unique_id_avoids_collisions() {
        let taken: HashSet<&str> = ["fix-bug", "fix-bug-2"].into_iter().collect();
        let is_taken = |s: &str| taken.contains(s);
        assert_eq!(unique_id("fix-bug", is_taken), "fix-bug-3");
        assert_eq!(unique_id("other", is_taken), "other");
    }

    #[test]
    fn unique_id_keeps_suffixed_result_within_cap() {
        let base = "a".repeat(MAX_ID_LEN);
        let id = unique_id(&base, |s| s == base);
        assert!(id.chars().count() <= MAX_ID_LEN);
        assert!(id.ends_with("-2"));
    }

    #[test]
    fn fallback_ids_skip_existing() {
        let taken: HashSet<&str> = ["card-1", "card-2"].into_iter().collect();
        assert_eq!(fallback_card_id(|s| taken.contains(s)), "card-3");
    }

    #[test]
    fn suggested_ids_are_always_valid() {
        let none = |_: &str| false;
        for title in ["Fix login bug", "English-only text", "   ", "🚀", "A"] {
            let id = suggest_card_id(title, none);
            assert!(validate_id(&id).is_ok(), "invalid id {id:?} from title {title:?}");
        }
    }
}
