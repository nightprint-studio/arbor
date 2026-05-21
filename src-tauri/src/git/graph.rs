use std::collections::HashMap;

use git2::{Oid, Repository, Sort};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

// ---------------------------------------------------------------------------
// Public types (serialized to frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<CommitNode>,
    pub edges: Vec<GraphEdge>,
    pub lane_count: usize,
    pub total_commits: usize,
    pub offset: usize,
    /// Stashes anchored by their first-parent commit OID. Populated by
    /// `get_graph`; `load_graph*` leave this empty since they don't need
    /// mutable repo access.
    #[serde(default)]
    pub stashes: Vec<crate::git::stash::StashRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitNode {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub body: Option<String>,
    pub author: AuthorInfo,
    pub committer: AuthorInfo,
    pub timestamp: i64,
    /// Row index within the *loaded* page (0 = top).
    pub row: usize,
    /// Lane (column) in the graph.
    pub lane: usize,
    /// Index into the 10-color palette (lane % 10).
    pub color_index: usize,
    /// Labels (branches / tags / HEAD) pointing to this commit.
    pub refs: Vec<RefLabel>,
    pub is_merge: bool,
    pub is_head: bool,
    /// Which lanes carry edges through this row (for drawing vertical pass-through lines).
    pub active_lanes: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from_row: usize,
    pub from_lane: usize,
    pub to_row: usize,
    pub to_lane: usize,
    pub color_index: usize,
    pub edge_type: EdgeType,
    /// Set only for "trailing" edges whose parent commit lies outside the
    /// current page.  The frontend uses this OID to repair the edge's to_row
    /// once the parent's page is loaded, giving seamless lane continuity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_parent_oid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Straight,
    /// Fork-point: pi=0 edge crosses to a lower lane. Vertical-first → ⌟ bottom-to-top.
    ForkLeft,
    /// Fork-point: pi=0 edge crosses to a higher lane. Vertical-first → ⌞ bottom-to-top.
    ForkRight,
    /// Merge-parent: pi>0 edge goes to a lower lane. Horizontal-first → ┌ bottom-to-top.
    MergeLeft,
    /// Merge-parent: pi>0 edge goes to a higher lane. Horizontal-first → ┐ bottom-to-top.
    MergeRight,
    /// Dashed ghost edge indicating a squash merge.
    SquashMerge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefLabel {
    pub name: String,
    pub ref_type: RefType,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefType {
    LocalBranch,
    RemoteBranch,
    Tag,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Remove remote-tracking refs whose branch name (after stripping the remote
/// prefix, e.g. "origin/develop" → "develop") is already covered by a local
/// branch ref on the same commit.  When local and remote point at the same
/// commit the remote label is redundant; when they diverge they live on
/// different commits, so there is no conflict to worry about.
fn dedupe_refs(refs: &mut Vec<RefLabel>) {
    use std::collections::HashSet;
    let local_names: HashSet<String> = refs
        .iter()
        .filter(|r| r.ref_type == RefType::LocalBranch)
        .map(|r| r.name.clone())
        .collect();
    if local_names.is_empty() {
        return;
    }
    refs.retain(|r| {
        if r.ref_type == RefType::RemoteBranch {
            let branch = r.name.splitn(2, '/').nth(1).unwrap_or(r.name.as_str());
            !local_names.contains(branch)
        } else {
            true
        }
    });
}

// ---------------------------------------------------------------------------
// Internal lane tracking
// ---------------------------------------------------------------------------

type Lane = Option<String>; // None = free slot, Some(oid) = lane waiting for that commit

struct Lanes(Vec<Lane>);

impl Lanes {
    fn new() -> Self { Self(Vec::new()) }

    /// Index of the lane already waiting for `oid`, if any.
    fn find(&self, oid: &str) -> Option<usize> {
        self.0.iter().position(|l| l.as_deref() == Some(oid))
    }

    /// Take the first free lane (set it) and return its index.
    fn alloc(&mut self, oid: &str) -> usize {
        if let Some(idx) = self.0.iter().position(|l| l.is_none()) {
            self.0[idx] = Some(oid.to_string());
            idx
        } else {
            self.0.push(Some(oid.to_string()));
            self.0.len() - 1
        }
    }

    /// Force-write `oid` into slot `idx`, relocating it from any other slot it
    /// currently occupies.  Used only for explicit reclaim paths where the
    /// caller KNOWS they want to move the OID (e.g. reclaiming develop from a
    /// higher lane down to lane 0).
    fn set_force(&mut self, idx: usize, oid: &str) {
        // Remove any stale reservation for this OID in a different slot first,
        // to avoid the same OID appearing in two slots simultaneously.
        if let Some(old) = self.find(oid) {
            if old != idx {
                self.0[old] = None;
            }
        }
        // Extend the vec if the slot doesn't exist yet.
        while self.0.len() <= idx {
            self.0.push(None);
        }
        self.0[idx] = Some(oid.to_string());
    }

    /// Reserve slot `idx` for `oid` ONLY if the OID is not already placed
    /// elsewhere AND the target slot is either free or already holds this OID.
    /// Returns the lane the OID actually occupies after the call.
    ///
    /// This is the safe default — prevents "OID theft" where reserving a new
    /// slot silently evicts an earlier reservation, which was causing
    /// sibling feature branches to collapse onto the same lane.
    fn try_set(&mut self, idx: usize, oid: &str) -> usize {
        // If the OID already has a lane, keep it there.
        if let Some(existing) = self.find(oid) {
            return existing;
        }
        // Extend the vec if needed.
        while self.0.len() <= idx {
            self.0.push(None);
        }
        // If the target slot is free, take it.
        if self.0[idx].is_none() {
            self.0[idx] = Some(oid.to_string());
            return idx;
        }
        // Target slot is occupied by a different OID — fall back to alloc.
        // For non-primary placement semantics we use alloc_secondary so slot 0
        // remains reserved for the primary chain.
        self.alloc_secondary(oid)
    }

    fn free(&mut self, idx: usize) {
        if idx < self.0.len() {
            self.0[idx] = None;
        }
    }

    #[allow(dead_code)]
    fn assign_or_find(&mut self, oid: &str) -> usize {
        self.find(oid).unwrap_or_else(|| self.alloc(oid))
    }

    /// Like `alloc` but never uses slot 0.  Used for non-main-branch commits so
    /// that slot 0 stays reserved for the first-parent chain.
    fn alloc_secondary(&mut self, oid: &str) -> usize {
        // Ensure slot 0 exists as a placeholder so we never accidentally return it.
        if self.0.is_empty() {
            self.0.push(None);
        }
        if let Some(idx) = self.0.iter().enumerate().skip(1).find(|(_, l)| l.is_none()).map(|(i, _)| i) {
            self.0[idx] = Some(oid.to_string());
            idx
        } else {
            self.0.push(Some(oid.to_string()));
            self.0.len() - 1 // always >= 1 because of the guard above
        }
    }

    fn active_indices(&self) -> Vec<usize> {
        self.0
            .iter()
            .enumerate()
            .filter_map(|(i, l)| if l.is_some() { Some(i) } else { None })
            .collect()
    }

    #[allow(dead_code)]
    fn max_active(&self) -> usize {
        self.0.iter().enumerate().filter(|(_, l)| l.is_some()).map(|(i, _)| i).max().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns true if the commit (identified by OID) touches `file_path`.
/// Uses a pathspec-filtered diff against the first parent for efficiency.
fn commit_touches_file(repo: &Repository, oid: Oid, file_path: &str) -> bool {
    let commit = match repo.find_commit(oid) { Ok(c) => c, Err(_) => return false };
    let tree   = match commit.tree()          { Ok(t) => t, Err(_) => return false };

    let mut opts = git2::DiffOptions::new();
    opts.pathspec(file_path);

    if commit.parent_count() == 0 {
        // Initial commit — does the file exist in the tree?
        return repo
            .diff_tree_to_tree(None, Some(&tree), Some(&mut opts))
            .map(|d| d.deltas().count() > 0)
            .unwrap_or(false);
    }

    if let Ok(parent) = commit.parent(0) {
        if let Ok(parent_tree) = parent.tree() {
            return repo
                .diff_tree_to_tree(Some(&parent_tree), Some(&tree), Some(&mut opts))
                .map(|d| d.deltas().count() > 0)
                .unwrap_or(false);
        }
    }
    false
}

/// Load a paginated slice of the commit graph.
///
/// * `offset`      — how many commits to skip from the most recent
/// * `limit`       — max commits to return (e.g. 500)
/// * `path_filter` — when `Some(path)`, only include commits that touched that path
fn load_graph_inner(repo: &Repository, offset: usize, limit: usize, path_filter: Option<&str>) -> Result<GraphData> {
    // --- collect refs (oid → labels) ---
    let mut ref_map: HashMap<String, Vec<RefLabel>> = HashMap::new();
    let head_target = repo.head().ok().and_then(|h| h.target()).map(|id| id.to_string());

    for reference in repo.references()? {
        let reference = reference?;
        let resolved = match reference.resolve() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let target_oid = match resolved.target() {
            Some(id) => id.to_string(),
            None => continue,
        };
        let full_name = match reference.name() {
            Some(n) => n,
            None => continue,
        };

        let (short_name, ref_type) = if let Some(s) = full_name.strip_prefix("refs/heads/") {
            (s.to_string(), RefType::LocalBranch)
        } else if let Some(s) = full_name.strip_prefix("refs/remotes/") {
            (s.to_string(), RefType::RemoteBranch)
        } else if let Some(s) = full_name.strip_prefix("refs/tags/") {
            (s.to_string(), RefType::Tag)
        } else {
            continue;
        };

        let is_current = head_target.as_deref() == Some(&target_oid);
        ref_map
            .entry(target_oid)
            .or_default()
            .push(RefLabel { name: short_name, ref_type, is_current });
    }

    // --- topological walk ---
    let mut revwalk = repo.revwalk()?;
    revwalk.push_glob("refs/heads/*")?;
    revwalk.push_glob("refs/tags/*")?;
    revwalk.push_glob("refs/remotes/*")?;
    // also push HEAD in case it's detached
    if let Ok(head) = repo.head() {
        if let Some(id) = head.target() {
            let _ = revwalk.push(id);
        }
    }
    // Sort::TOPOLOGICAL | Sort::TIME: topological order is maintained (parents
    // never appear before their children), but among commits with no ordering
    // dependency (e.g. tips of unrelated branches), the one with the more
    // recent timestamp is preferred.  This prevents old remote branch tips
    // from floating to the top of the graph even though they haven't been
    // touched in months — which is the standard behavior of GitKraken and
    // `git log --graph --date-order`.
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    let all_oids: Vec<Oid> = if let Some(path) = path_filter {
        revwalk.filter_map(|r| r.ok())
            .filter(|&oid| commit_touches_file(repo, oid, path))
            .collect()
    } else {
        revwalk.filter_map(|r| r.ok()).collect()
    };
    let total = all_oids.len();

    let page: Vec<Oid> = all_oids.into_iter().skip(offset).take(limit).collect();

    // --- pre-compute first-parent chain ---
    //
    // We walk the "primary branch" first-parent chain and collect every OID that
    // falls within the current page.  These commits are assigned to lane 0 so the
    // main development line always appears as the leftmost straight line.  All other
    // commits use alloc_secondary() which skips slot 0.
    //
    // IMPORTANT: we prefer a well-known primary branch (develop > main > master >
    // trunk) over HEAD.  This keeps develop on lane 0 even when the user has a
    // feature branch checked out.  If none of those names exist we fall back to HEAD.
    let page_oid_set: std::collections::HashSet<String> =
        page.iter().map(|o| o.to_string()).collect();

    // Collect ALL tips of the primary branch — local AND every remote-tracking
    // variant.  When the local branch is behind origin (e.g. origin/develop is
    // ahead of the local develop), commits on origin/develop are NOT reachable
    // from the local tip.  Without including origin/develop here, those commits
    // are not recognised as primary and end up on wrong secondary lanes, making
    // the develop line visually break and feature branch tips go undetected by
    // the pre-pass.
    let primary_tips: Vec<String> = {
        let candidates = ["develop", "main", "master", "trunk"];
        let mut tips: Vec<String> = Vec::new();
        for name in candidates {
            let mut found_any = false;
            // Local ref
            if let Ok(r) = repo.find_reference(&format!("refs/heads/{}", name)) {
                if let Some(id) = r.resolve().ok().and_then(|r| r.target()) {
                    let s = id.to_string();
                    if !tips.contains(&s) { tips.push(s); }
                    found_any = true;
                }
            }
            // Remote-tracking refs (origin and upstream are the most common)
            for remote in &["origin", "upstream"] {
                if let Ok(r) = repo.find_reference(
                    &format!("refs/remotes/{}/{}", remote, name)
                ) {
                    if let Some(id) = r.resolve().ok().and_then(|r| r.target()) {
                        let s = id.to_string();
                        if !tips.contains(&s) { tips.push(s); }
                        found_any = true;
                    }
                }
            }
            if found_any { break; } // found the right branch name, stop iterating candidates
        }
        if tips.is_empty() {
            head_target.clone().into_iter().collect()
        } else {
            tips
        }
    };
    // Two sets derived from first-parent walks of ALL primary tips:
    //
    // • first_parent_set  — primary commits on the current page.
    // • all_primary_oids  — every commit on any primary first-parent chain,
    //   regardless of page, used to guard develop commits from being
    //   incorrectly claimed by feature branches.
    let (first_parent_set, all_primary_oids): (
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
    ) = {
        let mut page_set = std::collections::HashSet::new();
        let mut all_set  = std::collections::HashSet::new();
        for tip in &primary_tips {
            let mut cur = tip.clone();
            for _ in 0..200_000 {
                if all_set.contains(&cur) {
                    break; // chains have converged — no need to continue
                }
                all_set.insert(cur.clone());
                if page_oid_set.contains(&cur) {
                    page_set.insert(cur.clone());
                }
                let oid = match git2::Oid::from_str(&cur) { Ok(o) => o, Err(_) => break };
                match repo.find_commit(oid).and_then(|c| c.parent(0)) {
                    Ok(p) => cur = p.id().to_string(),
                    Err(_) => break,
                }
            }
        }
        (page_set, all_set)
    };

    // --- lane algorithm (pass 1) ---
    //
    // GitKraken-style: one direct edge per parent relationship (no pass-through edges).
    // Each edge spans from the child commit row to the parent commit row.  The frontend
    // draws these as right-angle elbow paths (vertical drop → sharp turn → vertical
    // descent) which keeps all branch lines strictly vertical and avoids T-junctions.
    //
    // Because edges span multiple rows we defer row resolution to pass 2, once every
    // commit has been placed.

    struct PendingEdge {
        from_row:         usize,
        from_lane:        usize,
        to_parent_oid:    String,
        to_lane:          usize,
        #[allow(dead_code)]
        color_index:      usize,
        /// true for pi>0 edges (merge parent) → horizontal-first shape; false for pi=0 (fork-point) → vertical-first shape
        is_merge_parent:  bool,
    }

    let mut lanes = Lanes::new();

    // --- pre-pass: seed the FIRST primary merge commit's feature-branch lanes ---
    //
    // Walk the page in revwalk order.  When we reach the FIRST primary
    // (develop) merge commit, pre-allocate secondary lanes for its non-primary
    // pi>0 parents (feature branch tips) before the main loop runs.
    //
    // Why this matters with Sort::TOPOLOGICAL | Sort::TIME:
    //   Active feature branches (B, C) have *newer* timestamps than an old
    //   merge commit (M_AR).  TIME ordering places B_tip and C_tip before
    //   M_AR in the revwalk.  Without this pre-pass, B and C would call
    //   alloc_secondary() first in the main loop, claiming lanes 1 and 2.
    //   When M_AR is finally reached, alloc_secondary(AR_tip) would return
    //   lane 3 (purple) even though AR should be lane 1 (orange).
    //
    // We stop after the FIRST primary merge commit.  Pre-allocating ALL merge
    // commits would fill lanes 1..N with every merged feature tip simultaneously,
    // pushing active (unmerged) branches to lane N+1, N+2, … causing the SVG
    // to become extremely wide ("lane sprawl").  For subsequent merge commits,
    // dynamic lane reuse in the main loop keeps the graph compact — by the time
    // an older merge commit is processed, earlier feature arcs have been freed.
    'prepass: for oid in &page {
        let oid_str = oid.to_string();
        if !all_primary_oids.contains(&oid_str) {
            continue;
        }
        let commit = match repo
            .find_object(*oid, None)
            .and_then(|obj| obj.peel_to_commit())
        {
            Ok(c) => c,
            Err(_) => continue,
        };
        if commit.parent_count() <= 1 {
            continue; // not a merge commit — keep scanning for the first primary merge
        }
        // First primary merge commit found — pre-allocate its non-primary parents.
        for (pi, parent) in commit.parents().enumerate() {
            if pi == 0 {
                continue;
            }
            let p_oid = parent.id().to_string();
            // Only pre-allocate non-primary parents (true feature branch tips).
            if all_primary_oids.contains(&p_oid) {
                continue;
            }
            if lanes.find(&p_oid).is_none() {
                lanes.alloc_secondary(&p_oid);
            }
        }
        break 'prepass; // stop — only the first primary merge needs pre-allocation
    }

    let mut nodes: Vec<CommitNode> = Vec::with_capacity(page.len());
    let mut pending: Vec<PendingEdge>  = Vec::new();
    let mut global_max_lane = 0usize;

    // Ghost reservations keyed by the OID of the commit they're waiting for
    // (typically a develop / main parent).  When that parent commit is
    // processed by the main loop, all lanes reserved on its behalf are
    // released so subsequent feature branches can recycle them — this is
    // what keeps the graph compact instead of sprawling ever-rightward as
    // the user scrolls back through history.
    let mut ghost_free_on: HashMap<String, Vec<usize>> = HashMap::new();

    for (local_row, oid) in page.iter().enumerate() {
        // Rows are absolute (offset + page-local index) so nodes from different
        // pages have non-overlapping coordinates and the frontend can concatenate
        // them without any coordinate fixup.
        let row = offset + local_row;
        let oid_str = oid.to_string();

        // Release any ghost reservations that were waiting for THIS commit.
        // Earlier feature-branch fork-points parked placeholders in their
        // original lane so sibling features wouldn't collapse onto them;
        // now that the parent they were waiting for has arrived, those lanes
        // can be recycled (matches GitKraken's lane recycling behavior).
        if let Some(slots) = ghost_free_on.remove(&oid_str) {
            for slot in slots {
                lanes.free(slot);
            }
        }
        // Use peel_to_commit so annotated tag OIDs (which revwalk can yield via
        // push_glob("refs/tags/*")) are transparently dereferenced to their target commit.
        let commit = repo
            .find_object(*oid, None)
            .and_then(|obj| obj.peel_to_commit())
            .map_err(|_| AppError::CommitNotFound(oid_str.clone()))?;

        // --- find this commit's lane ---
        //
        // For first-parent-chain (develop/main) commits we ALWAYS want the
        // lowest available slot (ideally 0).  If a feature commit stole this
        // commit and pre-allocated it to a higher slot, free that slot and
        // re-alloc so the primary branch stays as left as possible.
        let commit_lane = match lanes.find(&oid_str) {
            Some(k) if first_parent_set.contains(&oid_str) && k > 0 => {
                // Develop commit was pre-allocated to a non-zero lane by a feature
                // branch.  Free the stolen slot and try to get a better one.
                lanes.free(k);
                lanes.alloc(&oid_str)
            },
            Some(k) => k,
            None => {
                if first_parent_set.contains(&oid_str) {
                    lanes.alloc(&oid_str)
                } else {
                    lanes.alloc_secondary(&oid_str)
                }
            }
        };
        // free the current commit's slot — it will be reassigned to its first parent
        lanes.free(commit_lane);

        let color_index = commit_lane % 10;
        if commit_lane > global_max_lane {
            global_max_lane = commit_lane;
        }

        let parents: Vec<String> = commit.parents().map(|p| p.id().to_string()).collect();
        let is_merge = parents.len() > 1;
        let is_head = head_target.as_deref() == Some(&oid_str);

        // --- assign lanes for parents & queue pending edges ---
        for (pi, parent_oid) in parents.iter().enumerate() {
            if pi == 0 {
                // First parent: continue in the same lane (straight line).
                if let Some(existing) = lanes.find(parent_oid) {
                    if existing != commit_lane
                        && all_primary_oids.contains(parent_oid.as_str())
                        && first_parent_set.contains(&oid_str) // only develop commits reclaim develop parents
                        && commit_lane < existing
                    {
                        // The first parent is a main-branch commit that was pre-allocated
                        // to a higher lane (e.g. by a feature fork-point commit that had
                        // it as its own pi=0).  Reclaim it into the current (lower) lane.
                        // Note: we guard with first_parent_set.contains(&oid_str) so that
                        // a feature commit on lane 1 cannot "steal" a develop commit from
                        // lane 2 back to lane 1 — only develop-to-develop reclaims are
                        // allowed.
                        //
                        // set_force is required here: we intentionally want to move the
                        // OID from its old slot into commit_lane (the dedup inside
                        // set_force frees the old slot).
                        lanes.set_force(commit_lane, parent_oid);
                        pending.push(PendingEdge {
                            from_row: row, from_lane: commit_lane,
                            to_parent_oid: parent_oid.clone(), to_lane: commit_lane,
                            color_index, is_merge_parent: false,
                        });
                    } else {
                        // Already reserved in a valid lane — draw a line to wherever it is.
                        pending.push(PendingEdge {
                            from_row: row, from_lane: commit_lane,
                            to_parent_oid: parent_oid.clone(), to_lane: existing,
                            color_index, is_merge_parent: false,
                        });
                        // GHOST RESERVATION: when a feature branch's fork-point
                        // commit has its parent already placed in another lane
                        // (typically because a *sibling* feature branch already
                        // reserved develop's lane), the lane assigned to this
                        // feature (commit_lane) was freed at the top of the
                        // outer loop and is now empty.  Without reserving it,
                        // the NEXT secondary commit's alloc_secondary() will
                        // grab the same slot — causing two unrelated feature
                        // branches to collapse onto the same lane (the reported
                        // bug: 3 feature commits on the same orange lane).
                        //
                        // We park a sentinel that can never collide with a real
                        // OID lookup.  Topological revwalk emits children before
                        // parents, so the current commit's OID (oid_str) is
                        // guaranteed never to be searched by a subsequent
                        // `lanes.find()` — it is therefore safe to use as the
                        // reservation marker.
                        //
                        // The reservation is freed when the parent commit
                        // (parent_oid) is later processed (see ghost_free_on
                        // drain at the top of the loop).  This lets older
                        // feature branches recycle this lane instead of
                        // sprawling ever-rightward.
                        if existing != commit_lane && lanes.find(&oid_str).is_none() {
                            lanes.set_force(commit_lane, &oid_str);
                            ghost_free_on.entry(parent_oid.clone()).or_default().push(commit_lane);
                        }
                    }
                } else if !all_primary_oids.contains(&oid_str) && all_primary_oids.contains(parent_oid.as_str()) {
                    // Feature commit whose first parent is a main-chain (develop) commit.
                    //
                    // Reserve the lane for the develop parent so the slot stays occupied
                    // until develop is actually processed.  Without this, lanes.free()
                    // above makes the slot immediately available, and the next secondary
                    // branch's alloc_secondary() grabs it — causing two concurrent feature
                    // branches to share the same lane.
                    //
                    // try_set (not set): if develop is ALREADY placed in another lane by
                    // a previously processed feature branch fork-point, we must NOT steal
                    // it — doing so would collapse the two sibling feature branches onto
                    // the same lane (the exact bug report: "3 commit di 3 branch diversi
                    // sulla stessa lane").  try_set keeps develop where it was and falls
                    // back to a fresh secondary lane if commit_lane is occupied.
                    let placed_lane = lanes.try_set(commit_lane, parent_oid);
                    pending.push(PendingEdge {
                        from_row: row, from_lane: commit_lane,
                        to_parent_oid: parent_oid.clone(), to_lane: placed_lane,
                        color_index, is_merge_parent: false,
                    });
                    // Ghost reservation — see the comment in the mirror branch
                    // above for rationale.  Applies when develop was already
                    // placed in another lane by a sibling feature.
                    if placed_lane != commit_lane && lanes.find(&oid_str).is_none() {
                        lanes.set_force(commit_lane, &oid_str);
                        ghost_free_on.entry(parent_oid.clone()).or_default().push(commit_lane);
                    }
                } else {
                    // Reserve the current lane for the first parent (straight continuation).
                    // try_set: if the parent was already placed elsewhere (e.g. by another
                    // branch's pi>0 edge that ran before this commit), respect that
                    // placement rather than moving the OID to commit_lane.
                    let placed_lane = lanes.try_set(commit_lane, parent_oid);
                    pending.push(PendingEdge {
                        from_row: row, from_lane: commit_lane,
                        to_parent_oid: parent_oid.clone(), to_lane: placed_lane,
                        color_index, is_merge_parent: false,
                    });
                    if placed_lane != commit_lane && lanes.find(&oid_str).is_none() {
                        lanes.set_force(commit_lane, &oid_str);
                        ghost_free_on.entry(parent_oid.clone()).or_default().push(commit_lane);
                    }
                }
            } else {
                // Additional parents (pi>0 — merge commit absorbing a branch).
                let p_lane = if let Some(existing) = lanes.find(parent_oid) {
                    existing
                } else if all_primary_oids.contains(parent_oid.as_str()) {
                    commit_lane
                } else {
                    lanes.alloc_secondary(parent_oid)
                };
                if p_lane > global_max_lane { global_max_lane = p_lane; }
                pending.push(PendingEdge {
                    from_row: row, from_lane: commit_lane,
                    to_parent_oid: parent_oid.clone(), to_lane: p_lane,
                    color_index: p_lane % 10, is_merge_parent: true,
                });
            }
        }

        // active_indices() includes pre-allocated future feature lanes (from the
        // pre-pass). Do NOT use max_active() to update global_max_lane here or
        // the SVG width explodes. global_max_lane is already maintained by the
        // commit_lane and p_lane updates above.
        let active_lanes = lanes.active_indices();

        let author    = commit.author();
        let committer = commit.committer();
        let mut refs  = ref_map.get(&oid_str).cloned().unwrap_or_default();
        dedupe_refs(&mut refs);
        let body      = commit.body().filter(|b| !b.trim().is_empty()).map(String::from);

        nodes.push(CommitNode {
            oid: oid_str,
            short_oid: oid.to_string()[..7].to_string(),
            summary: commit.summary().unwrap_or("(no message)").to_string(),
            body,
            author: AuthorInfo {
                name: author.name().unwrap_or("").to_string(),
                email: author.email().unwrap_or("").to_string(),
            },
            committer: AuthorInfo {
                name: committer.name().unwrap_or("").to_string(),
                email: committer.email().unwrap_or("").to_string(),
            },
            timestamp: commit.time().seconds(),
            row,
            lane: commit_lane,
            color_index,
            refs,
            is_merge,
            is_head,
            active_lanes,
        });
    }

    // --- pass 2: resolve pending edges ---
    //
    // Map every parent OID to its row and lane within this page.  Parents outside
    // the page get to_row = page_end so the line visually trails off below.
    // `oid_to_lane` lets us fix up edges whose to_lane was only a placeholder
    // (pi>0 parents not yet allocated during pass 1).
    let oid_to_row:  HashMap<String, usize> = nodes.iter().map(|n| (n.oid.clone(), n.row)).collect();
    let oid_to_lane: HashMap<String, usize> = nodes.iter().map(|n| (n.oid.clone(), n.lane)).collect();
    // Absolute row just past the last commit in this page.  Any parent that
    // falls outside the page gets this as its provisional to_row, and the
    // frontend will repair it when the parent's page is loaded.
    let page_end = offset + nodes.len();

    let mut edges: Vec<GraphEdge> = Vec::with_capacity(pending.len());
    for pe in pending {
        let to_row  = oid_to_row.get(&pe.to_parent_oid).copied().unwrap_or(page_end);
        // Resolve the true lane from the processed node; fall back to the value
        // stored during pass 1 (which may already be correct for pi=0 edges).
        let to_lane = oid_to_lane.get(&pe.to_parent_oid).copied().unwrap_or(pe.to_lane);
        // Color rule: use the "secondary" (feature) lane's color for all crossing
        // edges so that a branch always appears in its own color end-to-end.
        //
        //  • straight (from == to): same lane, trivially consistent.
        //  • opening (from < to, line goes right — merge commit → feature tip):
        //    to_lane is the feature lane → use to_lane color.  ✓
        //  • closing (from > to, line goes left — feature's oldest commit → fork point):
        //    from_lane is the feature lane → use from_lane color so the departing
        //    line is the branch's own color, not the parent branch's color.
        let color_index = if pe.from_lane > to_lane {
            pe.from_lane % 10 // closing: feature branch going back to parent lane
        } else {
            to_lane % 10      // straight or opening
        };
        let is_trailing = to_row == page_end;
        // For trailing edges the to_lane is still a placeholder equal to from_lane,
        // so edge_type(from, from, …) would always return Straight, losing the fork/merge
        // distinction needed by the frontend.  Encode is_merge_parent as MergeLeft /
        // ForkLeft so the frontend can recompute the correct direction after repair.
        let et = if is_trailing && pe.from_lane == to_lane {
            if pe.is_merge_parent { EdgeType::MergeLeft } else { EdgeType::ForkLeft }
        } else {
            edge_type(pe.from_lane, to_lane, pe.is_merge_parent)
        };
        edges.push(GraphEdge {
            from_row:      pe.from_row,
            from_lane:     pe.from_lane,
            to_row,
            to_lane,
            color_index,
            edge_type:     et,
            to_parent_oid: if is_trailing { Some(pe.to_parent_oid) } else { None },
        });
    }

    Ok(GraphData {
        nodes,
        edges,
        lane_count: global_max_lane + 1,
        total_commits: total,
        offset,
        stashes: Vec::new(),
    })
}

/// Load a paginated slice of the full commit graph.
pub fn load_graph(repo: &Repository, offset: usize, limit: usize) -> Result<GraphData> {
    load_graph_inner(repo, offset, limit, None)
}

/// Load a paginated slice of the commit graph filtered to commits that touched `file_path`.
/// Uses a simple linear layout (all commits on lane 0, sequential edges) because the
/// filtered commits are non-contiguous in history, making the normal lane algorithm
/// produce isolated disconnected mini-graphs.
pub fn load_graph_for_file(repo: &Repository, file_path: &str, offset: usize, limit: usize) -> Result<GraphData> {
    // --- collect refs ---
    let mut ref_map: HashMap<String, Vec<RefLabel>> = HashMap::new();
    let head_target = repo.head().ok().and_then(|h| h.target()).map(|id| id.to_string());

    for reference in repo.references()? {
        let reference = reference?;
        let resolved = match reference.resolve() { Ok(r) => r, Err(_) => continue };
        let target_oid = match resolved.target() { Some(id) => id.to_string(), None => continue };
        let full_name = match reference.name() { Some(n) => n, None => continue };

        let (short_name, ref_type) = if let Some(s) = full_name.strip_prefix("refs/heads/") {
            (s.to_string(), RefType::LocalBranch)
        } else if let Some(s) = full_name.strip_prefix("refs/remotes/") {
            (s.to_string(), RefType::RemoteBranch)
        } else if let Some(s) = full_name.strip_prefix("refs/tags/") {
            (s.to_string(), RefType::Tag)
        } else {
            continue;
        };
        let is_current = head_target.as_deref() == Some(&target_oid);
        ref_map.entry(target_oid).or_default().push(RefLabel { name: short_name, ref_type, is_current });
    }

    // --- topological walk, filter to file-touching commits ---
    let mut revwalk = repo.revwalk()?;
    revwalk.push_glob("refs/heads/*")?;
    revwalk.push_glob("refs/tags/*")?;
    revwalk.push_glob("refs/remotes/*")?;
    if let Ok(head) = repo.head() {
        if let Some(id) = head.target() { let _ = revwalk.push(id); }
    }
    // Sort::TOPOLOGICAL | Sort::TIME: same reasoning as load_graph_inner —
    // keeps topological invariants while preferring recent commits over old
    // unrelated branch tips.
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    let all_oids: Vec<Oid> = revwalk
        .filter_map(|r| r.ok())
        .filter(|&oid| commit_touches_file(repo, oid, file_path))
        .collect();
    let total = all_oids.len();

    let page: Vec<Oid> = all_oids.into_iter().skip(offset).take(limit).collect();

    // --- linear layout: all commits on lane 0, one edge per adjacent pair ---
    let mut nodes: Vec<CommitNode> = Vec::with_capacity(page.len());
    for (local_row, oid) in page.iter().enumerate() {
        let row = offset + local_row;
        let oid_str = oid.to_string();
        let commit = repo
            .find_object(*oid, None)
            .and_then(|obj| obj.peel_to_commit())
            .map_err(|_| AppError::CommitNotFound(oid_str.clone()))?;

        let parents: Vec<String> = commit.parents().map(|p| p.id().to_string()).collect();
        let is_merge = parents.len() > 1;
        let is_head  = head_target.as_deref() == Some(&oid_str);
        let mut refs = ref_map.get(&oid_str).cloned().unwrap_or_default();
        dedupe_refs(&mut refs);
        let body     = commit.body().filter(|b| !b.trim().is_empty()).map(String::from);
        let author   = commit.author();
        let committer = commit.committer();

        nodes.push(CommitNode {
            oid: oid_str,
            short_oid: oid.to_string()[..7].to_string(),
            summary: commit.summary().unwrap_or("(no message)").to_string(),
            body,
            author: AuthorInfo {
                name: author.name().unwrap_or("").to_string(),
                email: author.email().unwrap_or("").to_string(),
            },
            committer: AuthorInfo {
                name: committer.name().unwrap_or("").to_string(),
                email: committer.email().unwrap_or("").to_string(),
            },
            timestamp: commit.time().seconds(),
            row,
            lane: 0,
            color_index: 0,
            refs,
            is_merge,
            is_head,
            active_lanes: vec![0],
        });
    }

    // Sequential edges: each node connects to the next one in the list (absolute rows).
    let mut edges: Vec<GraphEdge> = Vec::with_capacity(page.len().saturating_sub(1));
    for i in 0..nodes.len().saturating_sub(1) {
        edges.push(GraphEdge {
            from_row:      nodes[i].row,
            from_lane:     0,
            to_row:        nodes[i + 1].row,
            to_lane:       0,
            color_index:   0,
            edge_type:     EdgeType::Straight,
            to_parent_oid: None,
        });
    }
    // Trailing edge for the last commit when more pages remain.
    if !nodes.is_empty() && total > offset + nodes.len() {
        let last_row = nodes.last().unwrap().row;
        edges.push(GraphEdge {
            from_row:      last_row,
            from_lane:     0,
            to_row:        last_row + 1,
            to_lane:       0,
            color_index:   0,
            edge_type:     EdgeType::Straight,
            to_parent_oid: None, // file-filter graph has no cross-page OID to track
        });
    }

    Ok(GraphData {
        nodes,
        edges,
        lane_count: 1,
        total_commits: total,
        offset,
        stashes: Vec::new(),
    })
}

#[inline]
fn edge_type(from: usize, to: usize, is_merge_parent: bool) -> EdgeType {
    match from.cmp(&to) {
        std::cmp::Ordering::Equal => EdgeType::Straight,
        std::cmp::Ordering::Greater => {
            if is_merge_parent { EdgeType::MergeLeft } else { EdgeType::ForkLeft }
        }
        std::cmp::Ordering::Less => {
            if is_merge_parent { EdgeType::MergeRight } else { EdgeType::ForkRight }
        }
    }
}

// ---------------------------------------------------------------------------
// Commit detail (single commit metadata)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDetail {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub body: Option<String>,
    pub author: AuthorInfo,
    pub committer: AuthorInfo,
    pub timestamp: i64,
    pub committer_timestamp: i64,
    pub parent_oids: Vec<String>,
    pub refs: Vec<RefLabel>,
    pub is_head: bool,
}

pub fn get_commit_detail(repo: &Repository, oid_str: &str) -> Result<CommitDetail> {
    let oid = Oid::from_str(oid_str).map_err(|_| AppError::CommitNotFound(oid_str.to_string()))?;
    let commit = repo.find_commit(oid)?;
    let head_target = repo.head().ok().and_then(|h| h.target()).map(|id| id.to_string());

    let mut ref_map: HashMap<String, Vec<RefLabel>> = HashMap::new();
    for reference in repo.references()? {
        let reference = reference?;
        let resolved = match reference.resolve() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let target_oid = match resolved.target() {
            Some(id) => id.to_string(),
            None => continue,
        };
        let full_name = match reference.name() {
            Some(n) => n,
            None => continue,
        };
        let (short_name, ref_type) = if let Some(s) = full_name.strip_prefix("refs/heads/") {
            (s.to_string(), RefType::LocalBranch)
        } else if let Some(s) = full_name.strip_prefix("refs/remotes/") {
            (s.to_string(), RefType::RemoteBranch)
        } else if let Some(s) = full_name.strip_prefix("refs/tags/") {
            (s.to_string(), RefType::Tag)
        } else {
            continue;
        };
        let is_current = head_target.as_deref() == Some(&target_oid);
        ref_map.entry(target_oid).or_default().push(RefLabel { name: short_name, ref_type, is_current });
    }

    let is_head = head_target.as_deref() == Some(oid_str);
    let author = commit.author();
    let committer = commit.committer();
    let mut refs = ref_map.get(oid_str).cloned().unwrap_or_default();
    dedupe_refs(&mut refs);

    Ok(CommitDetail {
        oid: oid_str.to_string(),
        short_oid: oid_str[..7.min(oid_str.len())].to_string(),
        summary: commit.summary().unwrap_or("(no message)").to_string(),
        body: commit.body().filter(|b| !b.trim().is_empty()).map(String::from),
        author: AuthorInfo {
            name: author.name().unwrap_or("").to_string(),
            email: author.email().unwrap_or("").to_string(),
        },
        committer: AuthorInfo {
            name: committer.name().unwrap_or("").to_string(),
            email: committer.email().unwrap_or("").to_string(),
        },
        timestamp: commit.time().seconds(),
        committer_timestamp: committer.when().seconds(),
        parent_oids: commit.parents().map(|p| p.id().to_string()).collect(),
        refs,
        is_head,
    })
}

// ---------------------------------------------------------------------------
// File tree (all tracked files + last-touch commit info)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RepoFileEntry {
    pub path: String,
    pub last_commit_oid: Option<String>,
    pub last_commit_short_oid: Option<String>,
    pub last_commit_date: Option<i64>,
    pub last_commit_summary: Option<String>,
}

/// Return all paths tracked by the index. Very fast — no commit walking.
pub fn get_repo_files(repo: &Repository) -> Result<Vec<String>> {
    let mut index = repo.index()?;
    index.read(false)?;
    let mut paths: Vec<String> = index
        .iter()
        .filter_map(|e| std::str::from_utf8(&e.path).ok().map(|p| p.to_owned()))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Return the most-recent commit that touched each of the given paths.
/// Stops walking as soon as every path has been found (or MAX_COMMITS reached).
pub fn get_files_last_commit(repo: &Repository, paths: Vec<String>) -> Result<Vec<RepoFileEntry>> {
    const MAX_COMMITS: usize = 5_000;

    if paths.is_empty() {
        return Ok(vec![]);
    }

    let mut entry_map: HashMap<String, RepoFileEntry> = paths
        .into_iter()
        .map(|path| {
            let e = RepoFileEntry {
                path: path.clone(),
                last_commit_oid: None,
                last_commit_short_oid: None,
                last_commit_date: None,
                last_commit_summary: None,
            };
            (path, e)
        })
        .collect();

    let total = entry_map.len();
    let mut found = 0usize;

    let mut revwalk = repo.revwalk()?;
    if revwalk.push_head().is_ok() {
        revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.include_untracked(false).ignore_whitespace(false);

        let mut commit_count = 0usize;
        'walk: for oid_result in revwalk {
            if found >= total || commit_count >= MAX_COMMITS {
                break;
            }
            let oid = match oid_result { Ok(o) => o, Err(_) => continue };
            let commit = match repo.find_commit(oid) { Ok(c) => c, Err(_) => continue };
            let tree   = match commit.tree()            { Ok(t) => t, Err(_) => continue };

            let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
            let diff = match parent_tree {
                Some(ref pt) => repo.diff_tree_to_tree(Some(pt), Some(&tree), Some(&mut diff_opts)),
                None         => repo.diff_tree_to_tree(None,     Some(&tree), Some(&mut diff_opts)),
            };
            let diff = match diff { Ok(d) => d, Err(_) => continue };

            let oid_full  = oid.to_string();
            let short_oid = oid_full[..7].to_string();
            let date      = commit.time().seconds();
            let summary   = commit.summary().unwrap_or("").to_string();

            for delta in diff.deltas() {
                let candidates = [
                    delta.new_file().path().and_then(|p| p.to_str()),
                    delta.old_file().path().and_then(|p| p.to_str()),
                ];
                for path in candidates.into_iter().flatten() {
                    if let Some(entry) = entry_map.get_mut(path) {
                        if entry.last_commit_oid.is_none() {
                            entry.last_commit_oid       = Some(oid_full.clone());
                            entry.last_commit_short_oid = Some(short_oid.clone());
                            entry.last_commit_date      = Some(date);
                            entry.last_commit_summary   = Some(summary.clone());
                            found += 1;
                            if found >= total { break 'walk; }
                        }
                    }
                }
            }
            commit_count += 1;
        }
    }

    let mut result: Vec<RepoFileEntry> = entry_map.into_values().collect();
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

/// Return all files tracked by the index together with the most-recent commit
/// that touched each one.  We walk at most `MAX_COMMITS` commits; files not
/// found within that window still appear but with `None` metadata.
pub fn get_repo_file_tree(repo: &Repository) -> Result<Vec<RepoFileEntry>> {
    const MAX_COMMITS: usize = 20_000;

    // 1. Seed the map from the index so every tracked file is present.
    let mut index = repo.index()?;
    index.read(false)?;

    let mut entry_map: std::collections::HashMap<String, RepoFileEntry> = index
        .iter()
        .filter_map(|e| std::str::from_utf8(&e.path).ok().map(|p| p.to_owned()))
        .map(|path| {
            let e = RepoFileEntry {
                path: path.clone(),
                last_commit_oid: None,
                last_commit_short_oid: None,
                last_commit_date: None,
                last_commit_summary: None,
            };
            (path, e)
        })
        .collect();

    let total = entry_map.len();
    let mut found: usize = 0;

    // 2. Walk commits newest-first; stop once every file has been assigned.
    let mut revwalk = repo.revwalk()?;
    // Push HEAD; silently skip if repo is empty.
    if revwalk.push_head().is_ok() {
        revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.include_untracked(false).ignore_whitespace(false);

        let mut commit_count = 0usize;
        'walk: for oid_result in revwalk {
            if found >= total || commit_count >= MAX_COMMITS {
                break;
            }
            let oid = match oid_result { Ok(o) => o, Err(_) => continue };
            let commit = match repo.find_commit(oid) { Ok(c) => c, Err(_) => continue };
            let tree   = match commit.tree()            { Ok(t) => t, Err(_) => continue };

            let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
            let diff = match parent_tree {
                Some(ref pt) => repo.diff_tree_to_tree(Some(pt), Some(&tree), Some(&mut diff_opts)),
                None         => repo.diff_tree_to_tree(None,     Some(&tree), Some(&mut diff_opts)),
            };
            let diff = match diff { Ok(d) => d, Err(_) => continue };

            let oid_full  = oid.to_string();
            let short_oid = oid_full[..7].to_string();
            let date      = commit.time().seconds();
            let summary   = commit.summary().unwrap_or("").to_string();

            for delta in diff.deltas() {
                // Check both new and old paths (handles renames/deletes).
                let candidates = [
                    delta.new_file().path().and_then(|p| p.to_str()),
                    delta.old_file().path().and_then(|p| p.to_str()),
                ];
                for path in candidates.into_iter().flatten() {
                    if let Some(entry) = entry_map.get_mut(path) {
                        if entry.last_commit_oid.is_none() {
                            entry.last_commit_oid       = Some(oid_full.clone());
                            entry.last_commit_short_oid = Some(short_oid.clone());
                            entry.last_commit_date      = Some(date);
                            entry.last_commit_summary   = Some(summary.clone());
                            found += 1;
                            if found >= total { break 'walk; }
                        }
                    }
                }
            }
            commit_count += 1;
        }
    }

    let mut result: Vec<RepoFileEntry> = entry_map.into_values().collect();
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}
