use std::path::Path;

use crate::{GitError, KeyGenerator};

/// Git hash-based key generator, compatible with reg-keygen-git-hash-plugin.
///
/// Explores the commit graph to find a suitable base commit for comparison.
#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;
    use crate::KeyGenerator;

    struct GitTestRepo {
        dir: TempDir,
    }

    impl GitTestRepo {
        fn new() -> Self {
            let dir = TempDir::new().expect("failed to create temp dir");
            let repo = Self { dir };
            repo.git(&["init", "-b", "main"]);
            repo.git(&["config", "user.email", "test@test.com"]);
            repo.git(&["config", "user.name", "Test"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            repo
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }

        fn git(&self, args: &[&str]) -> String {
            let output = Command::new("git")
                .args(args)
                .current_dir(self.path())
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@test.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@test.com")
                .output()
                .expect("failed to run git");
            assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        }

        fn commit(&self, msg: &str) -> String {
            self.git(&["commit", "--allow-empty", "-m", msg]);
            self.head_hash()
        }

        fn create_branch(&self, name: &str) {
            self.git(&["branch", name]);
        }

        fn checkout(&self, name: &str) {
            self.git(&["checkout", name]);
        }

        fn checkout_new_branch(&self, name: &str) {
            self.git(&["checkout", "-b", name]);
        }

        fn checkout_detach(&self, rev: &str) {
            self.git(&["checkout", "--detach", rev]);
        }

        fn head_hash(&self) -> String {
            self.git(&["rev-parse", "HEAD"])
        }

        fn keygen(&self) -> GitHashKeygen {
            GitHashKeygen::new(self.path()).expect("failed to create keygen")
        }
    }

    // ===== Group A: Constructor =====

    #[test]
    fn test_new_valid_repo() {
        let repo = GitTestRepo::new();
        assert!(GitHashKeygen::new(repo.path()).is_ok());
    }

    #[test]
    fn test_new_nonexistent_path() {
        let result = GitHashKeygen::new(Path::new("/nonexistent/path/to/repo"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, GitError::RepoNotFound(_)));
    }

    #[test]
    fn test_new_plain_directory() {
        let dir = TempDir::new().unwrap();
        // A plain directory (no .git) still passes new() due to lazy validation
        assert!(GitHashKeygen::new(dir.path()).is_ok());
    }

    #[test]
    fn test_new_regular_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("somefile.txt");
        std::fs::write(&file_path, "hello").unwrap();
        let result = GitHashKeygen::new(&file_path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GitError::RepoNotFound(_)));
    }

    // ===== Group B: get_actual_key / HEAD hash =====

    #[test]
    fn test_actual_key_valid_sha() {
        let repo = GitTestRepo::new();
        repo.commit("initial");
        let keygen = repo.keygen();
        let key = keygen.get_actual_key().unwrap();
        assert_eq!(key.len(), 40);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(key, repo.head_hash());
    }

    #[test]
    fn test_actual_key_consistent() {
        let repo = GitTestRepo::new();
        repo.commit("initial");
        let keygen = repo.keygen();
        let key1 = keygen.get_actual_key().unwrap();
        let key2 = keygen.get_actual_key().unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_actual_key_changes_after_commit() {
        let repo = GitTestRepo::new();
        repo.commit("first");
        let key1 = repo.keygen().get_actual_key().unwrap();
        repo.commit("second");
        let key2 = repo.keygen().get_actual_key().unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_actual_key_empty_repo_errors() {
        let repo = GitTestRepo::new();
        // No commits yet
        let result = repo.keygen().get_actual_key();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GitError::Git(_)));
    }

    // ===== Group C: get_expected_key / fork point detection =====

    #[test]
    fn test_expected_key_feature_from_main() {
        // main: A → B, feature: C → D (HEAD)
        let repo = GitTestRepo::new();
        repo.commit("A");
        let b = repo.commit("B");
        repo.checkout_new_branch("feature");
        repo.commit("C");
        repo.commit("D");
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, Some(b));
    }

    #[test]
    fn test_expected_key_immediate_parent() {
        // main: A, feature: B (HEAD)
        let repo = GitTestRepo::new();
        let a = repo.commit("A");
        repo.checkout_new_branch("feature");
        repo.commit("B");
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, Some(a));
    }

    #[test]
    fn test_expected_key_single_branch_none() {
        // main: A → B (HEAD), no other branches
        let repo = GitTestRepo::new();
        repo.commit("A");
        repo.commit("B");
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, None);
    }

    #[test]
    fn test_expected_key_single_commit_none() {
        // main: A (HEAD), no other branches
        let repo = GitTestRepo::new();
        repo.commit("A");
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, None);
    }

    #[test]
    fn test_expected_key_head_same_as_branch() {
        // main: A, feature (HEAD) = A (just branched, no new commits)
        let repo = GitTestRepo::new();
        let a = repo.commit("A");
        repo.checkout_new_branch("feature");
        // HEAD is A, which is also where main points
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, Some(a));
    }

    #[test]
    fn test_expected_key_multiple_branches_nearest() {
        // main: A, develop: A → B, feature: C → D (HEAD)
        // The nearest branch tip in the walk from HEAD is B (develop)
        let repo = GitTestRepo::new();
        repo.commit("A");
        repo.checkout_new_branch("develop");
        let b = repo.commit("B");
        repo.checkout_new_branch("feature");
        repo.commit("C");
        repo.commit("D");
        let expected = repo.keygen().get_expected_key().unwrap();
        // Walking back from D: D(HEAD) → C → B, B is develop's tip
        assert_eq!(expected, Some(b));
    }

    #[test]
    fn test_expected_key_detached_head() {
        // main: A → B, detach at A
        // Detached HEAD: all branches are candidates (current_branch = None)
        // Walk: A (step 1). A == main? No (main points to B). No parent → done.
        let repo = GitTestRepo::new();
        let a = repo.commit("A");
        repo.commit("B");
        repo.checkout_detach(&a);
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, None);
    }

    #[test]
    fn test_expected_key_after_merge() {
        // main: A → C → M(merge), feature → B, new-feat: D (HEAD)
        // Walk from D: D → M → C → A. main points to M → found at step 2
        let repo = GitTestRepo::new();
        repo.commit("A");
        repo.checkout_new_branch("feature");
        repo.commit("B");
        repo.checkout("main");
        repo.commit("C");
        // Merge feature into main
        repo.git(&["merge", "feature", "--no-ff", "-m", "M"]);
        let merge_sha = repo.head_hash();
        repo.checkout_new_branch("new-feat");
        repo.commit("D");
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, Some(merge_sha));
    }

    // ===== Group D: Boundary / Stress =====

    #[test]
    #[ignore]
    fn test_expected_key_300th_step_found() {
        // main: fork, feature: 299 commits
        // Walk: steps 1..=299 are feature commits, step 300 is fork point
        let repo = GitTestRepo::new();
        let fork = repo.commit("fork");
        repo.create_branch("main-ref");
        repo.checkout_new_branch("feature");
        for i in 1..=299 {
            repo.commit(&format!("f{i}"));
        }
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, Some(fork));
    }

    #[test]
    #[ignore]
    fn test_expected_key_301st_step_not_found() {
        // main: fork, feature: 300 commits
        // Walk: steps 1..=300 are feature commits, step 301 would be fork but count > 300
        let repo = GitTestRepo::new();
        repo.commit("fork");
        repo.create_branch("main-ref");
        repo.checkout_new_branch("feature");
        for i in 1..=300 {
            repo.commit(&format!("f{i}"));
        }
        let expected = repo.keygen().get_expected_key().unwrap();
        assert_eq!(expected, None);
    }
}
