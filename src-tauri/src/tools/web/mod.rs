use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

const MAX_RESULTS: usize = 8;
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

const SEARXNG_POOL: &[&str] = &[
    "https://searx.be/search",
    "https://searx.tiekoetter.com/search",
    "https://priv.au/search",
];

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();

    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing {key}"))
}

#[derive(Clone, Debug)]
pub struct WebConfig {
    pub searxng_pool: Vec<String>,
    pub ddg_url: String,
    pub jina_base: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            searxng_pool: SEARXNG_POOL.iter().map(|s| s.to_string()).collect(),
            ddg_url: "https://html.duckduckgo.com/html/".into(),
            jina_base: "https://r.jina.ai".into(),
        }
    }
}

impl WebConfig {
    pub fn from_env() -> Self {
        let base = Self::default();
        let pool = std::env::var("ARGUS_SEARXNG_POOL")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or(base.searxng_pool);
        let non_empty = |key: &str, fallback: String| {
            std::env::var(key)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or(fallback)
        };
        Self {
            searxng_pool: pool,
            ddg_url: non_empty("ARGUS_DDG_URL", base.ddg_url),
            jina_base: non_empty("ARGUS_JINA_URL", base.jina_base),
        }
    }
}

pub async fn search(args: &Value) -> Result<String, String> {
    search_with(args, &WebConfig::from_env()).await
}

pub async fn search_with(args: &Value, cfg: &WebConfig) -> Result<String, String> {
    let query = arg_str(args, "query")?;
    let mut failures: Vec<String> = vec![];
    let mut empty_from: Vec<&str> = vec![];

    match searxng_search(&cfg.searxng_pool, query).await {
        Ok(hits) if !hits.is_empty() => return Ok(format_hits(&hits, "searxng")),
        Ok(_) => empty_from.push("searxng"),
        Err(e) => failures.push(format!("searxng: {e}")),
    }

    match duck_fetch(&cfg.ddg_url, query).await {
        Ok(hits) if !hits.is_empty() => return Ok(format_hits(&hits, "duckduckgo")),
        Ok(_) => empty_from.push("duckduckgo"),
        Err(e) => failures.push(format!("duckduckgo: {e}")),
    }

    if !empty_from.is_empty() {
        let mut out = format!(
            "No results found for \"{query}\" ({} returned none). Try rewording the query.",
            empty_from.join(", ")
        );
        if !failures.is_empty() {
            out.push_str(&format!(" Note: {}.", failures.join("; ")));
        }
        return Ok(out);
    }

    Err(format!(
        "All search providers failed for \"{query}\": {}",
        failures.join("; ")
    ))
}

fn domain_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default()
}

fn format_hits(hits: &[(String, String, String)], provider: &str) -> String {
    let mut out = String::new();
    for (i, (title, url, snippet)) in hits.iter().take(MAX_RESULTS).enumerate() {
        let dom = domain_of(url);
        let head = if dom.is_empty() {
            title.clone()
        } else {
            format!("{title} — {dom}")
        };
        let snip = if snippet.trim().is_empty() {
            "(no snippet)".to_string()
        } else {
            snippet.clone()
        };
        out.push_str(&format!("{}. {head}\n   {url}\n   {snip}\n", i + 1));
    }
    out.push_str(&format!("Provider: {provider}\n"));
    out
}

#[derive(Deserialize, Debug)]
struct SearxngHit {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
}

#[derive(Deserialize, Debug)]
struct SearxngResp {
    #[serde(default)]
    results: Vec<SearxngHit>,
}

async fn searxng_search(
    pool: &[String],
    query: &str,
) -> Result<Vec<(String, String, String)>, String> {
    if pool.is_empty() {
        return Err("no instances configured".into());
    }

    let mut last = String::new();
    for base in pool {
        match searxng_fetch(base, query).await {
            Ok(hits) => return Ok(hits),
            Err(e) => last = format!("{base}: {e}"),
        }
    }

    Err(format!("searxng pool failed ({last})"))
}

async fn searxng_fetch(base: &str, query: &str) -> Result<Vec<(String, String, String)>, String> {
    let resp = client()
        .get(base)
        .query(&[("q", query), ("format", "json"), ("categories", "general")])
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|err| format!("request failed: {err}"))?;

    if !resp.status().is_success() {
        return Err(format!("answered {}", resp.status()));
    }

    let body: SearxngResp = resp
        .json()
        .await
        .map_err(|_| "invalid response".to_string())?;
    Ok(body
        .results
        .into_iter()
        .filter_map(|r| {
            let title = strip(&r.title);
            let url = r.url.trim().to_string();

            (!title.is_empty() && !url.is_empty()).then(|| (title, url, strip(&r.content)))
        })
        .take(MAX_RESULTS)
        .collect())
}

async fn duck_fetch(ddg_url: &str, query: &str) -> Result<Vec<(String, String, String)>, String> {
    let resp = client()
        .get(ddg_url)
        .query(&[("q", query)])
        .send()
        .await
        .map_err(|err| format!("search request failed: {err}"))?;

    let status = resp.status();
    let html = resp
        .text()
        .await
        .map_err(|err| format!("search read failed: {err}"))?;

    if !status.is_success() {
        return Err(format!("search engine answered {status} — try again later"));
    }

    let lower = html.to_lowercase();

    if lower.contains("unusual traffic") || lower.contains("anomaly detected") {
        return Err(
            "search engine served a bot-check page — wait a moment and retry, \
             or reword the query"
                .into(),
        );
    }

    Ok(parse_results(&html))
}

struct Hit {
    title: String,
    url: String,
    snippet: String,
}

pub fn parse_results(html: &str) -> Vec<(String, String, String)> {
    let Ok(anchor) = Regex::new(r#"<a\s([^>]*result__a[^>]*)>(.*?)</a>"#) else {
        return vec![];
    };

    let Ok(snippet) = Regex::new(r#"<a\s[^>]*result__snippet[^>]*>(.*?)</a>"#) else {
        return vec![];
    };

    let Ok(href) = Regex::new(r#"href="([^"]*)""#) else {
        return vec![];
    };

    let Ok(tag) = Regex::new(r#"<[^>]*>"#) else {
        return vec![];
    };

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

    hits.into_iter()
        .map(|h| (h.title, h.url, h.snippet))
        .collect()
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
    read_with(args, &WebConfig::from_env()).await
}

pub async fn read_with(args: &Value, cfg: &WebConfig) -> Result<String, String> {
    let url = arg_str(args, "url")?;

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("url must start with http:// or https://".into());
    }

    let host_ok = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| !h.is_empty()))
        .unwrap_or(false);
    if !host_ok {
        return Err(format!("invalid URL: {url}"));
    }

    crate::tools::browser::url_guard(url)?;

    if let Some(text) = jina_read(&cfg.jina_base, url).await {
        return Ok(format!("Source: {url}\n\n{text}"));
    }

    Ok(format!("Source: {url}\n\n{}", direct_read(url).await?))
}

async fn jina_read(jina_base: &str, url: &str) -> Option<String> {
    let base = jina_base.trim_end_matches('/');
    let resp = client().get(format!("{base}/{url}")).send().await.ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body = resp.text().await.ok()?.trim().to_string();
    if body.is_empty() {
        return None;
    }

    let text = crate::tools::page_text(&body);
    if text.trim().is_empty() || bot_wall(&text).is_some() {
        return None;
    }

    Some(text)
}

async fn direct_read(url: &str) -> Result<String, String> {
    let resp = client()
        .get(url)
        .send()
        .await
        .map_err(|err| format!("page fetch failed: {err}"))?;

    let status = resp.status().as_u16();
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = resp
        .text()
        .await
        .map_err(|err| format!("page read failed: {err}"))?;

    if status == 404 {
        return Err(format!("page not found (404): {url}"));
    }

    if status == 401 || status == 403 {
        return Err(format!(
            "page requires authentication or forbids automated access ({status}): {url}"
        ));
    }

    if status == 429 {
        return Err(format!("page rate limited (429): {url}"));
    }

    if !(200..300).contains(&status) {
        if status >= 500 {
            return Err(format!("page fetch failed: server error {status}: {url}"));
        }
        return Err(format!("page answered {status}: {url}"));
    }

    let text = if ct.contains("html") {
        crate::tools::page_text(&html_to_text(&body))
    } else if ct.is_empty()
        || ct.starts_with("text/")
        || ct.contains("json")
        || ct.contains("xml")
        || ct.contains("javascript")
    {
        crate::tools::page_text(&body)
    } else {
        return Err(format!("unsupported content type ({ct}): {url}"));
    };

    if let Some(err) = bot_wall(&text) {
        return Err(err);
    }

    if text.trim().is_empty() {
        return Err(format!("page had no readable text: {url}"));
    }

    Ok(text)
}

const WALL_SIGNS: &[&str] = &[
    "confirm this search was made by a human",
    "complete the following challenge",
    "unusual traffic",
    "captcha",
    "are you a robot",
    "verify you are a human",
    "attention required",
    "access denied",
    "not redirected within a few seconds",
    "enable javascript",
    "javascript is required",
];

const WALL_MAX: usize = 2000;

pub fn bot_wall(text: &str) -> Option<String> {
    if text.len() >= WALL_MAX {
        return None;
    }

    let lower = text.to_lowercase();

    WALL_SIGNS
        .iter()
        .find(|s| lower.contains(*s))
        .map(|s| format!("page served a bot check ({s}) instead of content — try another source, not another query on the same engine"))
}

pub fn html_to_text(html: &str) -> String {
    let Ok(script) = Regex::new(r"(?is)<script[^>]*>.*?</script>") else {
        return html.to_string();
    };

    let Ok(style) = Regex::new(r"(?is)<style[^>]*>.*?</style>") else {
        return html.to_string();
    };

    let Ok(tag) = Regex::new(r"<[^>]*>") else {
        return html.to_string();
    };

    let Ok(blank) = Regex::new(r"\n{3,}") else {
        return html.to_string();
    };

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
