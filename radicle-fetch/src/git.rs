pub(crate) mod mem;
pub(crate) mod repository;
pub(crate) mod packfile;

pub mod refs;

pub(crate) mod oid {
    //! Helper functions for converting to/from [`radicle::git::Oid`] and
    //! [`ObjectId`].

    use gix_hash::ObjectId;
    use radicle::git::Oid;

    /// Convert from an [`ObjectId`] to an [`Oid`].
    pub fn to_oid(oid: ObjectId) -> Oid {
        Oid::try_from(oid.as_bytes()).expect("invalid gix Oid")
    }

    /// Convert from an [`Oid`] to an [`ObjectId`].
    pub fn to_object_id(oid: Oid) -> ObjectId {
        ObjectId::from(oid.as_bytes())
    }
}
