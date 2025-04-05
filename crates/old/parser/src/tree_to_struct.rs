use std::sync::Arc;

use crate::{
    dependencies::get_dependencies,
    structure::{Positioned, Target, Tree},
};

use super::{
    features::get_features,
    structure::{Cargo, DependencyKind},
};

impl Cargo {
    pub(crate) fn update_struct(&mut self, tree: &Tree, target: Arc<Vec<Positioned<Target>>>) {}
}

// cargo-features — Unstable, nightly-only features.
