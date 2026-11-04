//! The commands that bring a video's file to the site; the bytes themselves arrive over
//! `PATCH`, in `crate::uploads`.

use youcraft_api::commands::upload::{Create, Finish, NewUpload, Status};
use youcraft_api::commands::video::Which;
use youcraft_api::records::{Processing, Upload};
use youcraft_api::{ApiError, ChannelRole, Rule, VideoId, routes};
use youcraft_store::uploads::{Claim, NewUpload as NewRow};

use crate::dispatch::Handle;
use crate::handlers::studio::{description, title};
use crate::site::{Caller, Site, now};
use crate::storage;
use crate::uploads::open_upload;

/// The most bytes one `PATCH` of an upload may carry.
pub const PIECE: u64 = 16 << 20;

impl Handle<Create> for Site {
    async fn handle(&self, caller: Caller, input: NewUpload) -> Result<Upload, ApiError> {
        let account = caller.account.ok_or(ApiError::SignInRequired)?;
        let (title, description) = (title(&input.title)?, description(&input.description)?);
        if input.size == 0 {
            return Err(ApiError::invalid("size", Rule::Required));
        }
        let channel = input.channel;
        self.allow_in_channel(caller, channel, ChannelRole::Editor)
            .await?;
        let allowance = storage::allowance(self).await?;
        let video = VideoId::from_bits(crate::secrets::random_bits());
        let visibility = input.visibility;
        let size = input.size;
        let claim = self
            .db(move |store| {
                let row = NewRow {
                    video,
                    channel,
                    account,
                    title: &title,
                    description: &description,
                    visibility,
                    size,
                    now: now(),
                };
                store.create_upload(&row, &allowance)
            })
            .await?;
        match claim {
            Claim::Started => Ok(Upload {
                video,
                address: routes::upload(video),
                piece: PIECE,
                received: 0,
            }),
            Claim::Crowded => Err(ApiError::Conflict),
            Claim::TooLarge { limit } => Err(ApiError::TooLarge { limit }),
        }
    }
}

impl Handle<Status> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Upload, ApiError> {
        let receiving = open_upload(self, caller, which.video).await?;
        Ok(Upload {
            video: which.video,
            address: routes::upload(which.video),
            piece: PIECE,
            received: receiving.received,
        })
    }
}

impl Handle<Finish> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Processing, ApiError> {
        let video = which.video;
        let _held = self.lock_upload(video).await?;
        let receiving = open_upload(self, caller, video).await?;
        if receiving.received != receiving.size {
            return Err(ApiError::Conflict);
        }
        let config = self.config();
        let (from, to) = (
            config.uploads().join(video.to_string()),
            config.originals().join(video.to_string()),
        );
        // The file moves before the row, so a crash between them leaves the file whole in
        // `originals/` and the upload still open: finishing again finds it there.
        if let Err(e) = tokio::fs::rename(&from, &to).await {
            let moved = e.kind() == std::io::ErrorKind::NotFound
                && tokio::fs::try_exists(&to).await.unwrap_or(false);
            if !moved {
                tracing::error!("upload {video}: {e}");
                return Err(ApiError::Internal);
            }
        }
        if !self
            .db(move |store| store.finish_upload(video, now()))
            .await?
        {
            return Err(ApiError::Conflict);
        }
        self.work.notify_one();
        Ok(Processing::Processing { permille: 0 })
    }
}
