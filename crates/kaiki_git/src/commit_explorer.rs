use std::path::Path;

use crate::{GitError, KeyGenerator};

/// Git hash-based key generator, compatible with reg-keygen-git-hash-plugin.
///
/// Explores the commit graph to find a suitable base commit for comparison.
pub struct GitHashKeygen {
    repo_path: std::path::PathBuf,
}

impl GitHashKeygen {
    pub fn new(repo_path: &Path) -> Result<Self, GitError> {
        if !repo_path.join(".git").exists() && !repo_path.is_dir() {
            return Err(GitError::RepoNotFound(repo_path.display().to_string()));
        }
        Ok(Self { repo_path: repo_path.to_path_buf() })
    }

    /// Find a base commit by exploring the commit graph.
    ///
    /// Algorithm:
    /// 1. Get current branch name
    /// 2. Build commit graph from `git log -n 300 --graph`
    /// 3. Find commits reachable from other branches
    /// 4. Use merge-base to identify fork point
    fn find_base_commit(&self) -> Result<Option<String>, GitError> {
        let repo = gix::open(&self.repo_path).map_err(|e| GitError::Git(e.to_string()))?;

        let mut head = repo.head().map_err(|e| GitError::Git(e.to_string()))?;

        // Get the current branch name (before peel mutates head)
        let current_branch = head.referent_name().map(|n| n.as_bstr().to_string());

        let head_commit = head.peel_to_commit().map_err(|e| GitError::Git(e.to_string()))?;

        tracing::debug!(
            branch = ?current_branch,
            head = %head_commit.id,
            "exploring commit graph"
        );

        // Walk up to 300 commits to find a fork point
        let mut revwalk = head_commit
            .ancestors()
            .first_parent_only()
            .all()
            .map_err(|e| GitError::Git(e.to_string()))?;

        let mut count = 0;
        let max_commits = 300;

        // Collect references to find other branches
        let references: Vec<_> = repo
            .references()
            .map_err(|e| GitError::Git(e.to_string()))?
            .local_branches()
            .map_err(|e| GitError::Git(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter(|r| {
                current_branch.as_ref().is_none_or(|current| r.name().as_bstr() != current.as_str())
            })
            .filter_map(|r| r.into_fully_peeled_id().ok().map(|id| id.detach()))
            .collect();

        while let Some(Ok(info)) = revwalk.next() {
            count += 1;
            if count > max_commits {
                break;
            }

            let commit_id = info.id;

            // Check if this commit is reachable from any other branch
            for ref_id in &references {
                if commit_id == ref_id.as_ref() {
                    return Ok(Some(commit_id.to_string()));
                }
            }
        }

        Ok(None)
    }

    fn get_head_hash(&self) -> Result<String, GitError> {
        let repo = gix::open(&self.repo_path).map_err(|e| GitError::Git(e.to_string()))?;
        let mut head = repo.head().map_err(|e| GitError::Git(e.to_string()))?;
        let commit = head.peel_to_commit().map_err(|e| GitError::Git(e.to_string()))?;
        Ok(commit.id.to_string())
    }
}

impl KeyGenerator for GitHashKeygen {
    fn get_expected_key(&self) -> Result<Option<String>, GitError> {
        self.find_base_commit()
    }

    fn get_actual_key(&self) -> Result<String, GitError> {
        self.get_head_hash()
    }
}
