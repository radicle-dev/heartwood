use crate::commit::parse::{parse_body, try_parse_trailers};

#[test]
fn body_no_paragraph_separator_means_no_trailers() {
    // A body with no blank line cannot have a trailing trailer paragraph.
    let (message, trailers) = parse_body("Just a message with no blank line");
    assert_eq!(message, "Just a message with no blank line");
    assert!(trailers.is_empty());
}

#[test]
fn body_last_paragraph_not_trailers_stays_in_message() {
    let body = "Short description\n\nThis paragraph has no Token: value lines.";
    let (message, trailers) = parse_body(body);
    assert_eq!(message, body);
    assert!(trailers.is_empty());
}

#[test]
fn trailers_rejects_line_without_separator() {
    // A line that contains no ": " cannot be a trailer.
    assert!(try_parse_trailers("NotATrailerLine").is_none());
}

#[test]
fn trailers_rejects_invalid_token_chars() {
    // Token characters must be alphanumeric or '-'; spaces are not allowed.
    assert!(try_parse_trailers("Invalid Token: value").is_none());
}

#[test]
fn trailers_accepts_empty_input() {
    // An empty paragraph produces an empty trailer list rather than None.
    // (parse_body guards against this with the is_empty() check, but the
    // helper itself is defined to return Some([]) for an empty iterator.)
    let result = try_parse_trailers("");
    assert_eq!(result, Some(vec![]));
}
