//! Identity layer: Cleanverse Verified Identity (CVI).
//!
//! The oracle proves that a resource changed hands. It says nothing about
//! **who** the parties are: up to this layer a participant is a bare wallet
//! address we know nothing about. This is where that gap closes.
//!
//! The check sits in two places, and that is deliberate:
//!
//! * **in the contract** — `EscrowLock.fund` asks the validator about both
//!   parties before moving a single token. There is no way around it: the
//!   check lives where the money does;
//! * **here, in the application** — so that a person learns about the
//!   requirement before spending gas on a transaction doomed to revert, and
//!   gets the link where an identity can be obtained.
//!
//! No personal data reaches us. Verification happens on their side; we see
//! only the fact of it — an identity exists, it is valid, its tier is such.
//! That is strictly better than holding documents ourselves, where trust
//! would simply have moved from the seller onto us.

pub mod core;
pub mod types;
