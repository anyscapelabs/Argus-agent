// Turning the model's JSON arguments into the shapes the writers take. Lenient
// on purpose: the model may send one string where a table was asked for.
pub(super) fn parse_rows(v: &serde_json::Value) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = vec![];
    let arr = match v.as_array() {
        Some(a) => a,
        None => return out,
    };
    for row in arr {
        match row {
            serde_json::Value::Array(cells) => {
                out.push(
                    cells
                        .iter()
                        .map(|c| match c {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => String::new(),
                        })
                        .collect(),
                );
            }
            serde_json::Value::String(s) => out.push(vec![s.clone()]),
            _ => {}
        }
    }
    out
}

pub(super) fn parse_slides(v: &serde_json::Value) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    let arr = match v.as_array() {
        Some(a) => a,
        None => return out,
    };
    for s in arr {
        let title = s
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let bullets: Vec<String> = s
            .get("bullets")
            .and_then(|b| b.as_array())
            .map(|a| {
                a.iter()
                    .map(|c| match c {
                        serde_json::Value::String(x) => x.clone(),
                        _ => String::new(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push((title, bullets));
    }
    out
}
