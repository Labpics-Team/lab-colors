//! FV-01 public-surface sentinel.
//!
//! The final-visible node is not allowed to mint authority from detached
//! snapshots, Paint values, or CSS strings. The public capability itself must
//! therefore live on the Program wire seam and remain opaque to consumers.

use labcolors_core::program_wire::AttachedMaterializationAuthorityV1;

#[test]
fn attached_materialization_authority_is_a_public_typed_capability() {
    fn accepts_capability(_: &AttachedMaterializationAuthorityV1) {}
    let _ = accepts_capability as fn(&AttachedMaterializationAuthorityV1);
}
