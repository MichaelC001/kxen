use sha2::Digest;

use super::Goal;

pub(super) fn contract(goal: &Goal) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"kxen-goal-completion-contract-v1\0");
    hash_part(&mut hasher, Some(&goal.contract.objective));
    hash_part(&mut hasher, Some(&goal.contract.completion_criteria));
    hash_part(&mut hasher, goal.contract.constraints.as_deref());
    crate::core::shared::hex_lower(&hasher.finalize())
}

pub(super) fn evidence(value: &str) -> String {
    crate::core::shared::hex_lower(&sha2::Sha256::digest(value.as_bytes()))
}

fn hash_part(hasher: &mut sha2::Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        None => hasher.update([0]),
    }
}
