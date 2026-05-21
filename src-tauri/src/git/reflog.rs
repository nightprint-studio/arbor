use serde::Serialize;
use crate::error::Result;
use crate::git::repo::GitRepo;

#[derive(Debug, Clone, Serialize)]
pub struct ReflogEntry {
    pub index:           usize,
    pub id:              String,
    pub id_old:          String,
    pub message:         String,
    pub committer_name:  String,
    pub committer_time:  i64,
}

pub fn get_reflog(repo: &GitRepo, limit: Option<usize>) -> Result<Vec<ReflogEntry>> {
    let inner   = repo.inner();
    let reflog  = inner.reflog("HEAD")?;
    let limit   = limit.unwrap_or(200);

    let entries = reflog
        .iter()
        .enumerate()
        .take(limit)
        .map(|(index, entry)| {
            let id     = entry.id_new().to_string();
            let id_old = entry.id_old().to_string();
            let message = entry.message().unwrap_or("").to_string();
            let sig = entry.committer();
            let committer_name = sig.name().unwrap_or("").to_string();
            let committer_time = sig.when().seconds();

            ReflogEntry { index, id, id_old, message, committer_name, committer_time }
        })
        .collect();

    Ok(entries)
}
