mod store;
mod types;

pub use store::*;
pub use types::*;

#[derive(Debug)]
pub struct AddressManager<S> {
    store: S,
}

impl<S: Store> AddressManager<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
}
