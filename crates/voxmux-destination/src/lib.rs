pub mod dest_trait;
#[cfg(feature = "discord")]
pub mod discord_dest;
pub mod file_dest;
pub mod host;
pub mod registry;

pub use dest_trait::Destination;
#[cfg(feature = "discord")]
pub use discord_dest::DiscordDestination;
pub use file_dest::FileDestination;
pub use host::DestinationHost;
pub use registry::DestinationRegistry;
