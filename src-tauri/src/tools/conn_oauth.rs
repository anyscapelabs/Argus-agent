use serde_json::Value;

use super::connector::{num, opt, out, s};
use super::ToolMeta;

pub const META: &[ToolMeta] = &[
    ToolMeta {
        name: "gmail.search",
        desc: "search Gmail messages, newest first; query supports Gmail operators like is:unread from:x",
        args: "{\"query\":\"\",\"max\":10}",
        mutating: false,
    },
    ToolMeta {
        name: "gmail.read",
        desc: "read one Gmail message with body",
        args: "{\"id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "gmail.send",
        desc: "send an email from the connected Gmail account; in ask mode it renders as an editable draft card — put email drafts here, never in reply prose",
        args: "{\"to\":\"a@b.com\",\"subject\":\"...\",\"body\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "gcal.events",
        desc: "list upcoming Google Calendar events; timeMin is RFC3339, calendar_id defaults to primary",
        args: "{\"calendar_id\":\"\",\"time_min\":\"\",\"max\":15}",
        mutating: false,
    },
    ToolMeta {
        name: "gcal.create_event",
        desc: "create a Google Calendar event; start and end are RFC3339",
        args: "{\"calendar_id\":\"\",\"summary\":\"...\",\"start\":\"...\",\"end\":\"...\",\"description\":\"\"}",
        mutating: true,
    },
    ToolMeta {
        name: "gcal.delete_event",
        desc: "delete a Google Calendar event",
        args: "{\"calendar_id\":\"\",\"event_id\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "gdrive.search",
        desc: "search Google Drive files by name fragment",
        args: "{\"query\":\"\",\"max\":10}",
        mutating: false,
    },
    ToolMeta {
        name: "gdrive.read",
        desc: "read a Google file's text (docs, sheets or plain text via export)",
        args: "{\"file_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "gdrive.meta",
        desc: "read a Google Drive file's metadata",
        args: "{\"file_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "gdrive.create",
        desc: "create a file in Google Drive with content; mime defaults to text/plain",
        args: "{\"name\":\"...\",\"mime\":\"\",\"content\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "gdrive.delete",
        desc: "delete a Google Drive file",
        args: "{\"file_id\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "gdocs.read",
        desc: "read a Google Doc's content",
        args: "{\"doc_id\":\"...\"}",
        mutating: false,
    },
    ToolMeta {
        name: "gdocs.create",
        desc: "create an empty Google Doc",
        args: "{\"title\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "gdocs.append",
        desc: "append text to the end of a Google Doc",
        args: "{\"doc_id\":\"...\",\"text\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "gsheets.values",
        desc: "read a Google Sheets range, e.g. Sheet1!A1:C10",
        args: "{\"id\":\"...\",\"range\":\"Sheet1!A1:C10\"}",
        mutating: false,
    },
    ToolMeta {
        name: "gsheets.update",
        desc: "overwrite a Google Sheets range; values is a JSON array of rows",
        args: "{\"id\":\"...\",\"range\":\"Sheet1!A1\",\"values\":[[\"a\",\"b\"]]}",
        mutating: true,
    },
    ToolMeta {
        name: "gsheets.append",
        desc: "append rows to a Google Sheets range; values is a JSON array of rows",
        args: "{\"id\":\"...\",\"range\":\"Sheet1!A1\",\"values\":[[\"a\",\"b\"]]}",
        mutating: true,
    },
    ToolMeta {
        name: "gsheets.create",
        desc: "create a Google spreadsheet",
        args: "{\"title\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "github.repos",
        desc: "list your GitHub repositories",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "github.repo",
        desc: "read one GitHub repository by owner/name",
        args: "{\"full_name\":\"owner/repo\"}",
        mutating: false,
    },
    ToolMeta {
        name: "github.branches",
        desc: "list a GitHub repository's branches",
        args: "{\"full_name\":\"owner/repo\"}",
        mutating: false,
    },
    ToolMeta {
        name: "github.create_repo",
        desc: "create a GitHub repository",
        args: "{\"name\":\"...\",\"private\":false,\"description\":\"\"}",
        mutating: true,
    },
    ToolMeta {
        name: "github.issues",
        desc: "list a GitHub repo's issues; state open, closed or all",
        args: "{\"full_name\":\"owner/repo\",\"state\":\"open\"}",
        mutating: false,
    },
    ToolMeta {
        name: "github.issue",
        desc: "read one GitHub issue with comments",
        args: "{\"full_name\":\"owner/repo\",\"number\":1}",
        mutating: false,
    },
    ToolMeta {
        name: "github.create_issue",
        desc: "create a GitHub issue",
        args: "{\"full_name\":\"owner/repo\",\"title\":\"...\",\"body\":\"\"}",
        mutating: true,
    },
    ToolMeta {
        name: "github.comment",
        desc: "comment on a GitHub issue or pull request",
        args: "{\"full_name\":\"owner/repo\",\"number\":1,\"body\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "github.close_issue",
        desc: "close a GitHub issue",
        args: "{\"full_name\":\"owner/repo\",\"number\":1}",
        mutating: true,
    },
    ToolMeta {
        name: "github.prs",
        desc: "list a GitHub repo's pull requests; state open, closed or all",
        args: "{\"full_name\":\"owner/repo\",\"state\":\"open\"}",
        mutating: false,
    },
    ToolMeta {
        name: "github.pr",
        desc: "read one GitHub pull request",
        args: "{\"full_name\":\"owner/repo\",\"number\":1}",
        mutating: false,
    },
    ToolMeta {
        name: "github.create_pr",
        desc: "open a GitHub pull request from head branch into base branch",
        args: "{\"full_name\":\"owner/repo\",\"title\":\"...\",\"head\":\"branch\",\"base\":\"main\",\"body\":\"\"}",
        mutating: true,
    },
    ToolMeta {
        name: "github.merge_pr",
        desc: "squash-merge a GitHub pull request",
        args: "{\"full_name\":\"owner/repo\",\"number\":1}",
        mutating: true,
    },
    ToolMeta {
        name: "github.actions_runs",
        desc: "list recent GitHub Actions runs for a repo",
        args: "{\"full_name\":\"owner/repo\"}",
        mutating: false,
    },
    ToolMeta {
        name: "github.rerun_failed",
        desc: "re-run failed jobs of a GitHub Actions run",
        args: "{\"full_name\":\"owner/repo\",\"run_id\":123}",
        mutating: true,
    },
    ToolMeta {
        name: "outlook.mail",
        desc: "list recent Outlook mail",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "outlook.send",
        desc: "send an Outlook email; in ask mode it renders as an editable draft card — put email drafts here, never in reply prose",
        args: "{\"to\":\"a@b.com\",\"subject\":\"...\",\"body\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "outlook.events",
        desc: "list upcoming Outlook calendar events",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "outlook.create_event",
        desc: "create an Outlook calendar event; start and end are ISO datetimes",
        args: "{\"subject\":\"...\",\"start\":\"...\",\"end\":\"...\"}",
        mutating: true,
    },
    ToolMeta {
        name: "spotify.now_playing",
        desc: "the currently playing Spotify track",
        args: "{}",
        mutating: false,
    },
    ToolMeta {
        name: "spotify.control",
        desc: "control Spotify playback: play, pause, next or prev",
        args: "{\"action\":\"play\"}",
        mutating: true,
    },
];

fn int(args: &Value, key: &str) -> Result<i64, String> {
    args.get(key)
        .and_then(|v| v.as_i64())
        .ok_or(format!("missing {key}"))
}

fn rows(args: &Value, key: &str) -> Result<Value, String> {
    let v = args
        .get(key)
        .filter(|v| v.is_array())
        .cloned()
        .ok_or(format!("missing {key} as an array of rows"))?;

    Ok(v)
}

fn gcal_id<'a>(args: &'a Value) -> &'a str {
    let id = opt(args, "calendar_id");

    if id.trim().is_empty() {
        "primary"
    } else {
        id
    }
}

fn out_str(v: Result<String, String>) -> Result<String, String> {
    v
}

pub async fn exec(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "gmail.search" => out(crate::mcp::google::gmail::list_messages(
            opt(args, "query"),
            num(args, "max", 10),
        )
        .await),
        "gmail.read" => out(crate::mcp::google::gmail::get_message(s(args, "id")?).await),
        "gmail.send" => out(crate::mcp::google::gmail::send_message(
            s(args, "to")?,
            s(args, "subject")?,
            s(args, "body")?,
        )
        .await),
        "gcal.events" => out(crate::mcp::google::calendar::list_events(
            gcal_id(args),
            opt(args, "time_min"),
            num(args, "max", 15),
        )
        .await),
        "gcal.create_event" => out(crate::mcp::google::calendar::create_event(
            gcal_id(args),
            s(args, "summary")?,
            s(args, "start")?,
            s(args, "end")?,
            opt(args, "description"),
        )
        .await),
        "gcal.delete_event" => {
            crate::mcp::google::calendar::delete_event(gcal_id(args), s(args, "event_id")?).await?;
            Ok("deleted".into())
        }
        "gdrive.search" => out(crate::mcp::google::drive::list_files(
            opt(args, "query"),
            num(args, "max", 10),
        )
        .await),
        "gdrive.read" => out_str(crate::mcp::google::drive::export_text(s(args, "file_id")?).await),
        "gdrive.meta" => out(crate::mcp::google::drive::get_file(s(args, "file_id")?).await),
        "gdrive.create" => out(crate::mcp::google::drive::create_file(
            s(args, "name")?,
            opt(args, "mime"),
            s(args, "content")?,
        )
        .await),
        "gdrive.delete" => {
            crate::mcp::google::drive::delete_file(s(args, "file_id")?).await?;
            Ok("deleted".into())
        }
        "gdocs.read" => out(crate::mcp::google::docs::get_doc(s(args, "doc_id")?).await),
        "gdocs.create" => out(crate::mcp::google::docs::create_doc(s(args, "title")?).await),
        "gdocs.append" => {
            out(crate::mcp::google::docs::append_text(s(args, "doc_id")?, s(args, "text")?).await)
        }
        "gsheets.values" => {
            out(crate::mcp::google::sheets::get_values(s(args, "id")?, s(args, "range")?).await)
        }
        "gsheets.update" => out(crate::mcp::google::sheets::update_values(
            s(args, "id")?,
            s(args, "range")?,
            &rows(args, "values")?,
        )
        .await),
        "gsheets.append" => out(crate::mcp::google::sheets::append_values(
            s(args, "id")?,
            s(args, "range")?,
            &rows(args, "values")?,
        )
        .await),
        "gsheets.create" => out(crate::mcp::google::sheets::create_sheet(s(args, "title")?).await),
        "github.repos" => out(crate::mcp::github::repos::list_repos().await),
        "github.repo" => out(crate::mcp::github::repos::get_repo(s(args, "full_name")?).await),
        "github.branches" => {
            out(crate::mcp::github::repos::list_branches(s(args, "full_name")?).await)
        }
        "github.create_repo" => out(crate::mcp::github::repos::create_repo(
            s(args, "name")?,
            args.get("private")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            opt(args, "description"),
        )
        .await),
        "github.issues" => out(crate::mcp::github::issues::list_issues(
            s(args, "full_name")?,
            opt(args, "state"),
        )
        .await),
        "github.issue" => out(crate::mcp::github::issues::get_issue(
            s(args, "full_name")?,
            int(args, "number")?,
        )
        .await),
        "github.create_issue" => out(crate::mcp::github::issues::create_issue(
            s(args, "full_name")?,
            s(args, "title")?,
            opt(args, "body"),
        )
        .await),
        "github.comment" => out(crate::mcp::github::issues::comment_issue(
            s(args, "full_name")?,
            int(args, "number")?,
            s(args, "body")?,
        )
        .await),
        "github.close_issue" => out(crate::mcp::github::issues::close_issue(
            s(args, "full_name")?,
            int(args, "number")?,
        )
        .await),
        "github.prs" => {
            out(crate::mcp::github::prs::list_prs(s(args, "full_name")?, opt(args, "state")).await)
        }
        "github.pr" => {
            out(crate::mcp::github::prs::get_pr(s(args, "full_name")?, int(args, "number")?).await)
        }
        "github.create_pr" => out(crate::mcp::github::prs::create_pr(
            s(args, "full_name")?,
            s(args, "title")?,
            s(args, "head")?,
            s(args, "base")?,
            opt(args, "body"),
        )
        .await),
        "github.merge_pr" => out(crate::mcp::github::prs::merge_pr(
            s(args, "full_name")?,
            int(args, "number")?,
        )
        .await),
        "github.actions_runs" => {
            out(crate::mcp::github::actions::list_runs(s(args, "full_name")?).await)
        }
        "github.rerun_failed" => {
            crate::mcp::github::actions::rerun_failed(s(args, "full_name")?, int(args, "run_id")?)
                .await?;
            Ok("rerun started".into())
        }
        "outlook.mail" => out(crate::mcp::outlook::messages().await),
        "outlook.send" => {
            crate::mcp::outlook::send(s(args, "to")?, s(args, "subject")?, s(args, "body")?)
                .await?;
            Ok("sent".into())
        }
        "outlook.events" => out(crate::mcp::outlook::events().await),
        "outlook.create_event" => out(crate::mcp::outlook::create_event(
            s(args, "subject")?,
            s(args, "start")?,
            s(args, "end")?,
        )
        .await),
        "spotify.now_playing" => out(crate::mcp::spotify::now_playing().await),
        "spotify.control" => {
            crate::mcp::spotify::control(s(args, "action")?).await?;
            Ok(format!("{} done", opt(args, "action")))
        }
        _ => Err("unknown connector tool".into()),
    }
}
