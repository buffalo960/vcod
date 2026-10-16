//! Playing on a server: the stock menus, the usercmd stream, prediction and
//! the first-person weapon.

pub mod clock;
pub mod cmds;
pub mod cvars;
pub mod demos;
pub mod events;
pub mod input;
pub mod join;
#[cfg(test)]
mod loopback;
pub mod predict;
pub mod quake;
pub mod recoil;
#[cfg(test)]
mod replay;
pub mod seat;
pub mod shock;
pub mod view;
