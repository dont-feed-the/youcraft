//! How a channel shows itself: its name, description and photo, which its owners change;
//! administrators moderate any in `admin`.

use youcraft_api::commands::channel::{NAME_LIMIT, Which};
use youcraft_api::commands::profile::{
    Customization, DESCRIPTION_LIMIT, NewPhoto, Profile, RemovePhoto, SetPhoto, Update,
};
use youcraft_api::{ApiError, ChannelId, ChannelRole};
use youcraft_store::profiles::{PhotoFiles, ProfileEdit};

use crate::dispatch::Handle;
use crate::handlers::input::{line, optional_text};
use crate::photo;
use crate::site::{Caller, Site};

impl Site {
    /// How `channel` shows itself.
    pub async fn profile(&self, channel: ChannelId) -> Result<Profile, ApiError> {
        self.db(move |store| store.profile(channel))
            .await?
            .ok_or(ApiError::NotFound)
    }

    /// Give `channel` the photo `photo`, or none, and forget the files of the one it had.
    pub async fn replace_photo(
        &self,
        channel: ChannelId,
        photo: Option<PhotoFiles>,
    ) -> Result<Profile, ApiError> {
        let version = photo.as_ref().map(|photo| photo.version.clone());
        let old = self
            .db(move |store| store.set_photo(channel, photo.as_ref()))
            .await?;
        if old != version {
            photo::remove(self, channel, old).await;
        }
        self.profile(channel).await
    }
}

impl Handle<Update> for Site {
    async fn handle(&self, caller: Caller, change: Customization) -> Result<Profile, ApiError> {
        let channel = change.channel;
        self.allow_in_channel(caller, channel, ChannelRole::Owner)
            .await?;
        let name = change
            .name
            .as_deref()
            .map(|name| line("name", name, NAME_LIMIT))
            .transpose()?;
        let description = change
            .description
            .as_deref()
            .map(|description| optional_text("description", description, DESCRIPTION_LIMIT))
            .transpose()?;
        self.db(move |store| {
            let edit = ProfileEdit {
                name: name.as_deref(),
                description: description.as_deref(),
            };
            store.edit_profile(channel, &edit)
        })
        .await?
        .ok_or(ApiError::NotFound)
    }
}

impl Handle<SetPhoto> for Site {
    async fn handle(&self, caller: Caller, new: NewPhoto) -> Result<Profile, ApiError> {
        self.allow_in_channel(caller, new.channel, ChannelRole::Owner)
            .await?;
        let made = photo::make(self, new.channel, &new.image).await?;
        self.replace_photo(new.channel, Some(made)).await
    }
}

impl Handle<RemovePhoto> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Profile, ApiError> {
        self.allow_in_channel(caller, which.channel, ChannelRole::Owner)
            .await?;
        self.replace_photo(which.channel, None).await
    }
}
