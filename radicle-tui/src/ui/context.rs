use radicle::prelude::{Id, Project};
use radicle::Profile;

use radicle::storage::git::Repository;
use radicle::storage::ReadStorage;
pub struct Context {
    profile: Profile,
    id: Id,
    project: Project,
    repository: Repository,
}

impl Context {
    pub fn new(profile: Profile, id: Id, project: Project) -> Self {
        let repository = profile.storage.repository(id).unwrap();

        Self {
            id,
            profile,
            project,
            repository,
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn repository(&self) -> &Repository {
        &self.repository
    }
}
