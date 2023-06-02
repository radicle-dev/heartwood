use radicle::prelude::{Id, Project};
use radicle::Profile;

pub struct Context {
    profile: Profile,
    id: Id,
    project: Project,
}

impl Context {
    pub fn new(profile: Profile, id: Id, project: Project) -> Self {
        Self {
            id,
            profile,
            project,
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
}
