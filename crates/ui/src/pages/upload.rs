//! The upload page: choose a file, and while it is sent, give it a title, a description and a
//! visibility, as YouTube's upload dialog does.
//!
//! The video is created the moment the file is chosen, so the details can be saved while the
//! bytes travel; when the last piece has arrived, the details are saved once more and the file is
//! handed over to be made playable.

use leptos::prelude::*;
use leptos_meta::Title;
use youcraft_api::commands::video;
use youcraft_api::records::Visibility;
use youcraft_api::{VideoId, routes};

use crate::call::call;
use crate::icons::{Glyph, Icon};
use crate::lang::tr;
use crate::pages::studio::{processing_text, visibility_name};

/// Where the upload is; only the browser moves it past [`Stage::Choosing`].
#[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// No file yet.
    Choosing,
    /// `sent` bytes of `size` have arrived.
    Sending { sent: u64, size: u64 },
    /// All of it arrived; it is being made playable.
    Processing,
    /// It failed, for the reason shown.
    Failed(&'static str),
}

/// What a failed command means to the person uploading.
#[cfg(feature = "hydrate")]
fn failure(error: &youcraft_api::ApiError) -> &'static str {
    use youcraft_api::ApiError;
    match error {
        ApiError::TooLarge { .. } => tr("The file is too large."),
        ApiError::SignInRequired => tr("Sign in to see this page."),
        ApiError::Forbidden => tr("You can't see this page."),
        _ => tr("Upload failed. Try again."),
    }
}

/// The upload page.
#[component]
pub fn UploadPage() -> impl IntoView {
    let stage = RwSignal::new(Stage::Choosing);
    let video = RwSignal::new(None::<VideoId>);
    let title = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let visibility = RwSignal::new(Visibility::Private.as_str().to_owned());
    let edit = move |video: VideoId| video::Edit {
        video,
        title: Some(title.get_untracked()),
        description: Some(description.get_untracked()),
        visibility: Visibility::from_name(&visibility.get_untracked()),
        comments_enabled: None,
    };
    let save = Action::new(move |(): &()| {
        let change = video.get_untracked().map(edit);
        async move {
            match change {
                Some(change) => call::<video::Update>(change).await.map(|_| ()),
                None => Ok(()),
            }
        }
    });
    let choose = move |event: leptos::ev::Event| {
        #[cfg(feature = "hydrate")]
        if let Some(file) = crate::transfer::chosen_file(&event) {
            start(file, stage, video, title, description, visibility, edit);
        }
        #[cfg(not(feature = "hydrate"))]
        let _ = event;
    };
    let options = [
        Visibility::Public,
        Visibility::Unlisted,
        Visibility::Private,
    ];
    view! {
        <Title text=tr("Upload video")/>
        <main class="upload">
            <h1>{tr("Upload video")}</h1>
            <Show when=move || stage.get() == Stage::Choosing>
                <label class="drop">
                    <Glyph icon=Icon::Upload/>
                    <span>{tr("Choose a video file to upload.")}</span>
                    <span class="pill accent">{tr("Select file")}</span>
                    <input class="file" type="file" accept="video/*" on:change=choose/>
                </label>
            </Show>
            <Show when=move || stage.get() != Stage::Choosing>
                <div class="upload-progress" aria-live="polite">
                    {move || match stage.get() {
                        Stage::Sending { sent, size } => {
                            let text = processing_text(&youcraft_api::records::Processing::Uploading { received: sent, size });
                            view! {
                                <progress max=size as f64 value=sent as f64></progress>
                                <p>{text}</p>
                            }.into_any()
                        }
                        Stage::Processing => view! {
                            <p>{processing_text(&youcraft_api::records::Processing::Processing { permille: 0 })}</p>
                            <a class="pill" href=routes::STUDIO>{tr("Channel content")}</a>
                        }.into_any(),
                        Stage::Failed(reason) => view! { <p class="failure">{reason}</p> }.into_any(),
                        Stage::Choosing => ().into_any(),
                    }}
                </div>
                <form class="editor" on:submit=move |event| { event.prevent_default(); save.dispatch(()); }>
                    <label class="field">
                        <span>{tr("Title")}</span>
                        <input type="text" maxlength="100" required bind:value=title/>
                    </label>
                    <label class="field">
                        <span>{tr("Description")}</span>
                        <textarea rows="5" maxlength="5000" bind:value=description></textarea>
                    </label>
                    <label class="field">
                        <span>{tr("Visibility")}</span>
                        <select bind:value=visibility>
                            {options.map(|v| view! { <option value=v.as_str()>{visibility_name(v)}</option> }).collect_view()}
                        </select>
                    </label>
                    <div class="editor-actions">
                        <button class="pill accent" type="submit" disabled=move || video.get().is_none() || save.pending().get()>
                            {tr("Save")}
                        </button>
                    </div>
                </form>
            </Show>
        </main>
    }
}

/// Create the video for `file`, send its bytes, save the details and hand it over.
#[cfg(feature = "hydrate")]
fn start(
    file: web_sys::File,
    stage: RwSignal<Stage>,
    video: RwSignal<Option<VideoId>>,
    title: RwSignal<String>,
    description: RwSignal<String>,
    visibility: RwSignal<String>,
    edit: impl Fn(VideoId) -> video::Edit + 'static,
) {
    use youcraft_api::commands::upload;

    if title.get_untracked().trim().is_empty() {
        let name = file.name();
        let stem = name
            .rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem);
        title.set(stem.chars().take(100).collect());
    }
    let size = file.size() as u64;
    stage.set(Stage::Sending { sent: 0, size });
    leptos::task::spawn_local(async move {
        let request = upload::NewUpload {
            title: title.get_untracked(),
            description: description.get_untracked(),
            visibility: Visibility::from_name(&visibility.get_untracked()).unwrap_or_default(),
            size,
        };
        let done = async {
            let created = call::<upload::Create>(request).await?;
            video.set(Some(created.video));
            crate::transfer::send(&file, &created, |sent| {
                stage.set(Stage::Sending { sent, size })
            })
            .await?;
            call::<video::Update>(edit(created.video)).await?;
            call::<upload::Finish>(video::Which {
                video: created.video,
            })
            .await
        };
        stage.set(match done.await {
            Ok(_) => Stage::Processing,
            Err(error) => Stage::Failed(failure(&error)),
        });
    });
}
