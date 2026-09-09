use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

const MAX_RESULTS: usize = 8;
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(20))
            .build()
            .expect("http client init")
    })
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing {key}"))
}

pub async fn search(args: &Value) -> Result<String, String> {
    let query = arg_str(args, "query")?;

    let resp = client()
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .send()
        .await
        .map_err(|e| format!("search request failed: {e}"))?;

    let status = resp.status();
    let html = resp.text().await.map_err(|e| format!("search read failed: {e}"))?;

    if !status.is_success() {
        return Err(format!("search engine answered {status} — try again later"));
    }

    let results = parse_results(&html);
    if results.is_empty() {
        return Err(
            "search returned no results (the engine may be rate-limiting automated queries — try again in a moment)".into(),
        );
    }

    let mut out = String::new();
    for (i, (title, url, snippet)) in results.iter().take(MAX_RESULTS).enumerate() {
        out.push_str(&format!("{}. {title}\n   {url}\n   {snippet}\n", i + 1));
    }

    Ok(out)
}

struct Hit {
    title: String,
    url: String,
    snippet: String,
}

fn parse_results(html: &str) -> Vec<(String, String, String)> {
    let anchor = Regex::new(r#"<a\s([^>]*result__a[^>]*)>(.*?)</a>"#).expect("anchor regex");
    let snippet = Regex::new(r#"<a\s[^>]*result__snippet[^>]*>(.*?)</a>"#).expect("snippet regex");
    let href = Regex::new(r#"href="([^"]*)""#).expect("href regex");
    let tag = Regex::new(r#"<[^>]*>"#).expect("tag regex");

    let mut hits: Vec<Hit> = vec![];

    for cap in anchor.captures_iter(html) {
        let attrs = &cap[1];
        let url = match href.captures(attrs) {
            Some(h) => clean_url(&h[1]),
            None => continue,
        };

        let title = strip(&tag.replace_all(&cap[2], ""));
        if title.is_empty() || url.is_empty() {
            continue;
        }

        hits.push(Hit {
            title,
            url,
            snippet: String::new(),
        });
    }

    let mut snips = snippet
        .captures_iter(html)
        .map(|c| strip(&tag.replace_all(&c[1], "")))
        .collect::<Vec<_>>();

    snips.truncate(hits.len());
    for (h, s) in hits.iter_mut().zip(snips) {
        h.snippet = s;
    }

    hits.into_iter().map(|h| (h.title, h.url, h.snippet)).collect()
}

fn clean_url(href: &str) -> String {
    let mut h = href.trim().to_string();

    if let Some(pos) = h.find("uddg=") {
        let rest = &h[pos + 5..];
        let end = rest.find('&').unwrap_or(rest.len());
        return percent_decode(&rest[..end]);
    }

    if h.starts_with("//") {
        h = format!("https:{h}");
    }

    h
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn strip(s: &str) -> String {
    let decoded = decode_entities(s);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(s: &str) -> String {
    let table = [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#x27;", "'"),
        ("&#39;", "'"),
        ("&nbsp;", " "),
    ];

    let mut out = s.to_string();
    for (raw, ch) in table {
        out = out.replace(raw, ch);
    }

    out
}

pub async fn read(args: &Value) -> Result<String, String> {
    let url = arg_str(args, "url")?;

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("url must start with http:// or https://".into());
    }

    let via_jina = client().get(format!("https://r.jina.ai/{url}")).send().await;

    if let Ok(resp) = via_jina {
        if resp.status().is_success() {
            let body = resp.text().await.map_err(|e| format!("page read failed: {e}"))?;
            let body = body.trim().to_string();
            if !body.is_empty() {
                return Ok(crate::tools::clip(body));
            }
        }
    }

    let resp = client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("page fetch failed: {e}"))?;

    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let status = resp.status();
    let body = resp.text().await.map_err(|e| format!("page read failed: {e}"))?;

    if !status.is_success() {
        return Err(format!("page answered {status}"));
    }

    if ct.contains("html") {
        Ok(crate::tools::clip(html_to_text(&body)))
    } else {
        Ok(crate::tools::clip(body))
    }
}

fn html_to_text(html: &str) -> String {
    let script = Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("script regex");
    let style = Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("style regex");
    let tag = Regex::new(r"<[^>]*>").expect("tag regex");
    let blank = Regex::new(r"\n{3,}").expect("blank regex");

    let no_script = script.replace_all(html, "");
    let no_css = style.replace_all(&no_script, "");
    let no_tags = tag.replace_all(&no_css, "\n");
    let text = decode_entities(&no_tags);

    let lines = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    blank.replace_all(&lines, "\n\n").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ddg_html() {
        let html = r##"
          <div class="result">
          <a rel="nofollow" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&rut=abc" class="result__a">Ex<b>ample</b> One</a>
          <a class="result__snippet" href="#">The first &amp; best result</a>
          </div>
          <a rel="nofollow" href="https://direct.example.com/b" class="result__a">Direct Two</a>
          <a class="result__snippet" href="#">second snippet</a>
        "##;

        let out = parse_results(html);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "Example One");
        assert_eq!(out[0].1, "https://example.com/a");
        assert_eq!(out[0].2, "The first & best result");
        assert_eq!(out[1].1, "https://direct.example.com/b");
    }

    #[test]
    fn html_to_text_drops_scripts() {
        let t = html_to_text(
            "<html><script>var x=1;</script><style>a{}</style><body><p>hello world</p></body></html>",
        );
        assert_eq!(t, "hello world");
    }
}
