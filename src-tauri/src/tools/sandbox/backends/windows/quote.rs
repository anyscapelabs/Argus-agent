// 2n+1 backslashes before a quote. Get this wrong and a path silently
// becomes extra arguments.
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }

    let mut out = String::with_capacity(arg.len() + 2);

    out.push('"');

    let mut slashes = 0usize;

    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                // 2n+1 backslashes: n literal, one to escape the quote itself.
                for _ in 0..(2 * slashes + 1) {
                    out.push('\\');
                }

                slashes = 0;
                out.push('"');
            }
            _ => {
                for _ in 0..slashes {
                    out.push('\\');
                }

                slashes = 0;
                out.push(c);
            }
        }
    }

    // Backslashes before the closing quote must be doubled, or they escape it
    // and the argument never terminates.
    for _ in 0..slashes {
        out.push('\\');
    }

    for _ in 0..slashes {
        out.push('\\');
    }

    out.push('"');

    out
}

pub fn command_line(argv: &[String]) -> String {
    argv.iter()
        .map(|a| quote_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}
