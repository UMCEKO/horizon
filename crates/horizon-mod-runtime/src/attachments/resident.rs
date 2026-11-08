//! Owning storage for a resident holder, distinct from detached map snapshots.

use super::{
    AttachmentAccess, AttachmentError, AttachmentHost, AttachmentLifetime, AttachmentRegistration,
    Attachments, Holder, index::AttachmentIndex,
};
use std::{
    hash::Hash,
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
};

/// Canonical storage whose lifetime is independent of borrowed map contents.
///
/// Moves preserve publication. Clones contain detached data only, so saving or
/// journaling a clone never owns the original holder's residency.
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct ResidentAttachments {
    contents: Attachments,
    lifetime: AttachmentLifetime,
}

impl ResidentAttachments {
    /// Bind the owning storage, not a borrowed data map.
    pub fn bind_index<H: Copy + Eq + Hash + Send + Sync + 'static>(
        &mut self,
        index: &Arc<Mutex<AttachmentIndex<H>>>,
        holder: H,
    ) -> AttachmentRegistration {
        let binding = self.contents.bind_index(index, holder);
        self.lifetime = AttachmentLifetime::new(binding.clone());
        binding
    }

    /// Copy the current registration without owning its lifetime.
    pub fn registration(&self) -> Option<AttachmentRegistration> {
        self.lifetime.registration()
    }

    /// Retire before a removed owner is returned or retained across callbacks.
    pub fn retire(&mut self) {
        self.lifetime = AttachmentLifetime::default();
    }
}

impl Drop for ResidentAttachments {
    fn drop(&mut self) {
        self.retire();
    }
}

impl Deref for ResidentAttachments {
    type Target = Attachments;
    fn deref(&self) -> &Self::Target {
        &self.contents
    }
}

impl DerefMut for ResidentAttachments {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.contents
    }
}

impl AttachmentHost for ResidentAttachments {
    fn attachments(
        &mut self,
        _: Holder,
        _: AttachmentAccess,
    ) -> Result<&mut Attachments, AttachmentError> {
        Ok(&mut self.contents)
    }
}
