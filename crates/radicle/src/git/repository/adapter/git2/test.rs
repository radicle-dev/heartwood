// TODO(finto): The tests within these submodules could be generic and form
// contracts for any adapter of the Git interfaces. For now, since we only
// define them for `crate::git::raw`, we will leave them as-is.

mod ancestry;
mod object;
mod reference;
mod revwalk;
mod symbolic;
