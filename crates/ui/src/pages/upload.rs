//! The upload page: choose a file, and while it is sent, give it a title, a description and a
//! visibility, as YouTube's upload dialog does.
//!
//! The video is created the moment the file is chosen, so the details can be saved while the
//! bytes travel; when the last piece has arrived, the details are saved once more and the file is
//! handed over to be made playable, and the page follows it until it plays. The sending outlives
//! the page: a person who goes elsewhere meanwhile still gets their video, with the details as
//! they last stood, and the studio shows how far it has come.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::video;
use youcraft_api::records::{Processing, Visibility};
use youcraft_api::{VideoId, routes};

use crate::act::{Scope, act};
use crate::app::titled;
use crate::browser::Timer;
use crate::icons::{Glyph, Icon};
use crate::lang::tr;
use crate::pages::studio::{DetailFields, processing_text};

/// Where the upload is; only the browser moves it past [`Stage::Choosing`].
#[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Stage {
    /// No file yet.
    Choosing,
    /// `sent` bytes of `size` have arrived.
    Sending { sent: u64, size: u64 },
    /// All of it arrived; it is on its way to playing, as the page's `processing` says.
    Processing,
    /// It failed, for the reason shown.
    Failed(String),
}

/// What the page shows of an upload, and the details the person gives it.
#[derive(Clone, Copy)]
struct Upload {
    stage: RwSignal<Stage>,
    processing: RwSignal<Processing>,
    video: RwSignal<Option<VideoId>>,
    title: RwSignal<String>,
    description: RwSignal<String>,
    visibility: RwSignal<Visibility>,
}

impl Upload {
    /// The details as the page has them, for `video`; `None` once the page is gone.
    fn edit(self, video: VideoId) -> Option<video::Edit> {
        Some(video::Edit {
            video,
            title: Some(self.title.try_get_untracked()?),
            description: Some(self.description.try_get_untracked()?),
            visibility: Some(self.visibility.try_get_untracked()?),
            comments_enabled: None,
        })
    }
}

/// How long the page waits before it asks again how far a handed-over video has come.
#[cfg(feature = "hydrate")]
const FOLLOW_EVERY: std::time::Duration = std::time::Duration::from_secs(3);

/// The upload page.
#[component]
pub fn UploadPage() -> impl IntoView {
    let upload = Upload {
        stage: RwSignal::new(Stage::Choosing),
        processing: RwSignal::new(Processing::Processing { permille: 0 }),
        video: RwSignal::new(None),
        title: RwSignal::new(String::new()),
        description: RwSignal::new(String::new()),
        visibility: RwSignal::new(Visibility::Private),
    };
    let Upload {
        stage,
        processing,
        video,
        title,
        description,
        visibility,
    } = upload;
    let saving = RwSignal::new(false);
    let save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let Some(edit) = video.get_untracked().and_then(|video| upload.edit(video)) else {
            return;
        };
        if saving.get_untracked() {
            return;
        }
        saving.set(true);
        act::<video::Update>(edit, move |_| saving.set(false));
    };
    let follow = Timer::new();
    let scope = Scope::current();
    let choose = move |event: leptos::ev::Event| {
        #[cfg(feature = "hydrate")]
        if let Some(file) = crate::transfer::chosen_file(&event) {
            start(file, upload, follow, scope.clone());
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = (event, follow, &scope);
    };
    view! {
        <Title text=titled(tr("Upload video"))/>
        <main class="upload">
            <h1>{tr("Upload video")}</h1>
            <Show when=move || stage.get() == Stage::Choosing>
                <label class="drop">
                    <Glyph icon=Icon::Upload/>
                    <span>{tr("Choose a video file to upload.")}</span>
                    <span class="pill accent">{tr("Select file")}</span>
                    <input class="file" type="file" accept="video/*" on:change=choose.clone()/>
                </label>
            </Show>
            <Show when=move || stage.get() != Stage::Choosing>
                <div class="upload-progress" aria-live="polite">
                    {move || match stage.get() {
                        Stage::Sending { sent, size } => {
                            let text = processing_text(&Processing::Uploading { received: sent, size });
                            view! {
                                <progress max=size as f64 value=sent as f64></progress>
                                <p>{text}</p>
                            }.into_any()
                        }
                        Stage::Processing => {
                            let state = match processing_text(&processing.get()) {
                                Some(text) => view! { <p>{text}</p> }.into_any(),
                                None => {
                                    let watch = video.get().map(routes::watch);
                                    view! { <a class="pill accent" href=watch>{title.get()}</a> }.into_any()
                                }
                            };
                            view! {
                                {state}
                                <a class="pill" href=routes::STUDIO>{tr("Channel content")}</a>
                            }.into_any()
                        }
                        Stage::Failed(reason) => view! { <p class="failure">{reason}</p> }.into_any(),
                        Stage::Choosing => ().into_any(),
                    }}
                </div>
                <form class="editor" on:submit=save>
                    <DetailFields title description visibility/>
                    <div class="editor-actions">
                        <button class="pill accent" type="submit" disabled=move || video.get().is_none() || saving.get()>
                            {tr("Save")}
                        </button>
                    </div>
                </form>
            </Show>
        </main>
    }
}

/// Create the video for `file`, send its bytes, save the details, hand it over, and follow it
/// with `follow` for as long as the page is shown.
///
/// The sending is no part of the page: it runs to its end even if the person leaves, and reads
/// the page's signals only with the `try_` methods, which find nothing once the page is gone.
#[cfg(feature = "hydrate")]
fn start(file: web_sys::File, upload: Upload, follow: Timer, scope: Scope) {
    use youcraft_api::commands::upload as uploading;

    use crate::call::call;
    use crate::notice::describe;

    if upload.title.get_untracked().trim().is_empty() {
        let name = file.name();
        let stem = name
            .rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem);
        upload
            .title
            .set(stem.chars().take(video::TITLE_LIMIT).collect());
    }
    let size = file.size() as u64;
    upload.stage.set(Stage::Sending { sent: 0, size });
    let request = uploading::NewUpload {
        title: upload.title.get_untracked(),
        description: upload.description.get_untracked(),
        visibility: upload.visibility.get_untracked(),
        size,
    };
    leptos::task::spawn_local(async move {
        let done = async {
            let created = call::<uploading::Create>(request).await?;
            upload.video.set(Some(created.video));
            crate::transfer::send(&file, &created, |sent| {
                upload.stage.set(Stage::Sending { sent, size })
            })
            .await?;
            if let Some(edit) = upload.edit(created.video) {
                call::<video::Update>(edit).await?;
            }
            call::<uploading::Finish>(video::Which {
                video: created.video,
            })
            .await?;
            Ok(created.video)
        };
        match done.await {
            Ok(video) => {
                upload.stage.set(Stage::Processing);
                follow_progress(video, upload.processing, follow, scope);
            }
            Err(error) => upload.stage.set(Stage::Failed(describe(&error))),
        }
    });
}

/// Ask, every few seconds and only while the page is shown, how far `video` has come, until it
/// plays or fails; a failed question ends the following, and the studio's list still shows the
/// rest.
#[cfg(feature = "hydrate")]
fn follow_progress(video: VideoId, processing: RwSignal<Processing>, follow: Timer, scope: Scope) {
    use youcraft_api::commands::studio;

    use crate::pages::studio::on_its_way;

    follow.after(FOLLOW_EVERY, move || {
        let again = scope.clone();
        scope.ask::<studio::Video>(video::Which { video }, move |answer| {
            if let Ok(found) = answer {
                let following = on_its_way(&found.processing);
                processing.set(found.processing);
                if following {
                    follow_progress(video, processing, follow, again);
                }
            }
        });
    });
}
