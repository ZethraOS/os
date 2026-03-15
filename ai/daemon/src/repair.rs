/// Attempts to fix truncated JSON and normalises quotes.
pub fn repair_json(input: &str) -> String {
    let input = &input.replace('\'', "\"");
    let mut repaired = input.to_string();

    let mut open_braces = 0;
    let mut open_brackets = 0;
    let mut in_quote = false;
    let mut escaped = false;

    for c in repaired.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => in_quote = !in_quote,
            '{' if !in_quote => open_braces += 1,
            '}' if !in_quote => open_braces -= 1,
            '[' if !in_quote => open_brackets += 1,
            ']' if !in_quote => open_brackets -= 1,
            _ => {}
        }
    }

    if in_quote {
        repaired.push('"');
    }
    while open_brackets > 0 {
        repaired.push(']');
        open_brackets -= 1;
    }
    while open_braces > 0 {
        repaired.push('}');
        open_braces -= 1;
    }

    repaired
}
