use anyhow::{anyhow, Context};
use git_cmd::Repo;
use git_url_parse::GitUrl;

#[derive(Debug, Clone)]
pub struct RepoUrl {
    pub scheme: String,
    pub host: String,
    port: Option<u16>,
    pub owner: String,
    pub name: String,
}

impl RepoUrl {
    pub fn new(git_host_url: &str) -> anyhow::Result<Self> {
        let git_url = GitUrl::parse(git_host_url)
            .map_err(|err| anyhow!("cannot parse git url {}: {}", git_host_url, err))?;
        let owner = git_url
            .owner
            .with_context(|| format!("cannot find owner in git url {git_host_url}"))?;
        let name = git_url.name;
        let host = git_url
            .host
            .with_context(|| format!("cannot find host in git url {git_host_url}"))?;
        let port = git_url.port;
        let scheme = git_url.scheme.to_string();
        Ok(RepoUrl {
            owner,
            name,
            host,
            port,
            scheme,
        })
    }

    pub fn from_repo(repo: &Repo) -> Result<Self, anyhow::Error> {
        let url = repo
            .original_remote_url()
            .context("cannot determine origin url")?;
        RepoUrl::new(&url)
    }

    pub fn is_on_github(&self) -> bool {
        self.host.contains("github")
    }

    /// Get GitHub/Gitea release link
    pub fn git_release_link(&self, prev_tag: &str, new_tag: &str) -> String {
        let host = format!("https://{}/{}/{}", self.host, self.owner, self.name);

        if prev_tag == new_tag {
            format!("{host}/releases/tag/{new_tag}")
        } else {
            format!("{host}/compare/{prev_tag}...{new_tag}")
        }
    }

    pub fn git_pr_link(&self) -> String {
        let host = format!("https://{}/{}/{}", self.host, self.owner, self.name);
        let pull_path = if self.is_on_github() { "pull" } else { "pulls" };
        format!("{host}/{pull_path}")
    }

    pub fn gitea_api_url(&self) -> String {
        let v1 = "api/v1/";
        if let Some(port) = self.port {
            format!("{}://{}:{}/{v1}", self.scheme, self.host, port)
        } else {
            format!("{}://{}/{v1}", self.scheme, self.host)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::RepoUrl;

    const GITHUB_REPO_URL: &str = "https://github.com/MarcoIeni/release-plz";

    #[test]
    fn gh_release_link_works_for_first_release() {
        let repo = RepoUrl::new(GITHUB_REPO_URL).unwrap();
        let tag = "v0.0.1";
        let expected_url = format!("{GITHUB_REPO_URL}/releases/tag/{tag}");
        // when we are at the first release, we have the prev_tag and the new_tag to be
        // the same as there is no other tag available.
        let release_link = repo.git_release_link(tag, tag);
        assert_eq!(expected_url, release_link);
    }

    #[test]
    fn gh_release_link_for_crates_already_published() {
        let repo = RepoUrl::new(GITHUB_REPO_URL).unwrap();
        let previous_tag = "v0.1.0";
        let next_tag = "v0.5.0";
        // when there is already a previous version, we should use the compare url, with the
        // ranging between the previous tag and the newest one
        let expected_url = format!("{GITHUB_REPO_URL}/compare/{previous_tag}...{next_tag}");
        let release_link = repo.git_release_link(previous_tag, next_tag);
        assert_eq!(expected_url, release_link);
    }
}
