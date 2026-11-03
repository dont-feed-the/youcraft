//! The commands that bring a video's file to the site; the bytes themselves arrive over
//! `PATCH`, in `crate::uploads`.

use youcraft_api::commands::upload::{Create, Finish, NewUpload, Status};
use youcraft_api::commands::video::Which;
use youcraft_api::records::{Processing, Upload};
use youcraft_api::{ApiError, VideoId, routes};
use youcraft_store::uploads::NewUpload as NewRow;

use crate::dispatch::Handle;
use crate::handlers::studio::{description, may_manage, title};
use crate::site::{Caller, Site, now};
use crate::storage;

/// The most bytes one `PATCH` of an upload may carry.
pub const PIECE: u64 = 16 << 20;

impl Handle<Create> for Site {
    async fn handle(&self, caller: Caller, input: NewUpload) -> Result<Upload, ApiError> {
        let account = caller.account.ok_or(ApiError::SignInRequired)?;
        let (title, description) = (title(&input.title)?, description(&input.description)?);
        if input.size == 0 {
            return Err(ApiError::invalid("size", "a file with bytes"));
        }
        let channel = self
            .db(move |store| store.channel_of(account))
            .await?
            .ok_or(ApiError::Forbidden)?;
        // Measuring the room and claiming it happen under one lock, so that two uploads
        // cannot both take the same room.
        let _writing = self.writing.lock().await;
        let limit = self.config().upload_bytes.min(storage::room(self).await?);
        if input.size > limit {
            return Err(ApiError::TooLarge { limit });
        }
        let video = VideoId::from_bits(crate::secrets::random_bits());
        let visibility = input.visibility;
        let size = input.size;
        self.db(move |store| {
            let row = NewRow {
                video,
                channel,
                title: &title,
                description: &description,
                visibility,
                size,
                now: now(),
            };
            store.create_upload(&row)
        })
        .await?;
        Ok(Upload {
            video,
            address: routes::upload(video),
            piece: PIECE,
            received: 0,
        })
    }
}

impl Handle<Status> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Upload, ApiError> {
        may_manage(self, caller, which.video).await?;
        let video = which.video;
        let receiving = self
            .db(move |store| store.receiving(video))
            .await?
            .ok_or(ApiError::Conflict)?;
        Ok(Upload {
            video,
            address: routes::upload(video),
            piece: PIECE,
            received: receiving.received,
        })
    }
}

impl Handle<Finish> for Site {
    async fn handle(&self, caller: Caller, which: Which) -> Result<Processing, ApiError> {
        may_manage(self, caller, which.video).await?;
        let video = which.video;
        let _writing = self.writing.lock().await;
        let config = self.config();
        let (from, to) = (
            config.uploads().join(video.to_string()),
            config.originals().join(video.to_string()),
        );
        let receiving = self
            .db(move |store| store.receiving(video))
            .await?
            .ok_or(ApiError::Conflict)?;
        if receiving.received != receiving.size {
            return Err(ApiError::Conflict);
        }
        tokio::fs::rename(&from, &to).await.map_err(|e| {
            tracing::error!("upload {video}: {e}");
            ApiError::Internal
        })?;
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
