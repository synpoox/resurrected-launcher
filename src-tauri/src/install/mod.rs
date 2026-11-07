//! Install module: plan and execute install, adopt, repair, update, uninstall,
//! clean-install, cancel and restart recovery.
//!
//! Owns archive download/verification (#35), confined extraction (#35), the durable
//! transaction journal, per-install operation locks and recovery (#36), and the
//! operations with file preservation (#37). Callers never orchestrate file swaps.
//!
//! Every write into a product root must hold a [`crate::contract::WriteGuard`], and
//! every transaction reports each [`crate::contract::Boundary`] of its
//! [`crate::contract::Boundary::sequence`] to a [`crate::contract::FaultInjector`].
