//! A project's stack map (the Stack view): each repository's languages, frameworks and versions, its direct
//! dependencies, Compose services, databases, ports, expected variables and CI workflows, the links between the
//! repositories, and what the package registries, OSV.dev and endoflife.date say of the versions used.

use std::path::Path;

mod deps;
mod map;
pub mod remote;
#[cfg(test)]
mod tests;
pub mod version;

pub use deps::{Dep, Ecosystem};
pub use map::{Database, Link, LinkKind, RepoMap, Service, StackMap, Workflow, map};
pub use remote::{Check, DepCheck, Fetch, Severity, Support, Vuln, curl};

/// What the outside says of `repo`'s dependencies, runtimes and frameworks, kept in the `cache` file for a day.
pub fn check(repo: &Path, cache: &Path, refresh: bool, fetch: &Fetch<'_>) -> Check {
    let today = jiff::Timestamp::now().strftime("%Y-%m-%d").to_string();
    remote::check(
        &deps::read(repo),
        &map::tags(repo),
        cache,
        refresh,
        fetch,
        &today,
    )
}
