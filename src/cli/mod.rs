//! CLI subcommand handlers. Each module contains the implementation of one
//! `alluvium <subcommand>` entry point.

pub mod archive;
pub mod consolidate;
pub mod dry_run;
pub mod init;
pub mod lint;
pub mod paths;
pub mod pre_compact;
pub mod replay;
pub mod session_end;
pub mod session_start;
pub mod status;
pub mod uninstall;
