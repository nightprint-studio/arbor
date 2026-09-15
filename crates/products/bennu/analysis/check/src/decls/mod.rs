//! Declaration legality: modifiers, duplicates and redeclarations, constructors, initialization, unused members.

pub mod constructors;
pub mod ctor_before;
pub mod ctor_checks;
pub mod ctor_recursion;
pub mod declarations;
pub mod duplicates;
pub mod erasure_clash;
pub mod iface_dup;
pub mod init_checks;
pub mod local_class;
pub mod method_body;
pub mod record_ctor;
pub mod redeclaration;
pub mod self_ref;
pub mod unused_member;
