use crate::protocol::{ErrorCode, Failure, Request};
use bravebot_agent::bots::{self, Refused};
use serde_json::{Value, json};
use std::path::PathBuf;

pub fn list(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let listing = bots::list(&data_folder);
    Ok(json!({
        "bots": listing.rows.iter().map(bots::Row::to_value).collect::<Vec<_>>(),
        "dropped": listing.dropped.iter().map(|each| json!({
            "index": each.index,
            "slug": each.slug,
            "reason": each.reason.to_string(),
        })).collect::<Vec<_>>(),
        "unreadable": listing.unreadable.map(|problem| json!({
            "path": problem.path.display().to_string(),
            "reason": problem.reason,
        })),
    }))
}

pub fn replace(request: &Request) -> Result<Value, Failure> {
    let data_folder = data_folder(request)?;
    let Some(entries) = request.param("bots").as_array() else {
        return Err(Failure::bad_request("`bots` must be an array"));
    };
    match bots::replace(&data_folder, entries) {
        Ok(rows) => Ok(json!({ "bots": rows.iter().map(bots::Row::to_value).collect::<Vec<_>>() })),
        Err(refused) => Err(failure(&refused)),
    }
}

fn data_folder(request: &Request) -> Result<PathBuf, Failure> {
    let named = request.string("dataFolder")?;
    if !bots::is_absolute_path(&named) {
        return Err(Failure::bad_request(
            "`dataFolder` must be an absolute path",
        ));
    }
    Ok(PathBuf::from(named))
}

fn failure(refused: &Refused) -> Failure {
    let code = match refused {
        Refused::Ceiling | Refused::Entry { .. } => ErrorCode::BadRequest,
        _ => ErrorCode::NoHome,
    };
    Failure::new(code, refused.to_string())
}
