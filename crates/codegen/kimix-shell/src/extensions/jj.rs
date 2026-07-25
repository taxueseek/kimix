//! Jujutsu extension handlers — delegates to [`kimix_workspace::session::jj`].
use agent_client_protocol as acp;

use super::{Empty, ExtResult, to_ext_response, to_ext_response_partial};
use kimix_workspace::session::git::{CommitData, StageData};
use kimix_workspace::session::jj;

/// Handle a `Kimix/git/*` method for a jj-colocated repo.
///
/// Returns `Some(result)` if handled, `None` to fall through to git.
pub async fn try_handle(
    method: &str,
    git_root: &std::path::Path,
    raw_params: &serde_json::value::RawValue,
) -> Option<ExtResult> {
    match method {
        "kimix/git/status" => Some(to_ext_response(jj::status(git_root).await)),
        "kimix/git/info" => Some(to_ext_response(jj::info(git_root).await)),
        // git HEAD points at `@-` in a colocated repo; route to jj so we report
        // the working-copy commit (`@`), consistent with `status`/`info`.
        "kimix/git/current_commit" => Some(to_ext_response(jj::current_commit(git_root).await)),
        "kimix/git/branches" => Some(to_ext_response(jj::list_bookmarks(git_root).await)),

        // jj has no staging area — stage/unstage are no-ops
        "kimix/git/stage" => Some(to_ext_response(Ok(StageData { paths: Vec::new() }))),
        "kimix/git/stage/content" | "kimix/git/unstage" => Some(to_ext_response(Ok(Empty {}))),

        "kimix/git/discard" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Req {
                #[serde(default)]
                paths: Option<Vec<String>>,
            }
            let req: Req = serde_json::from_str(raw_params.get()).ok()?;
            Some(to_ext_response(
                jj::discard(git_root, req.paths).await.map(|_| Empty {}),
            ))
        }

        "kimix/git/commit" => {
            #[derive(serde::Deserialize)]
            struct Req {
                message: String,
            }
            let req: Req = serde_json::from_str(raw_params.get()).ok()?;
            let result = jj::commit(git_root, &req.message).await;
            Some(match result {
                Ok(r) => to_ext_response_partial(Ok(r.data), r.warning),
                Err(e) => to_ext_response(Err::<CommitData, _>(e)),
            })
        }

        // Operations that don't apply to jj
        "kimix/git/checkout" => Some(Err(acp::Error::invalid_params()
            .data("checkout is not supported in jj repos; use `jj new` or `jj edit`"))),
        "kimix/git/stash" => Some(Err(acp::Error::invalid_params()
            .data("stash is not supported in jj repos; changes are always committed"))),

        // Everything else (diffs, files, serialize_changes) falls through to git
        _ => None,
    }
}
