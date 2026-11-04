//! How the caller's channel shows itself: its name, description and photo, which every account
//! that is not blocked changes on its own channel; administrators moderate others' in `admin`.

use youcraft_api::commands::channel::NAME_LIMIT;
use youcraft_api::commands::profile::{
    Customization, DESCRIPTION_LIMIT, NewPhoto, Profile, RemovePhoto, SetPhoto, Update,
};
use youcraft_api::commands::site::Nothing;
use youcraft_api::{AccountId, ApiError};
use youcraft_store::profiles::{PhotoFiles, ProfileEdit};

use crate::dispatch::Handle;
use crate::handlers::input::{line, optional_text};
use crate::photo;
use crate::site::{Caller, Site};

impl Site {
    /// How the channel of `account` shows itself.
    pub async fn profile(&self, account: AccountId) -> Result<Profile, ApiError> {
        self.db(move |store| store.profile(account))
            .await?
            .ok_or(ApiError::NotFound)
    }

    /// Give the channel of `account` the photo `photo`, or none, and forget the files of the one
    /// it had.
    pub async fn replace_photo(
        &self,
        account: AccountId,
        photo: Option<PhotoFiles>,
    ) -> Result<Profile, ApiError> {
        let channel = self
            .db(move |store| store.channel_of(account))
            .await?
            .ok_or(ApiError::NotFound)?;
        let version = photo.as_ref().map(|photo| photo.version.clone());
        let old = self
            .db(move |store| store.set_photo(account, photo.as_ref()))
            .await?;
        if old != version {
            photo::remove(self, channel, old).await;
        }
        self.profile(account).await
    }
}

impl Handle<Update> for Site {
    async fn handle(&self, caller: Caller, change: Customization) -> Result<Profile, ApiError> {
        let account = caller.signed_in()?;
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
            store.edit_profile(account, &edit)
        })
        .await?
        .ok_or(ApiError::NotFound)
    }
}

impl Handle<SetPhoto> for Site {
    async fn handle(&self, caller: Caller, new: NewPhoto) -> Result<Profile, ApiError> {
        let account = caller.signed_in()?;
        let channel = self
            .db(move |store| store.channel_of(account))
            .await?
            .ok_or(ApiError::NotFound)?;
        let made = photo::make(self, channel, &new.image).await?;
        self.replace_photo(account, Some(made)).await
    }
}

impl Handle<RemovePhoto> for Site {
    async fn handle(&self, caller: Caller, _: Nothing) -> Result<Profile, ApiError> {
        let account = caller.signed_in()?;
        self.replace_photo(account, None).await
    }
}
